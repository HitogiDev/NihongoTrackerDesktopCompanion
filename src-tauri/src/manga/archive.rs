//! CBZ/ZIP volumes: which entries are pages, in reading order, and reading one
//! of them without trusting the archive.
//!
//! A volume comes from wherever the user found it, so nothing in it is taken at
//! its word: names with `..` or an absolute path are never pages, an entry that
//! inflates far beyond its compressed size is a bomb, and every read stops at a
//! hard limit whatever the header claims.

use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::Path;

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

/// One page, inflated in a stream that stops at the limit whatever the header
/// says, so a lying entry costs at most `MAX_ENTRY_BYTES`.
pub fn read_page<R: Read + Seek>(reader: R, name: &str) -> Result<Vec<u8>, ArchiveError> {
    if safe_entry_name(name).as_deref() != Some(name) || is_junk(name) || !is_image_name(name) {
        return Err(ArchiveError::NotFound);
    }
    let mut archive = zip::ZipArchive::new(reader)?;
    if archive.len() > MAX_ENTRIES {
        return Err(ArchiveError::TooManyEntries);
    }
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

pub fn read_page_at(path: &Path, name: &str) -> Result<Vec<u8>, ArchiveError> {
    read_page(BufReader::new(File::open(path)?), name)
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
    fn missing_pages_and_non_archives_are_errors() {
        let bytes = archive(&[("1.png", b"x")], zip::CompressionMethod::Stored);
        assert!(matches!(read_page(Cursor::new(bytes), "2.png"), Err(ArchiveError::NotFound)));
        assert!(list_pages(Cursor::new(b"not a zip".to_vec())).is_err());
    }
}
