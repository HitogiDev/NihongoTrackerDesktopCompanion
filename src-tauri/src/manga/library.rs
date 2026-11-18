//! Local library roots: folders the user added, scanned into series and
//! volumes.
//!
//! What counts as what:
//!
//! * a folder with images is a volume (its pages, in natural order);
//! * a `.cbz` or `.zip` is a volume;
//! * `<Vol>.mokuro` next to `<Vol>.cbz` or to the folder `<Vol>/` gives that
//!   volume its text;
//! * the folder a volume is in is its series (the root itself, if that is
//!   where the volumes are).
//!
//! Sidecar images (`<Vol>.webp` next to `<Vol>.cbz`, `cover.jpg`) are covers,
//! not pages. A root that is missing (an unplugged drive) is skipped, never
//! emptied: its series come back as they were when it does.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use super::store::{SeriesRecord, VolumeRecord, FORMAT_ARCHIVE, FORMAT_FOLDER, SOURCE_LOCAL};
use super::{archive, is_image_name, natural_cmp, thumbs};
use crate::state::AppState;

pub const LIBRARY_CHANGED: &str = "manga://library-changed";

/// Deeper than this is not a manga library (and stops a link loop for good).
const MAX_DEPTH: usize = 8;
/// Opening the Manga screen rescans, but not more often than this.
const AUTO_RESCAN_EVERY: Duration = Duration::from_secs(60);

/// Folders that are never volumes nor series.
pub(crate) fn skipped_dir(name: &str) -> bool {
    name.starts_with('.')
        || name.eq_ignore_ascii_case("__MACOSX")
        || name.eq_ignore_ascii_case("_ocr")
        || name.eq_ignore_ascii_case("$RECYCLE.BIN")
        || name.eq_ignore_ascii_case("System Volume Information")
}

pub(crate) fn is_archive_name(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("cbz") || ext.eq_ignore_ascii_case("zip"))
}

pub(crate) fn stem(name: &str) -> &str {
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
}

/// `cover.jpg`, `folder.png`, `poster.webp`: what media managers call a cover.
pub(crate) fn is_cover_name(name: &str) -> bool {
    is_image_name(name) && ["cover", "folder", "poster"].iter().any(|known| stem(name).eq_ignore_ascii_case(known))
}

const NAMESPACE: uuid::Uuid = uuid::Uuid::from_u128(0x6d61_6e67_612d_4e54_9c1d_2a7e_3b4f_5a60);

/// The id of whatever lives at `path`: the same every scan, and the same if it
/// disappears and comes back, so its progress and its link survive.
pub fn stable_id(kind: &str, path: &Path) -> String {
    uuid::Uuid::new_v5(&NAMESPACE, format!("{kind}\0{}", path.to_string_lossy()).as_bytes()).to_string()
}

fn signature_of(meta: &std::fs::Metadata) -> (u64, u64) {
    let modified = meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
    (meta.len(), modified)
}

#[derive(Default)]
struct Listing {
    dirs: Vec<(String, PathBuf)>,
    images: Vec<(String, std::fs::Metadata)>,
    archives: Vec<(String, PathBuf, std::fs::Metadata)>,
    mokuro: HashMap<String, PathBuf>,
}

fn list(dir: &Path) -> std::io::Result<Listing> {
    let mut listing = Listing::default();
    for entry in std::fs::read_dir(dir)? {
        let Ok(entry) = entry else { continue };
        let Some(name) = entry.file_name().to_str().map(str::to_string) else { continue };
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        // Follows links on purpose (libraries are often linked in); the depth
        // limit and the visited set stop loops.
        let Ok(meta) = std::fs::metadata(&path) else { continue };
        if meta.is_dir() {
            if !skipped_dir(&name) {
                listing.dirs.push((name, path));
            }
        } else if meta.is_file() {
            if is_image_name(&name) {
                listing.images.push((name, meta));
            } else if is_archive_name(&name) {
                listing.archives.push((name, path, meta));
            } else if name.rsplit_once('.').is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("mokuro")) {
                listing.mokuro.insert(stem(&name).to_lowercase(), path);
            }
        }
    }
    listing.dirs.sort_by(|a, b| natural_cmp(&a.0, &b.0));
    listing.images.sort_by(|a, b| natural_cmp(&a.0, &b.0));
    listing.archives.sort_by(|a, b| natural_cmp(&a.0, &b.0));
    Ok(listing)
}

/// What a previous scan stored, so unchanged archives are not reopened.
pub type Known = HashMap<String, (String, Vec<String>)>;

struct Walker<'a> {
    known: &'a Known,
    visited: HashSet<PathBuf>,
    /// Series folder → (cover, volumes found in it).
    series: BTreeMap<PathBuf, (Option<String>, Vec<VolumeRecord>)>,
}

impl Walker<'_> {
    fn walk(&mut self, dir: &Path, parent: Option<&Path>, depth: usize) {
        let Ok(canonical) = dir.canonicalize() else { return };
        if !self.visited.insert(canonical) {
            return;
        }
        let Ok(listing) = list(dir) else { return };

        // Images that are covers of something else here are not pages.
        let sidecars: HashSet<String> = listing
            .archives
            .iter()
            .map(|(name, ..)| stem(name).to_lowercase())
            .chain(listing.dirs.iter().map(|(name, _)| name.to_lowercase()))
            .collect();
        let is_sidecar = |name: &str| is_cover_name(name) || sidecars.contains(&stem(name).to_lowercase());

        let own_cover = listing.images.iter().find(|(name, _)| is_cover_name(name)).map(|(name, _)| dir.join(name));
        let first_sidecar = listing
            .images
            .iter()
            .find(|(name, _)| !is_cover_name(name) && sidecars.contains(&stem(name).to_lowercase()))
            .map(|(name, _)| dir.join(name));

        for (name, path, meta) in &listing.archives {
            let id = stable_id("local", path);
            let (len, modified) = signature_of(meta);
            let signature = format!("{len}-{modified}");
            let pages = match self.known.get(&id) {
                Some((known, pages)) if *known == signature => pages.clone(),
                _ => match archive::list_pages_at(path) {
                    Ok(pages) => pages,
                    Err(error) => {
                        log::info!("skipping manga archive {}: {error}", path.display());
                        continue;
                    }
                },
            };
            if pages.is_empty() {
                continue;
            }
            let mokuro = listing.mokuro.get(&stem(name).to_lowercase()).map(|p| p.to_string_lossy().into_owned());
            self.add(dir, own_cover.clone().or_else(|| first_sidecar.clone()), VolumeRecord {
                id,
                title: stem(name).to_string(),
                source_ref: path.to_string_lossy().into_owned(),
                format: FORMAT_ARCHIVE,
                position: 0,
                pages,
                signature,
                mokuro_path: mokuro,
            });
        }

        let pages: Vec<&(String, std::fs::Metadata)> = listing.images.iter().filter(|(name, _)| !is_sidecar(name)).collect();
        if !pages.is_empty() {
            let series_dir = parent.unwrap_or(dir);
            let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let (total, newest) = pages.iter().fold((0u64, 0u64), |(total, newest), (_, meta)| {
                let (len, modified) = signature_of(meta);
                (total + len, newest.max(modified))
            });
            let mokuro = parent
                .map(|parent| parent.join(format!("{name}.mokuro")))
                .filter(|path| path.is_file())
                .map(|path| path.to_string_lossy().into_owned());
            let cover = parent.and_then(|parent| {
                std::fs::read_dir(parent).ok()?.filter_map(Result::ok).map(|e| e.path()).find(|path| {
                    path.is_file() && path.file_name().and_then(|n| n.to_str()).is_some_and(is_cover_name)
                })
            });
            self.add(series_dir, cover, VolumeRecord {
                id: stable_id("local", dir),
                title: name,
                source_ref: dir.to_string_lossy().into_owned(),
                format: FORMAT_FOLDER,
                position: 0,
                pages: pages.iter().map(|(name, _)| name.clone()).collect(),
                signature: format!("{}-{total}-{newest}", pages.len()),
                mokuro_path: mokuro,
            });
        }

        if depth < MAX_DEPTH {
            for (_, child) in &listing.dirs {
                self.walk(child, Some(dir), depth + 1);
            }
        }
    }

    fn add(&mut self, series_dir: &Path, cover: Option<PathBuf>, volume: VolumeRecord) {
        let entry = self.series.entry(series_dir.to_path_buf()).or_insert((None, Vec::new()));
        if entry.0.is_none() {
            entry.0 = cover.map(|path| path.to_string_lossy().into_owned());
        }
        entry.1.push(volume);
    }
}

/// Every series under `root`. `known` lets unchanged archives skip reopening.
pub fn scan_root(root: &Path, known: &Known) -> Vec<SeriesRecord> {
    let mut walker = Walker { known, visited: HashSet::new(), series: BTreeMap::new() };
    walker.walk(root, None, 0);
    let mut found: Vec<SeriesRecord> = walker
        .series
        .into_iter()
        .map(|(dir, (cover, mut volumes))| {
            volumes.sort_by(|a, b| natural_cmp(&a.title, &b.title));
            for (index, volume) in volumes.iter_mut().enumerate() {
                volume.position = index as i64 + 1;
            }
            let title = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| dir.to_string_lossy().into_owned());
            SeriesRecord { id: stable_id("local-series", &dir), source_ref: dir.to_string_lossy().into_owned(), title, cover, volumes }
        })
        .collect();
    found.sort_by(|a, b| natural_cmp(&a.title, &b.title));
    found
}

/// The outcome of a rescan, for the toast.
#[derive(Debug, Clone, Default, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct ScanSummary {
    pub series: usize,
    pub volumes: usize,
    /// Roots that could not be read (a drive that is not plugged in).
    pub offline: Vec<String>,
    /// Another scan was running, or one ran a moment ago.
    pub skipped: bool,
}

/// Rescans every local root. `force` is the user's button; without it a scan
/// that ran in the last minute is enough.
pub async fn rescan(app: &AppHandle, force: bool) -> Result<ScanSummary, String> {
    let state = app.state::<AppState>();
    {
        let last = state.manga.last_scan.lock().unwrap();
        if !force && last.is_some_and(|at| at.elapsed() < AUTO_RESCAN_EVERY) {
            return Ok(ScanSummary { skipped: true, ..Default::default() });
        }
    }
    if state.manga.scanning.swap(true, Ordering::AcqRel) {
        return Ok(ScanSummary { skipped: true, ..Default::default() });
    }
    let db = state.db.clone();
    let cache_dir = app.path().app_cache_dir().ok();
    let result = tauri::async_runtime::spawn_blocking(move || -> Result<ScanSummary, String> {
        let mut summary = ScanSummary::default();
        for source in db.manga_sources().map_err(|e| e.to_string())? {
            if source.kind != SOURCE_LOCAL {
                continue;
            }
            let root = Path::new(&source.url);
            if !root.is_dir() {
                summary.offline.push(source.name.clone());
                continue;
            }
            let known = db.manga_known_volumes(&source.id).unwrap_or_default();
            let found = scan_root(root, &known);
            let changes = db.apply_manga_scan(&source.id, SOURCE_LOCAL, &found).map_err(|e| e.to_string())?;
            if let Some(cache_dir) = &cache_dir {
                thumbs::purge(cache_dir, &changes.stale_volumes);
            }
            summary.series += changes.series;
            summary.volumes += changes.volumes;
        }
        Ok(summary)
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|summary| summary);
    state.manga.scanning.store(false, Ordering::Release);
    *state.manga.last_scan.lock().unwrap() = Some(Instant::now());
    let _ = app.emit(LIBRARY_CHANGED, ());
    result
}

/// Why a folder cannot be added, as a code the UI translates.
pub fn check_new_root(path: &Path, existing: &[PathBuf]) -> Result<(), &'static str> {
    let canonical = path.canonicalize().map_err(|_| "not-a-folder")?;
    if !canonical.is_dir() {
        return Err("not-a-folder");
    }
    for other in existing {
        let other = other.canonicalize().unwrap_or_else(|_| other.clone());
        if canonical.starts_with(&other) || other.starts_with(&canonical) {
            return Err("already-added");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tree() -> PathBuf {
        std::env::temp_dir().join(format!("ntdc-lib-{}", uuid::Uuid::new_v4()))
    }

    fn touch(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    fn cbz(path: &Path, pages: &[&str]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut writer = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for page in pages {
            writer.start_file(*page, zip::write::SimpleFileOptions::default()).unwrap();
            writer.write_all(b"img").unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn folders_archives_and_mokuro_files_become_series_and_volumes() {
        let root = tree();
        // An image-folder series with mokuro text for its first volume.
        touch(&root.join("Yotsuba/Vol 10/001.jpg"), b"x");
        touch(&root.join("Yotsuba/Vol 2/002.jpg"), b"x");
        touch(&root.join("Yotsuba/Vol 2/001.jpg"), b"x");
        touch(&root.join("Yotsuba/Vol 2.mokuro"), b"{}");
        touch(&root.join("Yotsuba/cover.jpg"), b"x");
        touch(&root.join("Yotsuba/_ocr/Vol 2/001.json"), b"{}");
        // A bunko-style series: archive + mokuro + sidecar cover.
        cbz(&root.join("Frieren/Vol 1.cbz"), &["10.png", "2.png", "__MACOSX/._2.png"]);
        touch(&root.join("Frieren/Vol 1.mokuro"), b"{}");
        touch(&root.join("Frieren/Vol 1.webp"), b"x");
        cbz(&root.join("Frieren/Broken.cbz"), &[]);
        touch(&root.join("Frieren/notes.txt"), b"x");
        touch(&root.join(".trash/Vol 1/001.jpg"), b"x");

        let found = scan_root(&root, &Known::new());
        let titles: Vec<_> = found.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["Frieren", "Yotsuba"]);

        let frieren = &found[0];
        assert_eq!(frieren.volumes.len(), 1);
        assert_eq!(frieren.volumes[0].format, FORMAT_ARCHIVE);
        assert_eq!(frieren.volumes[0].pages, ["2.png", "10.png"]);
        assert!(frieren.volumes[0].mokuro_path.as_deref().unwrap().ends_with("Vol 1.mokuro"));
        assert!(frieren.cover.as_deref().unwrap().ends_with("Vol 1.webp"));

        let yotsuba = &found[1];
        let volumes: Vec<_> = yotsuba.volumes.iter().map(|v| (v.title.as_str(), v.position, v.mokuro_path.is_some())).collect();
        assert_eq!(volumes, [("Vol 2", 1, true), ("Vol 10", 2, false)]);
        assert_eq!(yotsuba.volumes[0].pages, ["001.jpg", "002.jpg"]);
        assert!(yotsuba.cover.as_deref().unwrap().ends_with("cover.jpg"));

        // Same place, same ids.
        let again = scan_root(&root, &Known::new());
        assert_eq!(again[1].id, yotsuba.id);
        assert_eq!(again[1].volumes[0].id, yotsuba.volumes[0].id);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_root_full_of_archives_is_one_series() {
        let root = tree().join("Oneshots");
        cbz(&root.join("b.cbz"), &["1.jpg"]);
        cbz(&root.join("a.zip"), &["1.jpg"]);
        let found = scan_root(&root, &Known::new());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].title, "Oneshots");
        assert_eq!(found[0].volumes.iter().map(|v| v.title.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        let _ = std::fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn unchanged_archives_reuse_their_page_list() {
        let root = tree();
        let path = root.join("S/v.cbz");
        cbz(&path, &["1.jpg"]);
        let first = scan_root(&root, &Known::new());
        let volume = &first[0].volumes[0];
        let mut known = Known::new();
        known.insert(volume.id.clone(), (volume.signature.clone(), vec!["remembered.jpg".into()]));
        assert_eq!(scan_root(&root, &known)[0].volumes[0].pages, ["remembered.jpg"]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn overlapping_roots_are_refused() {
        let root = tree();
        std::fs::create_dir_all(root.join("a/b")).unwrap();
        let existing = vec![root.join("a")];
        assert_eq!(check_new_root(&root.join("a/b"), &existing), Err("already-added"));
        assert_eq!(check_new_root(&root, &existing), Err("already-added"));
        assert_eq!(check_new_root(&root.join("missing"), &existing), Err("not-a-folder"));
        std::fs::create_dir_all(root.join("c")).unwrap();
        assert!(check_new_root(&root.join("c"), &existing).is_ok());
        let _ = std::fs::remove_dir_all(root);
    }
}
