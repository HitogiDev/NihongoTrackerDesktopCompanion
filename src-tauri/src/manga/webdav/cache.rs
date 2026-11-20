//! What was fetched from WebDAV servers, on disk under
//! `app_cache_dir/manga/webdav/`, least recently used first out.
//!
//! Every file is a key: byte blocks of archives read by ranges, whole archives
//! downloaded because the server has no ranges, `.mokuro` files, covers and
//! the pages of image folders. Keys carry the remote file's signature, so a
//! file that changes on the server is simply a new key and the old one ages
//! out. Reading a file marks it used (its modification time), unless it was
//! marked less than `TOUCH_AFTER` ago: the order of files used within the
//! same minute is only roughly kept, and a block read over and over costs
//! one open instead of two. When the total passes the limit the oldest go,
//! down to `LOW_WATER` of it, so the next writes do not scan the folder again
//! at once. Downloads "for offline reading" are not here: they live in
//! `app_data_dir` and are never evicted.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The default limit, 2 GB.
pub const DEFAULT_LIMIT_MB: u64 = 2048;
/// A file used again this soon after it was marked is not marked again.
pub const TOUCH_AFTER: Duration = Duration::from_secs(60);
/// Evicting goes down to this share of the limit (per cent).
const LOW_WATER: u64 = 90;

pub struct Cache {
    root: PathBuf,
    limit: AtomicU64,
    /// Bytes on disk, counted once and then kept up to date.
    used: Mutex<Option<u64>>,
    /// The last access time handed out, so two accesses in the same instant
    /// still have an order.
    clock: AtomicU64,
    touch_after: Duration,
    /// Folder scans, for the tests.
    #[cfg(test)]
    scans: AtomicU64,
}

/// Keys are ours (uuids and suffixes): never a path.
fn valid_key(key: &str) -> bool {
    (1..=128).contains(&key.len()) && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.') && !key.starts_with('.')
}

impl Cache {
    pub fn new(root: PathBuf, limit_bytes: u64) -> Self {
        Cache {
            root,
            limit: AtomicU64::new(limit_bytes),
            used: Mutex::new(None),
            clock: AtomicU64::new(0),
            touch_after: TOUCH_AFTER,
            #[cfg(test)]
            scans: AtomicU64::new(0),
        }
    }

    /// Marks every use, however recent (exact LRU order, for the tests).
    #[cfg(test)]
    fn exact(mut self) -> Self {
        self.touch_after = Duration::ZERO;
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn set_limit(&self, bytes: u64) {
        self.limit.store(bytes, Ordering::Relaxed);
        self.evict(None);
    }

    pub fn limit(&self) -> u64 {
        self.limit.load(Ordering::Relaxed)
    }

    /// Where a key lives (one folder per first two characters, so no folder
    /// holds every block of every volume).
    pub fn path(&self, key: &str) -> Option<PathBuf> {
        valid_key(key).then(|| self.root.join(&key[..2.min(key.len())]).join(key))
    }

    fn now(&self) -> SystemTime {
        let wall = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_micros() as u64);
        let mut last = self.clock.load(Ordering::Relaxed);
        loop {
            let next = wall.max(last + 1);
            match self.clock.compare_exchange(last, next, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => return UNIX_EPOCH + Duration::from_micros(next),
                Err(current) => last = current,
            }
        }
    }

    fn touch(&self, path: &Path) {
        if let Ok(file) = std::fs::File::options().write(true).open(path) {
            let _ = file.set_modified(self.now());
        }
    }

    /// Marked longer ago than `touch_after` (or never, as far as we know).
    fn stale(&self, modified: Option<SystemTime>) -> bool {
        if self.touch_after.is_zero() {
            return true;
        }
        modified.is_none_or(|modified| SystemTime::now().duration_since(modified).map_or(false, |age| age >= self.touch_after))
    }

    /// A cached file's bytes, marking it used. One open does both.
    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        use std::io::Read;
        let path = self.path(key)?;
        let mut file = std::fs::File::options().read(true).write(true).open(&path).ok()?;
        let meta = file.metadata().ok()?;
        if !meta.is_file() {
            return None;
        }
        let mut bytes = Vec::with_capacity(meta.len() as usize);
        file.read_to_end(&mut bytes).ok()?;
        if self.stale(meta.modified().ok()) {
            let _ = file.set_modified(self.now());
        }
        Some(bytes)
    }

    /// A cached file's path, marking it used.
    pub fn file(&self, key: &str) -> Option<PathBuf> {
        let path = self.path(key)?;
        let meta = std::fs::metadata(&path).ok().filter(|meta| meta.is_file())?;
        if self.stale(meta.modified().ok()) {
            self.touch(&path);
        }
        Some(path)
    }

    /// Stores bytes under a key (written aside and renamed, so a reader never
    /// sees half a file), then makes room.
    pub fn put(&self, key: &str, bytes: &[u8]) {
        let Some(path) = self.path(key) else { return };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let partial = path.with_extension(format!("{}.part", uuid::Uuid::new_v4().simple()));
        if std::fs::write(&partial, bytes).is_err() || std::fs::rename(&partial, &path).is_err() {
            let _ = std::fs::remove_file(&partial);
            return;
        }
        self.touch(&path);
        self.added(bytes.len() as u64, &path);
    }

    /// Takes a file downloaded elsewhere (a whole archive) in under a key.
    pub fn adopt(&self, key: &str, from: &Path) -> Option<PathBuf> {
        let path = self.path(key)?;
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let size = std::fs::metadata(from).ok()?.len();
        std::fs::rename(from, &path).ok()?;
        self.touch(&path);
        self.added(size, &path);
        Some(path)
    }

    /// A scratch place for a download that will be adopted, inside the cache
    /// folder so the rename is never across disks.
    pub fn scratch(&self) -> PathBuf {
        self.root.join("downloading").join(uuid::Uuid::new_v4().simple().to_string())
    }

    fn added(&self, bytes: u64, keep: &Path) {
        let over = {
            let mut used = self.used.lock().unwrap();
            let total = match *used {
                Some(total) => total.saturating_add(bytes),
                // A fresh count already includes the file just written.
                None => self.scan().iter().map(|(_, size, _)| size).sum(),
            };
            *used = Some(total);
            total > self.limit()
        };
        if over {
            self.evict(Some(keep));
        }
    }

    /// Every cached file with its size and last use.
    fn scan(&self) -> Vec<(PathBuf, u64, SystemTime)> {
        #[cfg(test)]
        self.scans.fetch_add(1, Ordering::Relaxed);
        let mut files = Vec::new();
        let Ok(buckets) = std::fs::read_dir(&self.root) else { return files };
        for bucket in buckets.filter_map(Result::ok) {
            if bucket.file_name() == "downloading" {
                continue;
            }
            let Ok(entries) = std::fs::read_dir(bucket.path()) else { continue };
            for entry in entries.filter_map(Result::ok) {
                let Ok(meta) = entry.metadata() else { continue };
                if meta.is_file() && !entry.file_name().to_string_lossy().ends_with(".part") {
                    files.push((entry.path(), meta.len(), meta.modified().unwrap_or(UNIX_EPOCH)));
                }
            }
        }
        files
    }

    /// Once the cache is over its limit, deletes the least recently used
    /// files until it is down to `LOW_WATER` of it: the room left means the
    /// next writes add up for a while before another scan. Never `keep`
    /// (what was just stored for someone waiting on it).
    fn evict(&self, keep: Option<&Path>) {
        let mut used = self.used.lock().unwrap();
        let mut files = self.scan();
        let mut total: u64 = files.iter().map(|(_, size, _)| size).sum();
        let limit = self.limit();
        if total > limit {
            let target = limit / 100 * LOW_WATER + limit % 100 * LOW_WATER / 100;
            files.sort_by_key(|(_, _, used)| *used);
            for (path, size, _) in files {
                if total <= target {
                    break;
                }
                if Some(path.as_path()) == keep {
                    continue;
                }
                if std::fs::remove_file(&path).is_ok() {
                    total -= size;
                }
            }
        }
        *used = Some(total);
    }

    /// Bytes the cache holds now.
    pub fn usage(&self) -> u64 {
        let mut used = self.used.lock().unwrap();
        *used.get_or_insert_with(|| self.scan().iter().map(|(_, size, _)| size).sum())
    }

    /// Empties the cache (a download in progress keeps its scratch file).
    pub fn clear(&self) {
        let mut used = self.used.lock().unwrap();
        if let Ok(buckets) = std::fs::read_dir(&self.root) {
            for bucket in buckets.filter_map(Result::ok) {
                if bucket.file_name() != "downloading" {
                    let _ = std::fs::remove_dir_all(bucket.path());
                }
            }
        }
        *used = Some(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> PathBuf {
        std::env::temp_dir().join(format!("ntdc-davcache-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn the_least_recently_used_go_first_when_over_the_limit() {
        let root = temp();
        let cache = Cache::new(root.clone(), 1_000).exact();
        for n in 0..10u8 {
            cache.put(&format!("k{n}-x"), &[n; 100]);
        }
        assert_eq!(cache.usage(), 1_000);
        // Reading `k0-x` makes `k1-x` the oldest.
        assert_eq!(cache.get("k0-x").unwrap(), vec![0; 100]);
        let scans = cache.scans.load(Ordering::Relaxed);
        cache.put("ka-x", &[10; 100]);
        // Over the limit: down to 90 % of it, the two oldest.
        assert_eq!(cache.usage(), 900);
        assert_eq!(cache.scans.load(Ordering::Relaxed), scans + 1);
        assert!(cache.get("k1-x").is_none() && cache.get("k2-x").is_none(), "least recently used");
        assert!(cache.get("k0-x").is_some() && cache.get("k3-x").is_some() && cache.get("ka-x").is_some());
        // The room left takes the next write without another scan.
        cache.put("kb-x", &[11; 100]);
        assert_eq!(cache.usage(), 1_000);
        assert_eq!(cache.scans.load(Ordering::Relaxed), scans + 1, "no scan until over the limit again");

        // Lowering the limit evicts at once; a file larger than the limit
        // is still kept for whoever is waiting on it.
        cache.set_limit(150);
        assert_eq!(cache.usage(), 100);
        cache.put("ee-5", &[5; 400]);
        assert_eq!(cache.get("ee-5").map(|b| b.len()), Some(400));
        assert_eq!(cache.usage(), 400);

        cache.clear();
        assert_eq!(cache.usage(), 0);
        assert!(cache.get("ee-5").is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_file_used_again_soon_is_not_marked_again() {
        let root = temp();
        let cache = Cache::new(root.clone(), 1_000);
        cache.put("aa-1", &[1; 10]);
        let path = cache.path("aa-1").unwrap();
        let old = SystemTime::now() - 2 * TOUCH_AFTER;
        std::fs::File::options().write(true).open(&path).unwrap().set_modified(old).unwrap();
        // Long ago: marked now.
        assert_eq!(cache.get("aa-1").unwrap(), vec![1; 10]);
        let marked = std::fs::metadata(&path).unwrap().modified().unwrap();
        assert!(marked > old + TOUCH_AFTER);
        // Just now: left as it is.
        assert!(cache.get("aa-1").is_some() && cache.file("aa-1").is_some());
        assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), marked);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn keys_are_never_paths_and_downloads_are_adopted() {
        let root = temp();
        let cache = Cache::new(root.clone(), 1_000);
        for bad in ["../x", "a/b", "", ".hidden", "C:x"] {
            assert!(cache.path(bad).is_none(), "{bad}");
        }
        let scratch = cache.scratch();
        std::fs::create_dir_all(scratch.parent().unwrap()).unwrap();
        std::fs::write(&scratch, [7; 50]).unwrap();
        let adopted = cache.adopt("ff-6.cbz", &scratch).unwrap();
        assert!(!scratch.exists() && adopted.is_file());
        assert_eq!(cache.file("ff-6.cbz"), Some(adopted));
        // A fresh cache counts what is already on disk, not the scratch folder.
        assert_eq!(Cache::new(root.clone(), 1_000).usage(), 50);
        let _ = std::fs::remove_dir_all(root);
    }
}
