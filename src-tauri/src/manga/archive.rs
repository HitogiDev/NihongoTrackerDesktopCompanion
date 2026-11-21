//! CBZ/ZIP volumes: which entries are pages, in reading order, and reading one
//! of them without trusting the archive.
//!
//! A volume comes from wherever the user found it, so nothing in it is taken at
//! its word: names with `..` or an absolute path are never pages, an entry that
//! inflates far beyond its compressed size is a bomb, and every read stops at a
//! hard limit whatever the header claims.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use super::{is_image_name, natural_cmp};

/// The largest page we will inflate. A 600 dpi double-page scan as PNG is
/// around 30 MB; anything past this is not a page.
pub const MAX_ENTRY_BYTES: u64 = 64 * 1024 * 1024;
/// More entries than this and the archive is not a manga volume.
pub const MAX_ENTRIES: usize = 20_000;
/// Inflated/compressed ratio past which an entry is a bomb. Pages are already
/// compressed images, so a real one is close to 1:1.
const MAX_RATIO: u64 = 50;
/// Below this size the ratio does not matter: a small blank PNG compresses well.
const RATIO_FLOOR: u64 = 2 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error("not a readable archive: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("the archive has too many entries")]
    TooManyEntries,
    #[error("no such page in the archive")]
    NotFound,
    #[error("the page is too large")]
    TooLarge,
    #[error("the page inflates far beyond its size")]
    Bomb,
}

/// The path of an entry if it is safe to treat as one: relative, with `/` as
/// separator, and no `..`, drive letter or empty component. `None` otherwise.
pub fn safe_entry_name(raw: &str) -> Option<String> {
    let name = raw.replace('\\', "/");
    if name.starts_with('/') || name.contains('\0') {
        return None;
    }
    let mut parts = Vec::new();
    for part in name.split('/') {
        match part {
            "" | "." => continue,
            ".." => return None,
            // `C:` would make the path absolute on Windows.
            _ if part.contains(':') => return None,
            _ => parts.push(part),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// Noise that archivers and operating systems leave behind.
fn is_junk(name: &str) -> bool {
    name.split('/').any(|part| part.starts_with('.') || part.eq_ignore_ascii_case("__MACOSX"))
        || name.rsplit('/').next().is_some_and(|file| file.eq_ignore_ascii_case("thumbs.db"))
}

/// The most an entry with this compressed size may inflate to.
fn inflate_limit(compressed: u64) -> u64 {
    compressed.saturating_mul(MAX_RATIO).max(RATIO_FLOOR).min(MAX_ENTRY_BYTES)
}

/// The image entries of an archive in reading order (natural order, so `2`
/// comes before `10`). Entries that are too large, compress suspiciously well
/// or have an unsafe name are left out rather than failing the whole volume.
pub fn list_pages<R: Read + Seek>(reader: R) -> Result<Vec<String>, ArchiveError> {
    let mut archive = zip::ZipArchive::new(reader)?;
    if archive.len() > MAX_ENTRIES {
        return Err(ArchiveError::TooManyEntries);
    }
    let mut pages = Vec::new();
    for index in 0..archive.len() {
        let entry = archive.by_index_raw(index)?;
        if entry.is_dir() {
            continue;
        }
        let raw = entry.name().to_string();
        let Some(name) = safe_entry_name(&raw) else { continue };
        // Only names that round-trip can be found again by `read_page`.
        if name != raw || is_junk(&name) || !is_image_name(&name) {
            continue;
        }
        if entry.size() > inflate_limit(entry.compressed_size()) {
            continue;
        }
        pages.push(name);
    }
    pages.sort_by(|a, b| natural_cmp(a, b));
    pages.dedup();
    Ok(pages)
}

/// The image entries of an archive in reading order, from its central
/// directory alone. Opening each entry (`by_index_raw`, as [`list_pages`]
/// does for its size check) reads that entry's local header: on a remote
/// archive that is one request per page, minutes for a volume. Sizes are
/// left to [`read_page`], which enforces the same limits when a page is read.
pub fn list_page_names<R: Read + Seek>(reader: R) -> Result<Vec<String>, ArchiveError> {
    let archive = zip::ZipArchive::new(reader)?;
    if archive.len() > MAX_ENTRIES {
        return Err(ArchiveError::TooManyEntries);
    }
    let mut pages: Vec<String> = (0..archive.len())
        .filter_map(|index| archive.name_for_index(index))
        .filter(|raw| !raw.ends_with('/'))
        .filter_map(|raw| {
            let name = safe_entry_name(raw)?;
            (name == *raw && !is_junk(&name) && is_image_name(&name)).then_some(name)
        })
        .collect();
    pages.sort_by(|a, b| natural_cmp(a, b));
    pages.dedup();
    Ok(pages)
}

pub fn list_pages_at(path: &Path) -> Result<Vec<String>, ArchiveError> {
    list_pages(BufReader::new(File::open(path)?))
}

/// Only a name that `list_pages` would list can be read.
fn check_name(name: &str) -> Result<(), ArchiveError> {
    if safe_entry_name(name).as_deref() != Some(name) || is_junk(name) || !is_image_name(name) {
        return Err(ArchiveError::NotFound);
    }
    Ok(())
}

/// The archive's central directory, read and checked.
fn open<R: Read + Seek>(reader: R) -> Result<zip::ZipArchive<R>, ArchiveError> {
    let archive = zip::ZipArchive::new(reader)?;
    if archive.len() > MAX_ENTRIES {
        return Err(ArchiveError::TooManyEntries);
    }
    Ok(archive)
}

/// One page, inflated in a stream that stops at the limit whatever the header
/// says, so a lying entry costs at most `MAX_ENTRY_BYTES`.
pub fn read_page<R: Read + Seek>(reader: R, name: &str) -> Result<Vec<u8>, ArchiveError> {
    check_name(name)?;
    read_entry(&mut open(reader)?, name)
}

/// [`read_page`] on an archive already open.
fn read_entry<R: Read + Seek>(archive: &mut zip::ZipArchive<R>, name: &str) -> Result<Vec<u8>, ArchiveError> {
    check_name(name)?;
    let entry = match archive.by_name(name) {
        Ok(entry) => entry,
        Err(zip::result::ZipError::FileNotFound) => return Err(ArchiveError::NotFound),
        Err(error) => return Err(error.into()),
    };
    if entry.is_dir() {
        return Err(ArchiveError::NotFound);
    }
    let limit = inflate_limit(entry.compressed_size());
    if entry.size() > MAX_ENTRY_BYTES {
        return Err(ArchiveError::TooLarge);
    }
    if entry.size() > limit {
        return Err(ArchiveError::Bomb);
    }
    let mut bytes = Vec::with_capacity(entry.size().min(limit) as usize);
    entry.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(if limit == MAX_ENTRY_BYTES { ArchiveError::TooLarge } else { ArchiveError::Bomb });
    }
    Ok(bytes)
}

// ── archives on disk, kept open ─────────────────────────────────────────

/// How many archives on disk stay open between pages.
const OPEN_ARCHIVES: usize = 4;

/// An archive on disk with its central directory already read, and the
/// version of the file it was read from (size and modification time).
struct OpenArchive {
    path: PathBuf,
    version: (u64, Option<SystemTime>),
    archive: zip::ZipArchive<BufReader<File>>,
}

/// The last archives read, most recent first: turning a page does not open
/// the file and parse its directory again. A file that changes (another
/// size or time) is opened afresh, and its old entry goes.
static OPEN: Mutex<Vec<OpenArchive>> = Mutex::new(Vec::new());

/// A page of an archive on disk (a local CBZ, or a remote one downloaded
/// whole), reading the file's directory only the first time.
pub fn read_page_at(path: &Path, name: &str) -> Result<Vec<u8>, ArchiveError> {
    check_name(name)?;
    let meta = std::fs::metadata(path)?;
    let version = (meta.len(), meta.modified().ok());
    let kept = {
        let mut open = OPEN.lock().unwrap();
        open.iter().position(|entry| entry.path == path && entry.version == version).map(|at| open.remove(at))
    };
    let mut entry = match kept {
        Some(entry) => entry,
        None => OpenArchive { path: path.to_path_buf(), version, archive: open(BufReader::new(File::open(path)?))? },
    };
    let result = read_entry(&mut entry.archive, name);
    // Only an archive that answered sensibly is kept: a read error may mean
    // the file changed under it.
    if matches!(result, Ok(_) | Err(ArchiveError::NotFound | ArchiveError::TooLarge | ArchiveError::Bomb)) {
        let mut open = OPEN.lock().unwrap();
        open.retain(|other| other.path != entry.path);
        open.insert(0, entry);
        open.truncate(OPEN_ARCHIVES);
    }
    result
}

/// Lets go of the archives kept open under `dir` (before deleting it).
pub fn close_under(dir: &Path) {
    OPEN.lock().unwrap().retain(|entry| !entry.path.starts_with(dir));
}

/// Lets go of every archive kept open, when the reader closes: on Windows an
/// open file keeps the folder it is in from being renamed or moved, and
/// nothing needs them until the next volume is read.
pub fn close_all() {
    OPEN.lock().unwrap().clear();
}

// ── archives on a server ────────────────────────────────────────────────

/// Bytes of an archive that are only known in places (blocks around its
/// central directory, one entry), as `Read + Seek` for the zip reader. Every
/// part is the file's real bytes at its offset. A read anywhere else fails,
/// or, while indexing (`blank`), finds a local header with no name and no
/// extra field: all the zip reader needs to hand out what the central
/// directory says of an entry without fetching it.
struct Sparse {
    len: u64,
    parts: Vec<(u64, Arc<[u8]>)>,
    pos: u64,
    blank: bool,
}

/// A local header's signature and nothing else.
const BLANK_HEADER: [u8; 30] = {
    let mut header = [0u8; 30];
    header[0] = b'P';
    header[1] = b'K';
    header[2] = 3;
    header[3] = 4;
    header
};

impl Read for Sparse {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pos >= self.len || buf.is_empty() {
            return Ok(0);
        }
        let pos = self.pos;
        let found = self.parts.iter().find(|(start, bytes)| pos >= *start && pos - start < bytes.len() as u64);
        let n = match found {
            Some((start, bytes)) => {
                let offset = (pos - start) as usize;
                let n = buf.len().min(bytes.len() - offset);
                buf[..n].copy_from_slice(&bytes[offset..offset + n]);
                n
            }
            None if self.blank => {
                let n = buf.len().min(BLANK_HEADER.len());
                buf[..n].copy_from_slice(&BLANK_HEADER[..n]);
                n
            }
            None => return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "not fetched")),
        };
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for Sparse {
    fn seek(&mut self, to: std::io::SeekFrom) -> std::io::Result<u64> {
        let target = match to {
            std::io::SeekFrom::Start(at) => Some(at),
            std::io::SeekFrom::End(delta) => self.len.checked_add_signed(delta),
            std::io::SeekFrom::Current(delta) => self.pos.checked_add_signed(delta),
        };
        self.pos = target.ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "seek before the start"))?;
        Ok(self.pos)
    }
}

/// Where an entry is in the archive, as its central directory says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntrySpan {
    pub header_start: u64,
    pub compressed_size: u64,
    pub size: u64,
    name_len: u64,
}

/// Room after an entry's data for a local header longer than the central
/// one: an extra field is at most 64 KB.
const LOCAL_MARGIN: u64 = 64 * 1024;

/// What a remote archive's central directory says of its entries, kept so a
/// page is one request for its local header and data: no directory read
/// again, no blocks of 512 KB that a page straddles.
pub struct RemoteIndex {
    len: u64,
    /// The blocks the directory was read from, for the zip reader to read it
    /// again from memory with each page.
    parts: Vec<(u64, Arc<[u8]>)>,
    entries: HashMap<String, EntrySpan>,
}

impl RemoteIndex {
    /// From the blocks that reading the directory of an archive of `len`
    /// bytes touched (`RangedFile::record`). No request: everything comes from
    /// those bytes.
    pub fn build(len: u64, parts: Vec<(u64, Arc<[u8]>)>) -> Result<Self, ArchiveError> {
        let mut archive = open(Sparse { len, parts: parts.clone(), pos: 0, blank: true })?;
        let mut entries = HashMap::new();
        for index in 0..archive.len() {
            // An entry the reader cannot place is left out: its page is read
            // the slow way.
            let Ok(entry) = archive.by_index_raw(index) else { continue };
            if entry.is_dir() || entry.encrypted() {
                continue;
            }
            entries.insert(entry.name().to_string(), EntrySpan {
                header_start: entry.header_start(),
                compressed_size: entry.compressed_size(),
                size: entry.size(),
                name_len: entry.name_raw().len() as u64,
            });
        }
        Ok(RemoteIndex { len, parts, entries })
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    /// Where a page is, if it can be one: the same limits `read_page`
    /// enforces, before anything is fetched.
    pub fn span(&self, name: &str) -> Option<Result<EntrySpan, ArchiveError>> {
        if check_name(name).is_err() {
            return Some(Err(ArchiveError::NotFound));
        }
        let span = *self.entries.get(name)?;
        Some(if span.size > MAX_ENTRY_BYTES || span.compressed_size > MAX_ENTRY_BYTES {
            Err(ArchiveError::TooLarge)
        } else if span.size > inflate_limit(span.compressed_size) {
            Err(ArchiveError::Bomb)
        } else {
            Ok(span)
        })
    }

    /// The bytes to fetch for an entry: its local header (as long as the
    /// central one, plus room for a longer extra field) and its data.
    pub fn window(&self, span: &EntrySpan) -> (u64, u64) {
        let want = 30 + span.name_len + span.compressed_size + LOCAL_MARGIN;
        (span.header_start, want.min(self.len.saturating_sub(span.header_start)))
    }

    /// How many bytes from the header's start the entry really takes, from
    /// its local header (name and extra field lengths at 26 and 28), when
    /// `fetched` starts with one.
    pub fn needed(span: &EntrySpan, fetched: &[u8]) -> Option<u64> {
        if fetched.len() < 30 || fetched[..4] != BLANK_HEADER[..4] {
            return None;
        }
        let name = u16::from_le_bytes([fetched[26], fetched[27]]) as u64;
        let extra = u16::from_le_bytes([fetched[28], fetched[29]]) as u64;
        Some(30 + name + extra + span.compressed_size)
    }

    /// Reads a page from the bytes fetched for it (`at` = where they start),
    /// with every rule of `read_page`.
    pub fn read(&self, name: &str, at: u64, fetched: Vec<u8>) -> Result<Vec<u8>, ArchiveError> {
        let mut parts = Vec::with_capacity(self.parts.len() + 1);
        parts.push((at, Arc::from(fetched)));
        parts.extend(self.parts.iter().cloned());
        read_page(Sparse { len: self.len, parts, pos: 0, blank: false }, name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;

    fn archive(entries: &[(&str, &[u8])], method: zip::CompressionMethod) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(method);
        for (name, data) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn pages_come_in_natural_order_without_junk() {
        let bytes = archive(
            &[
                ("10.jpg", b"x"),
                ("2.jpg", b"x"),
                ("001.jpg", b"x"),
                ("__MACOSX/._001.jpg", b"x"),
                (".hidden.png", b"x"),
                ("notes.txt", b"x"),
                ("Thumbs.db", b"x"),
                ("cover/.DS_Store", b"x"),
            ],
            zip::CompressionMethod::Stored,
        );
        assert_eq!(list_pages(Cursor::new(bytes)).unwrap(), ["001.jpg", "2.jpg", "10.jpg"]);
    }

    /// Counts where reads land, like a remote archive read by ranges would.
    struct Tracking<R> {
        inner: R,
        pos: u64,
        /// Where each read started.
        reads: Vec<u64>,
    }

    impl<R> Tracking<R> {
        fn new(inner: R) -> Self {
            Tracking { inner, pos: 0, reads: Vec::new() }
        }

        /// Distinct places read before `end` (each would be a range request).
        fn places_before(&self, end: u64) -> usize {
            let mut places: Vec<u64> = self.reads.iter().copied().filter(|&at| at < end).collect();
            places.dedup();
            places.len()
        }
    }

    impl<R: Read> Read for Tracking<R> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.reads.last() != Some(&self.pos) {
                self.reads.push(self.pos);
            }
            let n = self.inner.read(buf)?;
            self.pos += n as u64;
            Ok(n)
        }
    }

    impl<R: Seek> Seek for Tracking<R> {
        fn seek(&mut self, to: std::io::SeekFrom) -> std::io::Result<u64> {
            self.pos = self.inner.seek(to)?;
            Ok(self.pos)
        }
    }

    #[test]
    fn page_names_come_from_the_central_directory_alone() {
        let page = vec![7u8; 4096];
        let entries: Vec<(String, &[u8])> = (1..=20).map(|n| (format!("{n}.jpg"), page.as_slice())).collect();
        let mut all: Vec<(&str, &[u8])> = entries.iter().map(|(name, data)| (name.as_str(), *data)).collect();
        all.push(("__MACOSX/._1.jpg", b"x"));
        all.push(("notes.txt", b"x"));
        all.push(("../evil.png", b"x"));
        let bytes = archive(&all, zip::CompressionMethod::Stored);
        let directory = zip::ZipArchive::new(Cursor::new(bytes.clone())).unwrap().central_directory_start();

        let mut by_name = Tracking::new(Cursor::new(bytes.clone()));
        let names = list_page_names(&mut by_name).unwrap();
        let mut by_entry = Tracking::new(Cursor::new(bytes));
        assert_eq!(names, list_pages(&mut by_entry).unwrap());
        assert_eq!(names.len(), 20);
        // Opening every entry reads every local header; names alone only
        // touch the start once (the archive's own check of its first header).
        assert!(by_entry.places_before(directory) >= 20);
        assert!(by_name.places_before(directory) <= 1, "{:?}", by_name.reads);
    }

    #[test]
    fn nested_folders_are_ordered_by_folder_then_page() {
        let bytes = archive(
            &[("Vol 1/ch10/1.png", b"x"), ("Vol 1/ch2/10.png", b"x"), ("Vol 1/ch2/9.png", b"x")],
            zip::CompressionMethod::Stored,
        );
        assert_eq!(
            list_pages(Cursor::new(bytes)).unwrap(),
            ["Vol 1/ch2/9.png", "Vol 1/ch2/10.png", "Vol 1/ch10/1.png"]
        );
    }

    #[test]
    fn unsafe_names_are_never_pages() {
        assert_eq!(safe_entry_name("../evil.png"), None);
        assert_eq!(safe_entry_name("a/../../evil.png"), None);
        assert_eq!(safe_entry_name("..\\evil.png"), None);
        assert_eq!(safe_entry_name("/etc/evil.png"), None);
        assert_eq!(safe_entry_name("C:\\evil.png"), None);
        assert_eq!(safe_entry_name("ok/./1.png").as_deref(), Some("ok/1.png"));

        let bytes = archive(
            &[("../evil.png", b"x"), ("/abs.png", b"x"), ("good.png", b"x")],
            zip::CompressionMethod::Stored,
        );
        assert_eq!(list_pages(Cursor::new(bytes.clone())).unwrap(), ["good.png"]);
        assert!(matches!(read_page(Cursor::new(bytes.clone()), "../evil.png"), Err(ArchiveError::NotFound)));
        assert_eq!(read_page(Cursor::new(bytes), "good.png").unwrap(), b"x");
    }

    #[test]
    fn a_zip_bomb_is_neither_listed_nor_read() {
        // 8 MB of zeros deflates to a few KB: a ratio no real page has.
        let zeros = vec![0u8; 8 * 1024 * 1024];
        let bytes = archive(&[("bomb.png", &zeros), ("1.png", b"page")], zip::CompressionMethod::Deflated);
        assert_eq!(list_pages(Cursor::new(bytes.clone())).unwrap(), ["1.png"]);
        assert!(matches!(read_page(Cursor::new(bytes.clone()), "bomb.png"), Err(ArchiveError::Bomb)));
        assert_eq!(read_page(Cursor::new(bytes), "1.png").unwrap(), b"page");
    }

    #[test]
    fn a_small_page_that_compresses_well_is_still_a_page() {
        let blank = vec![0u8; 256 * 1024];
        let bytes = archive(&[("blank.png", &blank)], zip::CompressionMethod::Deflated);
        assert_eq!(list_pages(Cursor::new(bytes.clone())).unwrap(), ["blank.png"]);
        assert_eq!(read_page(Cursor::new(bytes), "blank.png").unwrap().len(), blank.len());
    }

    #[test]
    fn a_remote_page_is_read_from_its_own_bytes_with_the_same_rules() {
        use crate::manga::webdav::{cache::Cache, ranged::RangedFile};
        // Pages that do not compress, deflated, over several blocks.
        let noise = |seed: u64, len: usize| {
            let mut state = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (0..len)
                .map(|_| {
                    state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                    (state >> 33) as u8
                })
                .collect::<Vec<u8>>()
        };
        let pages: Vec<(String, Vec<u8>)> = (1..=6).map(|n| (format!("{n:03}.png"), noise(n, 300_000))).collect();
        let zeros = vec![0u8; 8 * 1024 * 1024];
        let mut entries: Vec<(&str, &[u8])> = pages.iter().map(|(name, data)| (name.as_str(), data.as_slice())).collect();
        entries.push(("bomb.png", &zeros));
        let bytes = archive(&entries, zip::CompressionMethod::Deflated);
        let len = bytes.len() as u64;

        let cache = Cache::new(std::env::temp_dir().join(format!("ntdc-index-{}", uuid::Uuid::new_v4())), u64::MAX);
        let index = {
            let mut file = RangedFile::new(len, &cache, "ix".into(), Box::new(|start, len| Ok(bytes[start as usize..(start + len) as usize].to_vec())));
            file.record();
            zip::ZipArchive::new(&mut file).unwrap();
            RemoteIndex::build(len, file.take_recorded()).unwrap()
        };
        for (name, data) in &pages {
            let span = index.span(name).unwrap().unwrap();
            let (start, take) = index.window(&span);
            let fetched = bytes[start as usize..(start + take) as usize].to_vec();
            assert!(RemoteIndex::needed(&span, &fetched).unwrap() <= take, "one request is enough");
            assert_eq!(&index.read(name, start, fetched).unwrap(), data);
            assert_eq!(&read_page(Cursor::new(bytes.clone()), name).unwrap(), data, "the same as before");
        }
        assert!(matches!(index.span("bomb.png"), Some(Err(ArchiveError::Bomb))), "refused before fetching");
        assert!(matches!(index.span("../evil.png"), Some(Err(ArchiveError::NotFound))));
        assert!(index.span("missing.png").is_none());
        // Bytes cut short are an error, never a wrong page.
        let span = index.span("003.png").unwrap().unwrap();
        let cut = (span.header_start + 30 + 7 + span.compressed_size - 1) as usize;
        assert!(index.read("003.png", span.header_start, bytes[span.header_start as usize..cut].to_vec()).is_err());
        let _ = std::fs::remove_dir_all(cache.root());
    }

    #[test]
    fn an_archive_on_disk_stays_open_until_it_changes() {
        let dir = std::env::temp_dir().join(format!("ntdc-open-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("v.cbz");
        std::fs::write(&path, archive(&[("1.png", b"one")], zip::CompressionMethod::Stored)).unwrap();
        assert_eq!(read_page_at(&path, "1.png").unwrap(), b"one");
        assert!(matches!(read_page_at(&path, "2.png"), Err(ArchiveError::NotFound)));
        assert_eq!(read_page_at(&path, "1.png").unwrap(), b"one");
        // Rewritten (another size): read afresh, the old one let go.
        std::fs::write(&path, archive(&[("1.png", b"two!!")], zip::CompressionMethod::Stored)).unwrap();
        assert_eq!(read_page_at(&path, "1.png").unwrap(), b"two!!");
        assert!(OPEN.lock().unwrap().iter().filter(|entry| entry.path == path).count() <= 1);
        close_under(&dir);
        assert!(!OPEN.lock().unwrap().iter().any(|entry| entry.path == path));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_pages_and_non_archives_are_errors() {
        let bytes = archive(&[("1.png", b"x")], zip::CompressionMethod::Stored);
        assert!(matches!(read_page(Cursor::new(bytes), "2.png"), Err(ArchiveError::NotFound)));
        assert!(list_pages(Cursor::new(b"not a zip".to_vec())).is_err());
    }
}
