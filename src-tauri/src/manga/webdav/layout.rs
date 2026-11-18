//! What a WebDAV listing means as a manga library. The same rules as a local
//! folder (`library`), on listings instead of a disk:
//!
//! * a `mokuro-reader/` folder at the address the user gave is the library
//!   (mokuro-bunko's layout); otherwise the address itself is;
//! * each folder in the library is a series, and the library itself is one if
//!   it holds volumes directly;
//! * in a series, `<Vol>.cbz`/`.zip` is a volume, `<Vol>.mokuro` its text,
//!   `<Vol>.<layer>.mokuro` an alternative text, an image named `<Vol>` its
//!   cover, and `cover.*` the series' cover;
//! * a folder of images inside a series is a volume too.
//!
//! Pure functions over [`Entry`] lists, so all of it is tested without a server.

use std::collections::HashMap;

use super::propfind::Entry;
use crate::manga::library::{is_archive_name, is_cover_name, skipped_dir, stem};
use crate::manga::{is_image_name, natural_cmp};

/// mokuro-bunko serves its library from this folder at the root.
pub const LIBRARY_FOLDER: &str = "mokuro-reader";

fn is_mokuro_name(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("mokuro"))
}

/// The folder whose subfolders are series: `mokuro-reader/` if `root` has one.
pub fn library_base(root_path: &str, root: &[Entry]) -> String {
    root.iter()
        .find(|entry| entry.is_dir && entry.name.eq_ignore_ascii_case(LIBRARY_FOLDER))
        .map_or_else(|| root_path.to_string(), |entry| entry.path.clone())
}

/// A folder of the library that is (or may be) a series.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeriesFolder {
    pub path: String,
    pub title: String,
    /// The folder's ETag or date, when the server gives one: a series whose
    /// folder did not change does not need listing again.
    pub signature: Option<String>,
}

/// The series in the library base. `title` names the base itself when it
/// holds volumes directly (a flat folder of CBZs is one series).
pub fn series_folders(base_path: &str, entries: &[Entry], base_title: &str) -> Vec<SeriesFolder> {
    let mut series: Vec<SeriesFolder> = entries
        .iter()
        .filter(|entry| entry.is_dir && !skipped_dir(&entry.name))
        .map(|entry| SeriesFolder { path: entry.path.clone(), title: entry.name.clone(), signature: entry.signature() })
        .collect();
    if entries.iter().any(|entry| !entry.is_dir && is_archive_name(&entry.name)) {
        series.push(SeriesFolder { path: base_path.to_string(), title: base_title.to_string(), signature: None });
    }
    series.sort_by(|a, b| natural_cmp(&a.title, &b.title));
    series
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeKind {
    Archive,
    Folder,
}

/// A volume as the listings describe it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteVolume {
    pub title: String,
    /// The archive, or the folder (with its trailing `/`).
    pub path: String,
    pub kind: VolumeKind,
    pub size: Option<u64>,
    pub signature: Option<String>,
    /// `<Vol>.mokuro`.
    pub mokuro: Option<String>,
    /// `<Vol>.<layer>.mokuro`, as `(layer, path)` in natural order.
    pub layers: Vec<(String, String)>,
    /// An image named like the volume, next to it.
    pub cover: Option<String>,
    /// A folder's page names; an archive's are only known once its central
    /// directory is read.
    pub pages: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SeriesContents {
    pub volumes: Vec<RemoteVolume>,
    /// `cover.*` in the series folder, else the first volume's cover.
    pub cover: Option<String>,
}

/// Subfolders of a series that may be image-folder volumes: they need their
/// own listing before [`series_contents`] can tell.
pub fn folder_candidates(entries: &[Entry]) -> Vec<String> {
    entries.iter().filter(|entry| entry.is_dir && !skipped_dir(&entry.name)).map(|entry| entry.path.clone()).collect()
}

/// The volumes of a series folder. `subfolders` holds the listing of each of
/// its [`folder_candidates`] that could be listed.
pub fn series_contents(entries: &[Entry], subfolders: &HashMap<String, Vec<Entry>>) -> SeriesContents {
    let mut volumes: Vec<RemoteVolume> = Vec::new();
    for entry in entries {
        if !entry.is_dir && is_archive_name(&entry.name) {
            volumes.push(RemoteVolume {
                title: stem(&entry.name).to_string(),
                path: entry.path.clone(),
                kind: VolumeKind::Archive,
                size: entry.size,
                signature: entry.signature(),
                mokuro: None,
                layers: Vec::new(),
                cover: None,
                pages: Vec::new(),
            });
        }
    }
    for entry in entries.iter().filter(|entry| entry.is_dir && !skipped_dir(&entry.name)) {
        let Some(listing) = subfolders.get(&entry.path) else { continue };
        let pages: Vec<&Entry> = listing.iter().filter(|e| !e.is_dir && is_image_name(&e.name) && !is_cover_name(&e.name)).collect();
        if pages.is_empty() {
            continue;
        }
        let total: u64 = pages.iter().filter_map(|page| page.size).sum();
        let mut names: Vec<String> = pages.iter().map(|page| page.name.clone()).collect();
        names.sort_by(|a, b| natural_cmp(a, b));
        volumes.push(RemoteVolume {
            title: entry.name.clone(),
            path: entry.path.clone(),
            kind: VolumeKind::Folder,
            size: Some(total).filter(|&total| total > 0),
            signature: Some(format!("{}-{total}-{}", pages.len(), entry.signature().unwrap_or_default())),
            mokuro: None,
            layers: Vec::new(),
            cover: None,
            pages: names,
        });
    }
    // Two volumes with the same name (`Vol 1.cbz` and `Vol 1/`): the archive.
    let mut seen = std::collections::HashSet::new();
    volumes.retain(|volume| seen.insert(volume.title.to_lowercase()));

    // Sidecars, matched to the volume they are named after, longest name
    // first so `Vol 1.5` does not take `Vol 1`'s files.
    let mut by_stem: Vec<(String, usize)> = volumes.iter().enumerate().map(|(i, v)| (v.title.to_lowercase(), i)).collect();
    by_stem.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    let mut series_cover = None;
    for entry in entries.iter().filter(|entry| !entry.is_dir) {
        let lower = entry.name.to_lowercase();
        if is_mokuro_name(&entry.name) {
            let name_stem = stem(&lower);
            if let Some(&(_, index)) = by_stem.iter().find(|(s, _)| s == name_stem) {
                volumes[index].mokuro = Some(entry.path.clone());
            } else if let Some((volume_stem, index)) = by_stem.iter().find(|(s, _)| name_stem.len() > s.len() + 1 && name_stem.starts_with(s.as_str()) && name_stem.as_bytes()[s.len()] == b'.') {
                let layer = stem(&entry.name)[volume_stem.len() + 1..].to_string();
                volumes[*index].layers.push((layer, entry.path.clone()));
            }
        } else if is_image_name(&entry.name) {
            if is_cover_name(&entry.name) {
                series_cover.get_or_insert_with(|| entry.path.clone());
            } else if let Some(&(_, index)) = by_stem.iter().find(|(s, _)| s == stem(&lower)) {
                volumes[index].cover.get_or_insert_with(|| entry.path.clone());
            }
        }
    }
    for volume in &mut volumes {
        volume.layers.sort_by(|a, b| natural_cmp(&a.0, &b.0));
    }
    volumes.sort_by(|a, b| natural_cmp(&a.title, &b.title));
    let cover = series_cover.or_else(|| volumes.iter().find_map(|volume| volume.cover.clone()));
    SeriesContents { volumes, cover }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, size: u64) -> Entry {
        Entry {
            name: path.rsplit('/').next().unwrap().to_string(),
            path: path.to_string(),
            is_dir: false,
            size: Some(size),
            etag: Some(format!("e-{size}")),
            modified: None,
        }
    }

    fn dir(path: &str) -> Entry {
        let trimmed = path.trim_end_matches('/');
        Entry { name: trimmed.rsplit('/').next().unwrap().to_string(), path: format!("{trimmed}/"), is_dir: true, size: None, etag: Some("d".into()), modified: None }
    }

    #[test]
    fn mokuro_reader_at_the_root_is_the_library() {
        let root = [dir("/mokuro-reader"), file("/index.html", 1)];
        assert_eq!(library_base("/", &root), "/mokuro-reader/");
        assert_eq!(library_base("/dav/", &[dir("/dav/Series")]), "/dav/");
    }

    #[test]
    fn folders_are_series_and_a_flat_folder_is_one_too() {
        let entries = [dir("/m/Yotsuba"), dir("/m/.trash"), dir("/m/__MACOSX"), file("/m/catalog.json", 9), dir("/m/Frieren")];
        let series = series_folders("/m/", &entries, "m");
        assert_eq!(series.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(), ["Frieren", "Yotsuba"]);
        assert_eq!(series[0].signature.as_deref(), Some("d"));

        let flat = [file("/m/b.cbz", 1), file("/m/a.cbz", 1)];
        let series = series_folders("/m/", &flat, "Oneshots");
        assert_eq!(series, [SeriesFolder { path: "/m/".into(), title: "Oneshots".into(), signature: None }]);
    }

    #[test]
    fn a_bunko_series_pairs_archives_text_layers_and_covers() {
        let s = "/mokuro-reader/Hoshi";
        let entries = [
            file(&format!("{s}/星 第10巻.cbz"), 600),
            file(&format!("{s}/星 第10巻.mokuro"), 5),
            file(&format!("{s}/星 第10巻.webp"), 2),
            file(&format!("{s}/星 第2巻.cbz"), 500),
            file(&format!("{s}/星 第2巻.mokuro"), 4),
            file(&format!("{s}/星 第2巻.edit.mokuro"), 4),
            file(&format!("{s}/星 第2巻.curated.mokuro"), 4),
            file(&format!("{s}/星 第2巻.webp"), 2),
            file(&format!("{s}/orphan.mokuro"), 1),
            file(&format!("{s}/notes.txt"), 1),
        ];
        let contents = series_contents(&entries, &HashMap::new());
        let titles: Vec<_> = contents.volumes.iter().map(|v| v.title.as_str()).collect();
        assert_eq!(titles, ["星 第2巻", "星 第10巻"]);
        let second = &contents.volumes[0];
        assert_eq!(second.kind, VolumeKind::Archive);
        assert_eq!(second.size, Some(500));
        assert_eq!(second.signature.as_deref(), Some("e-500"));
        assert_eq!(second.mokuro.as_deref(), Some("/mokuro-reader/Hoshi/星 第2巻.mokuro"));
        let layers: Vec<_> = second.layers.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(layers, ["curated", "edit"]);
        assert_eq!(second.cover.as_deref(), Some("/mokuro-reader/Hoshi/星 第2巻.webp"));
        assert!(contents.volumes[1].layers.is_empty());
        // No cover.* in the folder: the first volume's.
        assert_eq!(contents.cover.as_deref(), Some("/mokuro-reader/Hoshi/星 第2巻.webp"));
    }

    #[test]
    fn a_longer_volume_name_keeps_its_own_sidecars() {
        let entries = [file("/s/Vol 1.cbz", 1), file("/s/Vol 1.5.cbz", 1), file("/s/Vol 1.5.mokuro", 1), file("/s/Vol 1.mokuro", 1)];
        let contents = series_contents(&entries, &HashMap::new());
        let vol = |title: &str| contents.volumes.iter().find(|v| v.title == title).unwrap().clone();
        assert_eq!(vol("Vol 1").mokuro.as_deref(), Some("/s/Vol 1.mokuro"));
        assert!(vol("Vol 1").layers.is_empty());
        assert_eq!(vol("Vol 1.5").mokuro.as_deref(), Some("/s/Vol 1.5.mokuro"));
    }

    #[test]
    fn image_folders_are_volumes_once_listed() {
        let entries = [dir("/s/Vol 2"), dir("/s/Vol 1"), dir("/s/Empty"), file("/s/Vol 1.mokuro", 1), file("/s/cover.jpg", 1), file("/s/Vol 1.cbz", 1)];
        assert_eq!(folder_candidates(&entries), ["/s/Vol 2/", "/s/Vol 1/", "/s/Empty/"]);
        let mut subfolders = HashMap::new();
        subfolders.insert("/s/Vol 2/".to_string(), vec![file("/s/Vol 2/10.jpg", 3), file("/s/Vol 2/2.jpg", 3), file("/s/Vol 2/cover.jpg", 1), dir("/s/Vol 2/extra")]);
        subfolders.insert("/s/Vol 1/".to_string(), vec![file("/s/Vol 1/1.jpg", 3)]);
        subfolders.insert("/s/Empty/".to_string(), vec![file("/s/Empty/readme.txt", 1)]);
        let contents = series_contents(&entries, &subfolders);
        let summary: Vec<_> = contents.volumes.iter().map(|v| (v.title.as_str(), v.kind)).collect();
        // `Vol 1` is both an archive and a folder: the archive wins.
        assert_eq!(summary, [("Vol 1", VolumeKind::Archive), ("Vol 2", VolumeKind::Folder)]);
        let folder = &contents.volumes[1];
        assert_eq!(folder.pages, ["2.jpg", "10.jpg"]);
        assert_eq!(folder.size, Some(6));
        assert_eq!(contents.cover.as_deref(), Some("/s/cover.jpg"));
        assert_eq!(contents.volumes[0].mokuro.as_deref(), Some("/s/Vol 1.mokuro"));
    }
}
