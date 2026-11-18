//! Getting the OCR's files, only once the user has agreed to (the UI shows
//! what and how much first).
//!
//! Every file goes to `<name>.part` and only takes its name once its size and
//! SHA-256 are the ones in `assets`: a file that does not match is deleted,
//! never kept. Downloads resume: a `.part` left by a cancelled or broken
//! download is continued with a `Range` request (from the start if the
//! server ignores it). A runtime library is read out of its NuGet package by
//! ranges: its local zip header, then its deflated bytes (`<name>.zpart`,
//! resumable too), inflated at the end.
//!
//! Blocking: runs on its own thread, with `block_on` for the requests.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Serialize;

use super::assets::{Asset, Source};
use super::engine::sha256_file;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FetchError {
    #[error("offline")]
    Offline,
    #[error("server")]
    Server,
    /// The file is not the one pinned: deleted.
    #[error("checksum")]
    Checksum,
    #[error("cancelled")]
    Cancelled,
    #[error("disk")]
    Disk,
}

impl FetchError {
    pub fn code(&self) -> &'static str {
        match self {
            FetchError::Offline => "offline",
            FetchError::Server => "server",
            FetchError::Checksum => "checksum",
            FetchError::Cancelled => "cancelled",
            FetchError::Disk => "disk",
        }
    }
}

/// The download as the UI shows it (`manga://ocr-progress`).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export, rename = "OcrDownload"))]
pub struct Progress {
    /// `downloading`, `verifying`, `done`, `failed` or `cancelled`.
    pub phase: String,
    /// The file being fetched.
    pub file: String,
    /// Bytes over the network, of everything still missing when it started.
    pub received: u64,
    pub total: u64,
    /// The error code when `failed`.
    pub error: Option<String>,
}

const CHUNK_TIMEOUT: Duration = Duration::from_secs(60);

pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .read_timeout(CHUNK_TIMEOUT)
        .user_agent(concat!("NihongoTrackerCompanion/", env!("CARGO_PKG_VERSION")))
        .build()
        .unwrap_or_default()
}

fn disk(_: std::io::Error) -> FetchError {
    FetchError::Disk
}

fn network(error: &reqwest::Error) -> FetchError {
    if error.is_status() { FetchError::Server } else { FetchError::Offline }
}

fn part_path(dir: &Path, file: &str, suffix: &str) -> PathBuf {
    dir.join(format!("{file}.{suffix}"))
}

fn len(path: &Path) -> u64 {
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

/// Downloads the bytes of `url` from `start` to `end` (exclusive; the end of
/// the file when `None`) into `dest`, continuing what `dest` already holds.
/// A server that answers a whole file instead restarts it.
fn fetch_range(
    client: &reqwest::Client,
    url: &str,
    start: u64,
    end: Option<u64>,
    dest: &Path,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64),
) -> Result<(), FetchError> {
    tauri::async_runtime::block_on(async {
        let mut have = len(dest);
        if end.is_some_and(|end| start + have > end) {
            // Longer than it can be: not ours to continue.
            std::fs::remove_file(dest).map_err(disk)?;
            have = 0;
        }
        if end.is_some_and(|end| start + have == end) {
            return Ok(());
        }
        let from = start + have;
        let range = match end {
            Some(end) => format!("bytes={from}-{}", end - 1),
            None => format!("bytes={from}-"),
        };
        let ranged = from > 0 || end.is_some();
        let mut request = client.get(url);
        if ranged {
            request = request.header(reqwest::header::RANGE, range);
        }
        let mut response = request.send().await.map_err(|e| network(&e))?;
        let status = response.status();
        let append = status == reqwest::StatusCode::PARTIAL_CONTENT;
        if !status.is_success() {
            return Err(if status.is_server_error() || status.as_u16() == 429 { FetchError::Offline } else { FetchError::Server });
        }
        if !append && start > 0 {
            // A part of a package, and the server sent all of it: no way to
            // take only the entry.
            return Err(FetchError::Server);
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(append)
            .truncate(!append)
            .open(dest)
            .map_err(disk)?;
        let mut received = if append { have } else { 0 };
        progress(received);
        while let Some(chunk) = response.chunk().await.map_err(|e| network(&e))? {
            if cancel.load(Ordering::Relaxed) {
                file.flush().map_err(disk)?;
                return Err(FetchError::Cancelled);
            }
            file.write_all(&chunk).map_err(disk)?;
            received += chunk.len() as u64;
            progress(received);
        }
        file.flush().map_err(disk)?;
        Ok(())
    })
}

/// Checks a finished `.part` and gives it its name, or deletes it.
fn verify_and_keep(part: &Path, dest: &Path, asset: &Asset) -> Result<(), FetchError> {
    if len(part) != asset.size || sha256_file(part).map_err(disk)? != asset.sha256 {
        let _ = std::fs::remove_file(part);
        return Err(FetchError::Checksum);
    }
    std::fs::rename(part, dest).map_err(disk)
}

/// Where the data of a zip entry starts, from its local header: the fixed
/// 30 bytes, then the name and extra field, whose lengths it gives.
pub fn local_data_offset(header_offset: u64, header: &[u8]) -> Option<u64> {
    if header.len() < 30 || header[..4] != [0x50, 0x4b, 0x03, 0x04] {
        return None;
    }
    let name = u64::from(u16::from_le_bytes([header[26], header[27]]));
    let extra = u64::from(u16::from_le_bytes([header[28], header[29]]));
    Some(header_offset + 30 + name + extra)
}

/// Fetches one asset into `dir`. `progress` gets the bytes of this file
/// received so far (over the network).
pub fn fetch(client: &reqwest::Client, asset: &Asset, dir: &Path, cancel: &AtomicBool, progress: &mut dyn FnMut(u64)) -> Result<(), FetchError> {
    std::fs::create_dir_all(dir).map_err(disk)?;
    let dest = dir.join(asset.file);
    if asset.present(dir) {
        progress(asset.transfer_size());
        return Ok(());
    }
    let part = part_path(dir, asset.file, "part");
    match asset.source {
        Source::Url(url) => {
            fetch_range(client, url, 0, None, &part, cancel, progress)?;
            verify_and_keep(&part, &dest, asset)
        }
        Source::ZipEntry { url, header_offset, compressed, .. } => {
            let zpart = part_path(dir, asset.file, "zpart");
            let header_file = part_path(dir, asset.file, "zhead");
            let _ = std::fs::remove_file(&header_file);
            fetch_range(client, url, header_offset, Some(header_offset + 30), &header_file, cancel, &mut |_| {})?;
            let header = std::fs::read(&header_file).map_err(disk)?;
            let _ = std::fs::remove_file(&header_file);
            // The name and extra lengths are all the header has to say: the
            // full header is 30 bytes plus those, and the data follows.
            let start = local_data_offset(header_offset, &header).ok_or(FetchError::Server)?;
            fetch_range(client, url, start, Some(start + compressed), &zpart, cancel, progress)?;
            if len(&zpart) != compressed {
                return Err(FetchError::Offline);
            }
            let inflated = (|| -> std::io::Result<()> {
                let mut decoder = flate2::read::DeflateDecoder::new(std::io::BufReader::new(std::fs::File::open(&zpart)?));
                let mut out = std::io::BufWriter::new(std::fs::File::create(&part)?);
                // Never more than the file can be: a bad entry is not a disk filler.
                let copied = std::io::copy(&mut (&mut decoder).take(asset.size + 1), &mut out)?;
                out.flush()?;
                if copied > asset.size {
                    return Err(std::io::Error::other("too large"));
                }
                Ok(())
            })();
            let _ = std::fs::remove_file(&zpart);
            if inflated.is_err() {
                let _ = std::fs::remove_file(&part);
                return Err(FetchError::Checksum);
            }
            verify_and_keep(&part, &dest, asset)
        }
    }
}

/// Free space on the disk that holds `path` (or the nearest folder above it
/// that exists).
pub fn free_space(path: &Path) -> Option<u64> {
    let mut existing = path.to_path_buf();
    while !existing.exists() {
        existing = existing.parent()?.to_path_buf();
    }
    let existing = existing.canonicalize().unwrap_or(existing);
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .filter(|disk| existing.starts_with(disk.mount_point()))
        .max_by_key(|disk| disk.mount_point().as_os_str().len())
        .map(|disk| disk.available_space())
}

/// Deletes these downloads (and what is left of them unfinished). Returns
/// the names that could not go (a library still loaded, on Windows).
pub fn delete_files(dir: &Path, files: &[&str]) -> Vec<String> {
    let mut left = Vec::new();
    for file in files {
        for name in [file.to_string(), format!("{file}.part"), format!("{file}.zpart"), format!("{file}.zhead")] {
            let path = dir.join(&name);
            if path.exists() && std::fs::remove_file(&path).is_err() {
                left.push(name);
            }
        }
    }
    left
}

/// Any of these files on disk, finished or not.
pub fn any_present(dir: &Path, files: &[&str]) -> bool {
    files.iter().any(|file| ["", ".part", ".zpart"].iter().any(|suffix| dir.join(format!("{file}{suffix}")).exists()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manga::webdav::test_server::{serve_file, Reply, TestServer};
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ntdc-ocr-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sha(bytes: &[u8]) -> String {
        use sha2::Digest;
        super::super::engine::hex(&sha2::Sha256::digest(bytes))
    }

    fn leak(value: String) -> &'static str {
        Box::leak(value.into_boxed_str())
    }

    fn asset(file: &'static str, bytes: &[u8], url: String) -> Asset {
        Asset { file, size: bytes.len() as u64, sha256: leak(sha(bytes)), source: Source::Url(leak(url)) }
    }

    #[test]
    fn a_file_is_kept_only_with_its_checksum() {
        let good: Vec<u8> = (0..50_000u32).map(|i| (i % 251) as u8).collect();
        let served = good.clone();
        let tampered = Arc::new(AtomicBool::new(false));
        let flag = tampered.clone();
        let server = TestServer::start(move |request| {
            let mut bytes = served.clone();
            if flag.load(Ordering::Relaxed) {
                bytes[123] ^= 0xff;
            }
            serve_file(request, &bytes, true)
        });
        let dir = temp_dir();
        let wanted = asset("model.onnx", &good, server.url("/model.onnx"));

        // One byte changed on the way: rejected, and nothing left behind.
        tampered.store(true, Ordering::Relaxed);
        let cancel = AtomicBool::new(false);
        assert_eq!(fetch(&client(), &wanted, &dir, &cancel, &mut |_| {}), Err(FetchError::Checksum));
        assert!(!dir.join("model.onnx").exists());
        assert!(!dir.join("model.onnx.part").exists());

        tampered.store(false, Ordering::Relaxed);
        let mut last = 0;
        fetch(&client(), &wanted, &dir, &cancel, &mut |received| last = received).unwrap();
        assert_eq!(std::fs::read(dir.join("model.onnx")).unwrap(), good);
        assert_eq!(last, good.len() as u64);
        assert!(wanted.present(&dir));

        // Changed on disk afterwards: a checksum that no longer matches is
        // what the engine checks before loading a library.
        let mut file = std::fs::OpenOptions::new().write(true).open(dir.join("model.onnx")).unwrap();
        file.write_all(b"x").unwrap();
        assert_ne!(sha256_file(&dir.join("model.onnx")).unwrap(), wanted.sha256);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_broken_download_resumes_where_it_stopped() {
        let good: Vec<u8> = (0..80_000u32).map(|i| (i * 7 % 256) as u8).collect();
        let served = good.clone();
        let sent = Arc::new(AtomicU64::new(0));
        let counter = sent.clone();
        let server = TestServer::start(move |request| {
            let reply = serve_file(request, &served, true);
            counter.fetch_add(reply.body.len() as u64, Ordering::Relaxed);
            reply
        });
        let dir = temp_dir();
        std::fs::write(dir.join("vocab.txt.part"), &good[..30_000]).unwrap();
        let wanted = asset("vocab.txt", &good, server.url("/vocab.txt"));
        let mut first = None;
        fetch(&client(), &wanted, &dir, &AtomicBool::new(false), &mut |received| { first.get_or_insert(received); }).unwrap();
        assert_eq!(std::fs::read(dir.join("vocab.txt")).unwrap(), good);
        assert_eq!(sent.load(Ordering::Relaxed), 50_000, "only the rest was sent");
        assert_eq!(first, Some(30_000));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_server_without_ranges_starts_over() {
        let good = b"0123456789".repeat(1000);
        let served = good.clone();
        let server = TestServer::start(move |request| serve_file(request, &served, false));
        let dir = temp_dir();
        std::fs::write(dir.join("a.bin.part"), b"garbage that is not the start").unwrap();
        let wanted = asset("a.bin", &good, server.url("/a.bin"));
        fetch(&client(), &wanted, &dir, &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(std::fs::read(dir.join("a.bin")).unwrap(), good);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_library_comes_out_of_its_package_by_ranges() {
        // A package with two entries; only the second is wanted.
        let library: Vec<u8> = (0..200_000u32).map(|i| (i % 97) as u8).collect();
        let mut zip_bytes = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut zip_bytes));
            let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            zip.start_file("runtimes/other/big.bin", options).unwrap();
            zip.write_all(&vec![7u8; 300_000]).unwrap();
            zip.start_file("runtimes/linux-x64/native/libonnxruntime.so", options).unwrap();
            zip.write_all(&library).unwrap();
            zip.finish().unwrap();
        }
        let (header_offset, compressed) = {
            let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&zip_bytes)).unwrap();
            let entry = archive.by_name("runtimes/linux-x64/native/libonnxruntime.so").unwrap();
            (entry.header_start(), entry.compressed_size())
        };
        let served = zip_bytes.clone();
        let sent = Arc::new(AtomicU64::new(0));
        let counter = sent.clone();
        let server = TestServer::start(move |request| {
            let reply = serve_file(request, &served, true);
            counter.fetch_add(reply.body.len() as u64, Ordering::Relaxed);
            reply
        });
        let dir = temp_dir();
        let wanted = Asset {
            file: "libonnxruntime.so",
            size: library.len() as u64,
            sha256: leak(sha(&library)),
            source: Source::ZipEntry { url: leak(server.url("/pkg.nupkg")), entry: "runtimes/linux-x64/native/libonnxruntime.so", header_offset, compressed },
        };
        fetch(&client(), &wanted, &dir, &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(std::fs::read(dir.join("libonnxruntime.so")).unwrap(), library);
        assert!(sent.load(Ordering::Relaxed) < compressed + 100, "the other entry was not downloaded");
        assert!(!dir.join("libonnxruntime.so.zpart").exists());

        // The same entry, checked against another checksum: rejected.
        std::fs::remove_file(dir.join("libonnxruntime.so")).unwrap();
        let wrong = Asset { sha256: leak(sha(b"something else")), ..wanted };
        assert_eq!(fetch(&client(), &wrong, &dir, &AtomicBool::new(false), &mut |_| {}), Err(FetchError::Checksum));
        assert!(!dir.join("libonnxruntime.so").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn cancelling_keeps_the_part_for_later() {
        let good = vec![1u8; 2_000_000];
        let served = good.clone();
        let server = TestServer::start(move |request| serve_file(request, &served, true));
        let dir = temp_dir();
        let wanted = asset("big.onnx", &good, server.url("/big.onnx"));
        let cancel = AtomicBool::new(false);
        let result = fetch(&client(), &wanted, &dir, &cancel, &mut |received| {
            if received > 0 {
                cancel.store(true, Ordering::Relaxed);
            }
        });
        assert_eq!(result, Err(FetchError::Cancelled));
        assert!(dir.join("big.onnx.part").exists());
        assert!(!dir.join("big.onnx").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn errors_are_codes_and_headers_are_read() {
        let server = TestServer::start(|_| Reply::status(404));
        let dir = temp_dir();
        let wanted = asset("x.onnx", b"x", server.url("/x.onnx"));
        assert_eq!(fetch(&client(), &wanted, &dir, &AtomicBool::new(false), &mut |_| {}), Err(FetchError::Server));
        let mut header = vec![0x50, 0x4b, 0x03, 0x04];
        header.extend([0u8; 22]);
        header.extend(10u16.to_le_bytes());
        header.extend(4u16.to_le_bytes());
        assert_eq!(local_data_offset(100, &header), Some(144));
        assert_eq!(local_data_offset(0, b"nope"), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn deleting_some_models_leaves_the_others() {
        let dir = temp_dir();
        for name in ["meiki.a.onnx", "meiki.b.onnx.part", "keep.onnx", "lib.so"] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        assert!(any_present(&dir, &["meiki.b.onnx"]), "an unfinished download counts");
        assert!(delete_files(&dir, &["meiki.a.onnx", "meiki.b.onnx"]).is_empty());
        assert!(!any_present(&dir, &["meiki.a.onnx", "meiki.b.onnx"]));
        assert!(dir.join("keep.onnx").exists() && dir.join("lib.so").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The real thing: this computer's runtime and the int8 models (or
    /// meikiocr's with `NTDC_OCR_MODELS=meiki`), from their real URLs, into
    /// `NTDC_OCR_FETCH_DIR`. Run by hand.
    #[test]
    #[ignore]
    fn downloads_the_real_files() {
        use super::super::assets::{all_for, Models, Variant};
        let dir = PathBuf::from(std::env::var("NTDC_OCR_FETCH_DIR").expect("NTDC_OCR_FETCH_DIR"));
        let models = if std::env::var("NTDC_OCR_MODELS").as_deref() == Ok("meiki") { Models::Meiki } else { Models::MangaOcr(Variant::Int8) };
        let started = std::time::Instant::now();
        for asset in all_for(models).expect("a runtime for this computer") {
            fetch(&client(), &asset, &dir, &AtomicBool::new(false), &mut |_| {}).unwrap();
            assert!(asset.present(&dir));
            println!("{} ok ({} bytes, {:?})", asset.file, asset.size, started.elapsed());
        }
    }
}
