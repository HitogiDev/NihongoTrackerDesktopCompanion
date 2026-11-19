//! Reading remote volumes: what the `manga://` protocol, the reader's text
//! and the mining crop ask for a volume that lives on a WebDAV server.
//!
//! Where a remote file comes from, first match wins:
//!
//! 1. the copy downloaded "for offline reading" (`app_data_dir/manga/offline/<volume>/`);
//! 2. the cache (`app_cache_dir/manga/webdav/`), including a whole archive
//!    downloaded because its server has no `Range`;
//! 3. the server: an archive by ranges (its central directory, then each
//!    entry), anything else whole, into the cache.
//!
//! Everything here blocks (it runs on the protocol's and the commands'
//! blocking threads) and is never called from the watcher's tick. A request
//! that fails because the server cannot be reached marks its source offline,
//! and the next one that works marks it online again.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use super::cache::Cache;
use super::client::{Client, DavError};
use super::ranged::RangedFile;
use super::sync::client_for;
use crate::db::Db;
use crate::manga::archive::{self, MAX_ENTRY_BYTES};
use crate::manga::library::LIBRARY_CHANGED;
use crate::manga::store::{VolumeLocation, FORMAT_REMOTE_ARCHIVE, FORMAT_REMOTE_FOLDER, FORMAT_SUWAYOMI_CHAPTER};
use crate::manga::suwayomi::graphql::SwError;

/// A `.mokuro` past this size is not one.
const MAX_TEXT_BYTES: u64 = 64 * 1024 * 1024;
/// Download progress, to every window.
pub const DOWNLOAD_EVENT: &str = "manga://download";

/// A download, as the UI shows it.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct DownloadProgress {
    pub volume_id: String,
    /// For offline reading (kept), or only to be able to read it (cached).
    pub offline: bool,
    pub received: u64,
    pub total: Option<u64>,
    pub done: bool,
    /// The error code, when it stopped on one.
    pub error: Option<String>,
}

/// What the cache holds, for the settings screen.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct WebdavCacheUsage {
    pub cache_bytes: u64,
    pub limit_bytes: u64,
    /// Volumes downloaded for offline reading (not part of the cache).
    pub offline_bytes: u64,
    pub offline_volumes: u64,
}

struct Download {
    cancel: Arc<AtomicBool>,
    progress: DownloadProgress,
}

pub struct Remote {
    db: Arc<Db>,
    pub cache: Cache,
    offline_root: PathBuf,
    app: Option<AppHandle>,
    downloads: Mutex<HashMap<String, Download>>,
    /// The extensions server, for chapters of series from extensions. Set
    /// once at start.
    pub suwayomi: std::sync::OnceLock<Arc<crate::manga::suwayomi::Service>>,
}

impl From<SwError> for DavError {
    fn from(error: SwError) -> Self {
        match error {
            SwError::Offline => DavError::Offline,
            SwError::Unauthorized => DavError::Unauthorized,
            SwError::NotFound => DavError::NotFound,
            other => DavError::Extension(other.code()),
        }
    }
}

/// `.cbz`, `.png`…: the extension of a remote name, if it is a plain one.
fn extension(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .filter(|ext| (1..=5).contains(&ext.len()) && ext.bytes().all(|b| b.is_ascii_alphanumeric()))
        .unwrap_or_else(|| "bin".into())
}

/// A cache key for a remote file in its current version.
fn key(source_id: &str, path: &str, signature: Option<&str>, extension: &str) -> String {
    let id = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, format!("{source_id}\0{path}\0{}", signature.unwrap_or("")).as_bytes());
    format!("{}.{extension}", id.simple())
}

impl Remote {
    pub fn new(db: Arc<Db>, cache_root: PathBuf, offline_root: PathBuf, limit_bytes: u64, app: Option<AppHandle>) -> Self {
        Remote {
            db,
            cache: Cache::new(cache_root, limit_bytes),
            offline_root,
            app,
            downloads: Mutex::new(HashMap::new()),
            suwayomi: std::sync::OnceLock::new(),
        }
    }

    pub(crate) fn client(&self, source_id: &str) -> Result<Client, DavError> {
        let row = self.db.manga_source_row(source_id).map_err(|e| DavError::Other(e.to_string()))?.ok_or(DavError::NotFound)?;
        client_for(&row)
    }

    /// Notes how a request to the source's server went: offline or refused
    /// until one works again. The library is told only when that changes.
    fn report<T>(&self, source_id: &str, result: &Result<T, DavError>) {
        let error = match result {
            Ok(_) => None,
            Err(error @ (DavError::Offline | DavError::Unauthorized)) => Some(error.code()),
            Err(_) => return,
        };
        if self.db.set_manga_source_status(source_id, error, false).unwrap_or(false) {
            if let Some(app) = &self.app {
                let _ = app.emit(LIBRARY_CHANGED, ());
            }
        }
    }

    pub(crate) fn block_on<T>(&self, source_id: &str, future: impl std::future::Future<Output = Result<T, DavError>>) -> Result<T, DavError> {
        let result = tauri::async_runtime::block_on(future);
        self.report(source_id, &result);
        result
    }

    // ── offline copies ─────────────────────────────────────────────────

    fn offline_dir(&self, volume_id: &str) -> Option<PathBuf> {
        crate::manga::protocol::is_opaque_id(volume_id).then(|| self.offline_root.join(volume_id))
    }

    fn offline_archive(&self, volume_id: &str) -> Option<PathBuf> {
        self.offline_dir(volume_id).map(|dir| dir.join("volume.cbz")).filter(|path| path.is_file())
    }

    fn offline_page(&self, volume_id: &str, page: usize, name: &str) -> Option<PathBuf> {
        self.offline_dir(volume_id).map(|dir| dir.join("pages").join(format!("{page}.{}", extension(name))))
    }

    fn offline_text(&self, volume_id: &str, layer: Option<&str>) -> Option<PathBuf> {
        let name = match layer {
            Some(layer) => format!("text-{}.mokuro", key("", layer, None, "x").trim_end_matches(".x")),
            None => "text.mokuro".to_string(),
        };
        self.offline_dir(volume_id).map(|dir| dir.join(name))
    }

    fn offline_cover(&self, volume_id: &str) -> Option<PathBuf> {
        self.offline_dir(volume_id).map(|dir| dir.join("cover"))
    }

    /// Forgets the offline copies of these volumes.
    pub fn remove_offline(&self, volume_ids: &[String]) {
        for id in volume_ids {
            if let Some(dir) = self.offline_dir(id) {
                let _ = std::fs::remove_dir_all(dir);
            }
            let _ = self.db.delete_manga_offline(id);
        }
    }

    // ── archives ───────────────────────────────────────────────────────

    fn archive_key(location: &VolumeLocation) -> String {
        key(&location.source_id, &location.source_ref, location.signature.as_deref(), "cbz")
    }

    /// The whole archive on this computer, if it is.
    fn local_archive(&self, location: &VolumeLocation) -> Option<PathBuf> {
        self.offline_archive(&location.id).or_else(|| self.cache.file(&Self::archive_key(location)))
    }

    /// Runs `read` on the remote archive through ranges. `Err(NoRange)` when
    /// the server sends whole files only.
    fn with_ranged<T>(
        &self,
        location: &VolumeLocation,
        read: impl FnOnce(&mut RangedFile<'_>) -> Result<T, archive::ArchiveError>,
    ) -> Result<T, DavError> {
        let client = self.client(&location.source_id)?;
        let total = match location.size.and_then(|size| u64::try_from(size).ok()) {
            Some(size) => size,
            None => self.block_on(&location.source_id, client.size(&location.source_ref))?.ok_or(DavError::NoRange)?,
        };
        let path = location.source_ref.clone();
        let source_id = location.source_id.clone();
        let base = Self::archive_key(location);
        let fetch = {
            let client = client.clone();
            Box::new(move |start: u64, len: u64| self.block_on(&source_id, client.range(&path, start, len, Some(total))))
        };
        let mut file = RangedFile::new(total, &self.cache, base.trim_end_matches(".cbz").to_string(), fetch);
        let slot = file.error.clone();
        let result = read(&mut file);
        let error = slot.lock().unwrap().take();
        match (result, error) {
            (Ok(value), _) => Ok(value),
            (Err(_), Some(error)) => Err(error),
            (Err(error), None) => Err(DavError::Other(error.to_string())),
        }
    }

    /// The page list of a remote volume, reading the archive's central
    /// directory the first time (and storing it).
    pub fn ensure_index(&self, location: &VolumeLocation) -> Result<Vec<String>, DavError> {
        if location.pages.is_empty() && location.format == FORMAT_SUWAYOMI_CHAPTER {
            return self.chapter_index(location);
        }
        if !location.pages.is_empty() || location.format != FORMAT_REMOTE_ARCHIVE {
            return Ok(location.pages.clone());
        }
        let pages = match self.local_archive(location) {
            Some(path) => archive::list_pages_at(&path).map_err(|e| DavError::Other(e.to_string()))?,
            // Names only: sizes would cost a request per page.
            None => self.with_ranged(location, |file| archive::list_page_names(file))?,
        };
        self.db.set_manga_volume_pages(&location.id, &pages).map_err(|e| DavError::Other(e.to_string()))?;
        Ok(pages)
    }

    /// The bytes of page `page` (0-based) of a remote volume.
    pub fn page_bytes(&self, location: &VolumeLocation, page: usize) -> Option<Vec<u8>> {
        let name = location.pages.get(page)?;
        let result = match location.format.as_str() {
            FORMAT_REMOTE_ARCHIVE => match self.local_archive(location) {
                Some(path) => archive::read_page_at(&path, name).map_err(|e| DavError::Other(e.to_string())),
                None => self.with_ranged(location, |file| archive::read_page(file, name)),
            },
            FORMAT_REMOTE_FOLDER => {
                if let Some(bytes) = self.offline_page(&location.id, page, name).and_then(|path| std::fs::read(path).ok()) {
                    return Some(bytes);
                }
                let path = format!("{}{name}", location.source_ref);
                self.small_file(&location.source_id, &path, location.signature.as_deref(), MAX_ENTRY_BYTES)
            }
            FORMAT_SUWAYOMI_CHAPTER => self.chapter_page(location, page),
            _ => return None,
        };
        result.map_err(|error| log::debug!("remote page {}/{page}: {error}", location.id)).ok()
    }

    /// A whole small file (a cover, a page of a folder, a `.mokuro`) through
    /// the cache.
    fn small_file(&self, source_id: &str, path: &str, signature: Option<&str>, max: u64) -> Result<Vec<u8>, DavError> {
        let key = key(source_id, path, signature, &extension(path));
        if let Some(bytes) = self.cache.get(&key) {
            return Ok(bytes);
        }
        let client = self.client(source_id)?;
        let bytes = self.block_on(source_id, client.get(path, max))?;
        self.cache.put(&key, &bytes);
        Ok(bytes)
    }

    /// A volume's own cover (its sidecar image), for its thumbnail.
    pub fn volume_cover(&self, location: &VolumeLocation) -> Option<Vec<u8>> {
        if let Some(bytes) = self.offline_cover(&location.id).and_then(|path| std::fs::read(path).ok()) {
            return Some(bytes);
        }
        let path = location.cover.as_deref()?;
        self.small_file(&location.source_id, path, None, MAX_ENTRY_BYTES).ok()
    }

    /// Any image of a source by remote path (a series' `cover.*`).
    pub fn image(&self, source_id: &str, path: &str) -> Option<Vec<u8>> {
        self.small_file(source_id, path, None, MAX_ENTRY_BYTES).ok()
    }

    /// The `.mokuro` of a remote volume (or of one of its layers) as a local
    /// file, fetched into the cache the first time.
    pub fn text_path(&self, location: &VolumeLocation, layer: Option<&str>) -> Option<PathBuf> {
        let remote = match layer {
            Some(layer) => location.layers.iter().find(|(name, _)| name == layer).map(|(_, path)| path.as_str()),
            None => location.mokuro_path.as_deref(),
        }?;
        if let Some(path) = self.offline_text(&location.id, layer).filter(|path| path.is_file()) {
            return Some(path);
        }
        let key = key(&location.source_id, remote, location.signature.as_deref(), "mokuro");
        if let Some(path) = self.cache.file(&key) {
            return Some(path);
        }
        let client = self.client(&location.source_id).ok()?;
        let bytes = self.block_on(&location.source_id, client.get(remote, MAX_TEXT_BYTES)).ok()?;
        self.cache.put(&key, &bytes);
        self.cache.file(&key)
    }

    // ── downloads ──────────────────────────────────────────────────────

    pub fn downloads(&self) -> Vec<DownloadProgress> {
        self.downloads.lock().unwrap().values().map(|download| download.progress.clone()).collect()
    }

    pub fn cancel_download(&self, volume_id: &str) -> bool {
        match self.downloads.lock().unwrap().get(volume_id) {
            Some(download) => {
                download.cancel.store(true, Ordering::Relaxed);
                true
            }
            None => false,
        }
    }

    fn emit(&self, progress: &DownloadProgress) {
        if let Some(download) = self.downloads.lock().unwrap().get_mut(&progress.volume_id) {
            download.progress = progress.clone();
        }
        if let Some(app) = &self.app {
            let _ = app.emit(DOWNLOAD_EVENT, progress);
        }
    }

    /// Downloads a whole volume: `offline` keeps it (with its text and cover)
    /// until the user removes it; otherwise it goes to the cache, so a server
    /// without `Range` can still be read. One download per volume at a time.
    pub fn download(&self, location: &VolumeLocation, offline: bool) -> Result<(), DavError> {
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut downloads = self.downloads.lock().unwrap();
            if downloads.contains_key(&location.id) {
                drop(downloads);
                // Already downloading (the card's button, or an earlier Read):
                // wait for it, so whoever asked can open the volume right after.
                while self.downloads.lock().unwrap().contains_key(&location.id) {
                    std::thread::sleep(Duration::from_millis(200));
                }
                return if offline || self.local_archive(location).is_some() || location.format != FORMAT_REMOTE_ARCHIVE {
                    Ok(())
                } else {
                    Err(DavError::Cancelled)
                };
            }
            let progress = DownloadProgress { volume_id: location.id.clone(), offline, received: 0, total: None, done: false, error: None };
            downloads.insert(location.id.clone(), Download { cancel: cancel.clone(), progress });
        }
        let result = self.download_inner(location, offline, &cancel);
        let final_progress = {
            let downloads = self.downloads.lock().unwrap();
            let last = downloads.get(&location.id).map(|d| d.progress.clone());
            last.unwrap_or(DownloadProgress { volume_id: location.id.clone(), offline, received: 0, total: None, done: false, error: None })
        };
        self.emit(&DownloadProgress { done: true, error: result.as_ref().err().map(|e| e.code().to_string()), ..final_progress });
        self.downloads.lock().unwrap().remove(&location.id);
        result
    }

    fn download_inner(&self, location: &VolumeLocation, offline: bool, cancel: &AtomicBool) -> Result<(), DavError> {
        let client = self.client(&location.source_id)?;
        let other = |e: std::io::Error| DavError::Other(e.to_string());
        let dir = self.offline_dir(&location.id).ok_or(DavError::NotFound)?;
        let mut last_emit = Instant::now() - Duration::from_secs(1);
        let mut progress = |received: u64, total: Option<u64>| {
            if last_emit.elapsed() >= Duration::from_millis(200) || total == Some(received) {
                last_emit = Instant::now();
                self.emit(&DownloadProgress { volume_id: location.id.clone(), offline, received, total, done: false, error: None });
            }
        };
        let mut bytes = 0u64;
        match location.format.as_str() {
            FORMAT_REMOTE_ARCHIVE => {
                if offline {
                    std::fs::create_dir_all(&dir).map_err(other)?;
                    let target = dir.join("volume.cbz");
                    // Already cached whole: no need to ask the server again.
                    if let Some(cached) = self.cache.file(&Self::archive_key(location)) {
                        std::fs::copy(&cached, &target).map_err(other)?;
                        bytes = std::fs::metadata(&target).map_err(other)?.len();
                        progress(bytes, Some(bytes));
                    } else {
                        bytes = self.block_on(&location.source_id, client.download(&location.source_ref, &target, &mut progress, cancel))?;
                    }
                } else if self.local_archive(location).is_none() {
                    let scratch = self.cache.scratch();
                    bytes = self.block_on(&location.source_id, client.download(&location.source_ref, &scratch, &mut progress, cancel))?;
                    self.cache.adopt(&Self::archive_key(location), &scratch).ok_or_else(|| DavError::Other("cache".into()))?;
                }
                // The index comes from the local copy now.
                let fresh = VolumeLocation { pages: Vec::new(), ..location.clone() };
                self.ensure_index(&fresh)?;
            }
            FORMAT_REMOTE_FOLDER => {
                if !offline {
                    return Ok(());
                }
                let total = location.size.and_then(|size| u64::try_from(size).ok());
                std::fs::create_dir_all(dir.join("pages")).map_err(other)?;
                for (index, name) in location.pages.iter().enumerate() {
                    if cancel.load(Ordering::Relaxed) {
                        return Err(DavError::Cancelled);
                    }
                    let target = self.offline_page(&location.id, index, name).ok_or(DavError::NotFound)?;
                    if !target.is_file() {
                        let path = format!("{}{name}", location.source_ref);
                        let page = self.small_file(&location.source_id, &path, location.signature.as_deref(), MAX_ENTRY_BYTES)?;
                        std::fs::write(&target, &page).map_err(other)?;
                    }
                    bytes += std::fs::metadata(&target).map_or(0, |meta| meta.len());
                    progress(bytes, total.map(|total| total.max(bytes)));
                }
            }
            FORMAT_SUWAYOMI_CHAPTER => {
                if !offline {
                    return Ok(());
                }
                let pages = self.ensure_index(location)?;
                let location = VolumeLocation { pages: pages.clone(), ..location.clone() };
                std::fs::create_dir_all(dir.join("pages")).map_err(other)?;
                let total = pages.len() as u64;
                for index in 0..pages.len() {
                    if cancel.load(Ordering::Relaxed) {
                        return Err(DavError::Cancelled);
                    }
                    let target = self.offline_page(&location.id, index, "page.img").ok_or(DavError::NotFound)?;
                    if !target.is_file() {
                        let page = self.chapter_page(&location, index)?;
                        std::fs::write(&target, &page).map_err(other)?;
                    }
                    // Pages, not bytes: a chapter's size is not known before.
                    progress(index as u64 + 1, Some(total));
                }
                self.db.set_manga_offline(&location.id, dir_size(&dir) as i64).map_err(|e| DavError::Other(e.to_string()))?;
                return Ok(());
            }
            _ => return Err(DavError::NotFound),
        }
        if offline {
            // The text and the cover go with it; a missing one is no failure.
            if let Some(text) = self.text_path(location, None) {
                let _ = std::fs::copy(text, self.offline_text(&location.id, None).ok_or(DavError::NotFound)?);
            }
            for (layer, _) in &location.layers {
                if let Some(text) = self.text_path(location, Some(layer)) {
                    let _ = std::fs::copy(text, self.offline_text(&location.id, Some(layer)).ok_or(DavError::NotFound)?);
                }
            }
            if let Some(cover) = self.volume_cover(location) {
                let _ = std::fs::write(self.offline_cover(&location.id).ok_or(DavError::NotFound)?, cover);
            }
            self.db.set_manga_offline(&location.id, dir_size(&dir) as i64).map_err(|e| DavError::Other(e.to_string()))?;
        }
        let _ = bytes;
        Ok(())
    }

    // ── chapters from extensions ───────────────────────────────────────

    fn extensions(&self) -> Result<Arc<crate::manga::suwayomi::Service>, DavError> {
        self.suwayomi.get().cloned().ok_or(DavError::Extension("not-configured"))
    }

    /// A chapter's pages, asked of its server the first time (and stored).
    fn chapter_index(&self, location: &VolumeLocation) -> Result<Vec<String>, DavError> {
        let service = self.extensions()?;
        let chapter: i64 = location.source_ref.parse().map_err(|_| DavError::NotFound)?;
        let mode = service.mode_blocking();
        if mode.source_id() != location.source_id {
            // A series of the other mode's server: only what is downloaded.
            return Err(DavError::Extension("other-server"));
        }
        let pages = tauri::async_runtime::block_on(async { service.connect(mode).await?.chapter_pages(chapter).await })?;
        self.db.set_manga_volume_pages(&location.id, &pages).map_err(|e| DavError::Other(e.to_string()))?;
        Ok(pages)
    }

    /// One page of a chapter: its offline copy, the cache, or the server.
    fn chapter_page(&self, location: &VolumeLocation, page: usize) -> Result<Vec<u8>, DavError> {
        // Being read: the managed server is in use.
        if let Some(service) = self.suwayomi.get() {
            service.touch();
        }
        if let Some(dir) = self.offline_dir(&location.id) {
            if let Ok(entries) = std::fs::read_dir(dir.join("pages")) {
                let prefix = format!("{page}.");
                if let Some(entry) = entries.filter_map(Result::ok).find(|entry| entry.file_name().to_string_lossy().starts_with(&prefix)) {
                    if let Ok(bytes) = std::fs::read(entry.path()) {
                        return Ok(bytes);
                    }
                }
            }
        }
        let path = location.pages.get(page).ok_or(DavError::NotFound)?;
        let key = key(&location.source_id, path, location.signature.as_deref(), "img");
        if let Some(bytes) = self.cache.get(&key) {
            return Ok(bytes);
        }
        let bytes = self.extensions()?.fetch_blocking(&location.source_id, path, MAX_ENTRY_BYTES)?;
        self.cache.put(&key, &bytes);
        Ok(bytes)
    }

    /// An image of an extensions server by its token (an icon, a
    /// thumbnail), through the cache.
    pub fn extension_image(&self, token: &str) -> Option<Vec<u8>> {
        let service = self.suwayomi.get()?;
        let (source_row, path) = service.image_path(token)?;
        let key = key(&source_row, &path, None, "img");
        if let Some(bytes) = self.cache.get(&key) {
            return Some(bytes);
        }
        let bytes = service.fetch_blocking(&source_row, &path, MAX_ENTRY_BYTES).ok()?;
        self.cache.put(&key, &bytes);
        Some(bytes)
    }

    /// A series' cover on an extensions server (a server path), cached.
    pub fn extension_cover(&self, source_row: &str, path: &str) -> Option<Vec<u8>> {
        let key = key(source_row, path, None, "img");
        if let Some(bytes) = self.cache.get(&key) {
            return Some(bytes);
        }
        let bytes = self.suwayomi.get()?.fetch_blocking(source_row, path, MAX_ENTRY_BYTES).ok()?;
        self.cache.put(&key, &bytes);
        Some(bytes)
    }

    pub fn usage(&self) -> WebdavCacheUsage {
        let (offline_volumes, offline_bytes) = self.db.manga_offline_totals().unwrap_or((0, 0));
        WebdavCacheUsage {
            cache_bytes: self.cache.usage(),
            limit_bytes: self.cache.limit(),
            offline_bytes: offline_bytes.max(0) as u64,
            offline_volumes: offline_volumes.max(0) as u64,
        }
    }
}

fn dir_size(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    entries
        .filter_map(Result::ok)
        .map(|entry| match entry.metadata() {
            Ok(meta) if meta.is_dir() => dir_size(&entry.path()),
            Ok(meta) => meta.len(),
            Err(_) => 0,
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manga::store::tests::temp_db;
    use crate::manga::store::{RemoteVolumeRecord, SOURCE_WEBDAV};
    use crate::manga::thumbs::tests::png;
    use crate::manga::webdav::test_server::{serve_file, Reply, TestServer};
    use std::io::Write;

    /// A CBZ with `pages` PNG pages of some size each, written here.
    fn cbz(pages: usize) -> (Vec<u8>, Vec<Vec<u8>>) {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let mut images = Vec::new();
        for page in 0..pages {
            let image = png(300 + page as u32, 400);
            writer.start_file(format!("Vol 1/{:03}.png", page + 1), options).unwrap();
            writer.write_all(&image).unwrap();
            images.push(image);
        }
        // Filler so the archive spans many blocks.
        writer.start_file("extra/notes.txt", options).unwrap();
        writer.write_all(&vec![b'x'; 3 * super::super::ranged::BLOCK as usize]).unwrap();
        (writer.finish().unwrap().into_inner(), images)
    }

    struct Setup {
        remote: Remote,
        db: Arc<Db>,
        db_path: PathBuf,
        root: PathBuf,
        server: TestServer,
        volume: String,
    }

    fn setup(ranges: bool) -> (Setup, Vec<Vec<u8>>, usize) {
        let (bytes, images) = cbz(4);
        let size = bytes.len();
        let mokuro = r#"{"pages":[{"img_path":"Vol 1/001.png","img_width":300,"img_height":400,"blocks":[{"lines":["テスト"]}]}]}"#.as_bytes().to_vec();
        let server = TestServer::start(move |request| match request.path.as_str() {
            "/m/S/Vol%201.cbz" => serve_file(request, &bytes, ranges),
            "/m/S/Vol%201.mokuro" => Reply::new(200, mokuro.clone()),
            "/m/S/Vol%201.webp" => Reply::new(200, png(10, 15)),
            _ => Reply::status(404),
        });
        let (db, db_path) = temp_db();
        let db = Arc::new(db);
        db.add_manga_source("dav", SOURCE_WEBDAV, "S", &server.url("/m/")).unwrap();
        db.apply_remote_series("dav", &[("s".into(), "/m/S/".into(), "S".into())]).unwrap();
        let volume = crate::manga::webdav::sync::volume_id("dav", "/m/S/Vol 1.cbz");
        db.apply_remote_volumes("s", Some("1"), None, &[RemoteVolumeRecord {
            id: volume.clone(),
            title: "Vol 1".into(),
            source_ref: "/m/S/Vol 1.cbz".into(),
            format: FORMAT_REMOTE_ARCHIVE,
            position: 1,
            pages: Vec::new(),
            signature: Some("sig".into()),
            mokuro_path: Some("/m/S/Vol 1.mokuro".into()),
            layers: Vec::new(),
            cover: Some("/m/S/Vol 1.webp".into()),
            size: Some(size as i64),
        }]).unwrap();
        let root = std::env::temp_dir().join(format!("ntdc-remote-{}", uuid::Uuid::new_v4()));
        let remote = Remote::new(db.clone(), root.join("cache"), root.join("offline"), u64::MAX, None);
        (Setup { remote, db, db_path, root, server, volume }, images, size)
    }

    impl Drop for Setup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
            let _ = std::fs::remove_file(&self.db_path);
        }
    }

    #[test]
    fn a_cbz_is_read_by_ranges_without_downloading_it_whole() {
        let (s, images, size) = setup(true);
        let location = s.db.manga_volume_location(&s.volume).unwrap().unwrap();
        let pages = s.remote.ensure_index(&location).unwrap();
        assert_eq!(pages, ["Vol 1/001.png", "Vol 1/002.png", "Vol 1/003.png", "Vol 1/004.png"]);
        assert_eq!(s.db.manga_volume(&s.volume).unwrap().unwrap().page_count, 4);

        let location = s.db.manga_volume_location(&s.volume).unwrap().unwrap();
        assert_eq!(s.remote.page_bytes(&location, 2).unwrap(), images[2]);
        let sent = s.server.sent.load(Ordering::SeqCst) as usize;
        assert!(sent < size / 2, "{sent} of {size} bytes");

        // Shown again: from the cache, no request.
        let requests = s.server.requests.load(Ordering::SeqCst);
        assert_eq!(s.remote.page_bytes(&location, 2).unwrap(), images[2]);
        assert_eq!(s.server.requests.load(Ordering::SeqCst), requests);

        // Its text and cover come along.
        let text = s.remote.text_path(&location, None).unwrap();
        assert!(std::fs::read_to_string(text).unwrap().contains("テスト"));
        assert!(s.remote.volume_cover(&location).is_some());
    }

    #[test]
    fn without_range_it_has_to_be_downloaded_first_with_progress() {
        let (s, images, size) = setup(false);
        let location = s.db.manga_volume_location(&s.volume).unwrap().unwrap();
        assert_eq!(s.remote.ensure_index(&location).unwrap_err(), DavError::NoRange);
        // The whole-file answer is dropped, not kept: nothing was cached.
        assert_eq!(s.remote.usage().cache_bytes, 0);

        s.remote.download(&location, false).unwrap();
        let location = s.db.manga_volume_location(&s.volume).unwrap().unwrap();
        assert_eq!(location.pages.len(), 4);
        assert_eq!(s.remote.page_bytes(&location, 0).unwrap(), images[0]);
        assert!(s.remote.usage().cache_bytes >= size as u64);
        assert_eq!(s.remote.usage().offline_volumes, 0);
    }

    #[test]
    fn an_offline_copy_reads_with_the_server_gone_and_can_be_removed() {
        let (mut s, images, _) = setup(true);
        let location = s.db.manga_volume_location(&s.volume).unwrap().unwrap();
        s.remote.download(&location, true).unwrap();
        assert!(s.db.manga_volume(&s.volume).unwrap().unwrap().downloaded);
        assert_eq!(s.remote.usage().offline_volumes, 1);
        s.remote.cache.clear();

        // The server goes away.
        let port_holder = std::mem::replace(&mut s.server, TestServer::start(|_| Reply::status(500)));
        drop(port_holder);
        let location = s.db.manga_volume_location(&s.volume).unwrap().unwrap();
        assert_eq!(s.remote.page_bytes(&location, 3).unwrap(), images[3]);
        assert!(s.remote.text_path(&location, None).is_some());
        assert!(s.remote.volume_cover(&location).is_some());

        s.remote.remove_offline(std::slice::from_ref(&s.volume));
        assert!(!s.db.manga_volume(&s.volume).unwrap().unwrap().downloaded);
        assert!(s.remote.page_bytes(&location, 3).is_none());
    }
}
