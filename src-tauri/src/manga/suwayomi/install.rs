//! Installing the managed server, once the user has agreed: the jar and a
//! Java runtime, each checked against its pinned SHA-256 (through
//! `manga::ocr::download`, which resumes and deletes what does not match),
//! then the runtime unpacked.
//!
//! On disk, in `app_data_dir/suwayomi/`:
//!
//! ```text
//! bin/<jar>                  the server, as released
//! jre/<release>/…            the runtime, unpacked; `.ready` once complete
//! downloads/                 archives on their way (resumable `.part` files)
//! data/                      the server's own root (`rootDir`)
//! server.log                 what it prints, from its last start
//! ```
//!
//! Blocking: runs on its own thread.

use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::AtomicBool;

use super::assets::{self, Runtime};
use crate::manga::ocr::download::{self, FetchError};

pub const BIN_DIR: &str = "bin";
pub const JRE_DIR: &str = "jre";
pub const DOWNLOADS_DIR: &str = "downloads";
const READY: &str = ".ready";

pub fn jar_path(dir: &Path) -> PathBuf {
    dir.join(BIN_DIR).join(assets::SERVER.file)
}

fn runtime_dir(dir: &Path, runtime: &Runtime) -> PathBuf {
    dir.join(JRE_DIR).join(runtime.release.replace('+', "_"))
}

#[cfg(test)]
pub fn runtime_dir_for_test(dir: &Path, runtime: &Runtime) -> PathBuf {
    runtime_dir(dir, runtime)
}

/// The `java` to run, once the runtime is unpacked.
pub fn java_path(dir: &Path, runtime: &Runtime) -> Option<PathBuf> {
    let root = runtime_dir(dir, runtime);
    let ready = std::fs::read_to_string(root.join(READY)).ok()?;
    (ready.trim() == runtime.archive.sha256).then(|| root.join(runtime.java)).filter(|java| java.is_file())
}

pub fn server_present(dir: &Path) -> bool {
    assets::SERVER.present(&dir.join(BIN_DIR))
}

pub fn installed(dir: &Path) -> bool {
    server_present(dir) && assets::runtime().is_some_and(|runtime| java_path(dir, &runtime).is_some())
}

/// Bytes the downloads still need: the jar and the runtime's archive, unless
/// they are there.
pub fn missing_bytes(dir: &Path) -> u64 {
    let server = if server_present(dir) { 0 } else { assets::SERVER.size };
    let runtime = assets::runtime().filter(|runtime| java_path(dir, runtime).is_none()).map_or(0, |runtime| runtime.archive.size);
    server + runtime
}

/// One step of the installation, for the progress callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Downloading,
    Verifying,
    Unpacking,
}

/// Downloads and unpacks what is missing. `progress(step, file, received)`
/// gets the bytes of the current file received over the network.
pub fn install(dir: &Path, cancel: &AtomicBool, progress: &mut dyn FnMut(Step, &str, u64)) -> Result<(), FetchError> {
    let runtime = assets::runtime().ok_or(FetchError::Server)?;
    let client = download::client();
    if !server_present(dir) {
        let asset = assets::SERVER;
        progress(Step::Downloading, asset.file, 0);
        download::fetch(&client, &asset, &dir.join(BIN_DIR), cancel, &mut |received| {
            let step = if received >= asset.size { Step::Verifying } else { Step::Downloading };
            progress(step, asset.file, received);
        })?;
    }
    if java_path(dir, &runtime).is_none() {
        let asset = runtime.archive;
        let downloads = dir.join(DOWNLOADS_DIR);
        progress(Step::Downloading, asset.file, 0);
        download::fetch(&client, &asset, &downloads, cancel, &mut |received| {
            let step = if received >= asset.size { Step::Verifying } else { Step::Downloading };
            progress(step, asset.file, received);
        })?;
        progress(Step::Unpacking, asset.file, asset.size);
        let archive = downloads.join(asset.file);
        unpack_runtime(&archive, &runtime_dir(dir, &runtime), runtime.zip, runtime.archive.sha256).map_err(|error| {
            log::warn!("Suwayomi: the Java runtime did not unpack: {error}");
            FetchError::Disk
        })?;
        let _ = std::fs::remove_file(archive);
    }
    Ok(())
}

/// A name inside an archive as a relative path, or `None` if it would leave
/// the folder it is unpacked into.
fn safe_relative(name: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for part in name.components() {
        match part {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

/// Unpacks a runtime archive into `target`, dropping its single top folder
/// (`jdk-21…-jre/`), through a staging folder so a half-unpacked runtime is
/// never taken for a whole one. `.ready` holds the archive's checksum.
pub fn unpack_runtime(archive: &Path, target: &Path, zip: bool, sha256: &str) -> std::io::Result<()> {
    let parent = target.parent().ok_or_else(|| std::io::Error::other("no parent"))?;
    std::fs::create_dir_all(parent)?;
    let staging = parent.join(format!(".staging-{}", uuid::Uuid::new_v4().simple()));
    let result = (|| {
        std::fs::create_dir_all(&staging)?;
        if zip {
            unpack_zip(archive, &staging)?;
        } else {
            let file = std::fs::File::open(archive)?;
            let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(std::io::BufReader::new(file)));
            tar.set_preserve_permissions(true);
            tar.set_overwrite(true);
            for entry in tar.entries()? {
                let mut entry = entry?;
                let path = entry.path()?.into_owned();
                if safe_relative(&path).is_none() {
                    return Err(std::io::Error::other("unsafe path in archive"));
                }
                // `unpack_in` refuses anything that would land outside, links included.
                entry.unpack_in(&staging)?;
            }
        }
        let mut tops = std::fs::read_dir(&staging)?.filter_map(Result::ok).filter(|entry| entry.path().is_dir());
        let top = tops.next().ok_or_else(|| std::io::Error::other("empty archive"))?.path();
        if tops.next().is_some() {
            return Err(std::io::Error::other("more than one top folder"));
        }
        if target.exists() {
            std::fs::remove_dir_all(target)?;
        }
        std::fs::rename(&top, target)?;
        let mut ready = std::fs::File::create(target.join(READY))?;
        ready.write_all(sha256.as_bytes())?;
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&staging);
    result
}

fn unpack_zip(archive: &Path, staging: &Path) -> std::io::Result<()> {
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(std::fs::File::open(archive)?)).map_err(std::io::Error::other)?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(std::io::Error::other)?;
        let name = entry.enclosed_name().and_then(|name| safe_relative(&name)).ok_or_else(|| std::io::Error::other("unsafe path in archive"))?;
        let path = staging.join(name);
        if entry.is_dir() {
            std::fs::create_dir_all(&path)?;
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = std::fs::File::create(&path)?;
        // A runtime file is never this big: a bad entry is not a disk filler.
        let copied = std::io::copy(&mut (&mut entry).take(512 * 1024 * 1024), &mut out)?;
        if copied >= 512 * 1024 * 1024 {
            return Err(std::io::Error::other("entry too large"));
        }
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode & 0o755))?;
        }
    }
    Ok(())
}

/// What the installation takes on disk now (jar, runtime, server data).
pub fn installed_bytes(dir: &Path) -> u64 {
    dir_size(dir)
}

pub fn dir_size(dir: &Path) -> u64 {
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

/// Deletes the server, its runtime and its data (installed extensions
/// included). The series added to the library stay, and so do chapters
/// downloaded for offline reading, which live with the app's own data.
pub fn delete_all(dir: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(dir) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manga::ocr::assets::{Asset, Source};
    use crate::manga::webdav::test_server::{serve_file, TestServer};
    use std::sync::atomic::Ordering;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ntdc-swy-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sha(bytes: &[u8]) -> String {
        use sha2::Digest;
        crate::manga::ocr::engine::hex(&sha2::Sha256::digest(bytes))
    }

    fn leak(value: String) -> &'static str {
        Box::leak(value.into_boxed_str())
    }

    fn tar_gz(files: &[(&str, &[u8], u32)]) -> Vec<u8> {
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast()));
        for (name, bytes, mode) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(*mode);
            header.set_cksum();
            builder.append_data(&mut header, name, *bytes).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    #[test]
    fn a_runtime_with_the_wrong_checksum_is_rejected_and_never_unpacked() {
        let good = tar_gz(&[("jdk-21-jre/bin/java", b"#!/bin/sh\n", 0o755)]);
        let served = good.clone();
        let server = TestServer::start(move |request| serve_file(request, &served, true));
        let dir = temp_dir();
        // The pinned checksum is of another file: the download is deleted.
        let wrong = Asset { file: "jre.tar.gz", size: good.len() as u64, sha256: leak(sha(b"another build")), source: Source::Url(leak(server.url("/jre.tar.gz"))) };
        let cancel = AtomicBool::new(false);
        assert_eq!(download::fetch(&download::client(), &wrong, &dir, &cancel, &mut |_| {}), Err(FetchError::Checksum));
        assert!(!dir.join("jre.tar.gz").exists() && !dir.join("jre.tar.gz.part").exists());

        let right = Asset { sha256: leak(sha(&good)), ..wrong };
        download::fetch(&download::client(), &right, &dir, &cancel, &mut |_| {}).unwrap();
        let target = dir.join("jre").join("21");
        unpack_runtime(&dir.join("jre.tar.gz"), &target, false, right.sha256).unwrap();
        assert!(target.join("bin/java").is_file());
        assert_eq!(std::fs::read_to_string(target.join(READY)).unwrap(), right.sha256);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_ne!(std::fs::metadata(target.join("bin/java")).unwrap().permissions().mode() & 0o111, 0);
        }
        assert!(!cancel.load(Ordering::Relaxed));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_archive_that_escapes_its_folder_is_refused() {
        let dir = temp_dir();
        // tar's builder refuses `..` itself, so write the name into the header by hand.
        let mut header = tar::Header::new_gnu();
        header.as_gnu_mut().unwrap().name[..9].copy_from_slice(b"../evil.s");
        header.set_size(1);
        header.set_mode(0o644);
        header.set_cksum();
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast()));
        builder.append(&header, &b"x"[..]).unwrap();
        let bytes = builder.into_inner().unwrap().finish().unwrap();
        std::fs::write(dir.join("bad.tar.gz"), bytes).unwrap();
        let target = dir.join("jre").join("x");
        assert!(unpack_runtime(&dir.join("bad.tar.gz"), &target, false, "s").is_err());
        assert!(!dir.join("jre").join("evil.s").exists() && !dir.join("evil.s").exists());
        assert!(!target.exists());

        let mut zip_bytes = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut zip_bytes));
            zip.start_file("jdk/bin/java.exe", zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(b"MZ").unwrap();
            zip.finish().unwrap();
        }
        std::fs::write(dir.join("ok.zip"), zip_bytes).unwrap();
        unpack_runtime(&dir.join("ok.zip"), &target, true, "z").unwrap();
        assert!(target.join("bin/java.exe").is_file());
        let _ = std::fs::remove_dir_all(dir);
    }
}
