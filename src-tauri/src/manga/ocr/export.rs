//! A volume's text as a `.mokuro` file, so Mokuro Reader and other clients
//! read what the OCR (and the user's corrections) found: next to a local
//! volume, or uploaded beside a volume on a WebDAV server.
//!
//! The file holds every page in the volume's order: its `.mokuro` text, the
//! OCR's where that had none, the user's corrections over both, and an empty
//! page where there is no text at all. A volume that already had a
//! `.mokuro` keeps its title and uuids, so other clients (and Mokuro
//! Reader's progress) still know it. Nothing is overwritten unless the user
//! confirmed it; on WebDAV the write is conditional, so a file someone else
//! changed meanwhile is never replaced.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::db::Db;
use crate::manga::mokuro::{self, MokuroPage, MokuroVolume};
use crate::manga::store::{is_remote_format, VolumeLocation, FORMAT_ARCHIVE, FORMAT_FOLDER, FORMAT_REMOTE_ARCHIVE};
use crate::manga::webdav::client::{DavError, Precondition};
use crate::manga::webdav::remote::Remote;

/// The format version written. mokuro's own files say `0.2.x`.
const VERSION: &str = "0.2.1";
/// For the uuids of volumes that never had a `.mokuro`.
const NAMESPACE: uuid::Uuid = uuid::Uuid::from_u128(0x6e74_6463_6f63_7200_8000_0000_6d6b_7572);

/// Where an export went, for the UI.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct OcrExport {
    /// The file written: a path on this computer, or on the server.
    pub path: String,
    pub pages: i64,
    /// Pages that have text in it.
    pub pages_with_text: i64,
    pub remote: bool,
}

/// Where a local volume's `.mokuro` goes: beside its archive or folder,
/// with the same name (`Vol.cbz` / `Vol/` → `Vol.mokuro`).
pub fn local_target(location: &VolumeLocation) -> Option<PathBuf> {
    let path = Path::new(&location.source_ref);
    let stem = match location.format.as_str() {
        FORMAT_ARCHIVE => path.file_stem()?,
        FORMAT_FOLDER => path.file_name()?,
        _ => return None,
    };
    Some(path.parent()?.join(format!("{}.mokuro", stem.to_string_lossy())))
}

/// The same on the server: `/S/Vol.cbz` or `/S/Vol/` → `/S/Vol.mokuro`.
pub fn remote_target(location: &VolumeLocation) -> Option<String> {
    let path = location.source_ref.trim_end_matches('/');
    let (parent, name) = path.rsplit_once('/')?;
    let stem = if location.format == FORMAT_REMOTE_ARCHIVE { name.rsplit_once('.').map_or(name, |(stem, _)| stem) } else { name };
    (!stem.is_empty()).then(|| format!("{parent}/{stem}.mokuro"))
}

/// The `.mokuro` the volume had, if any (its title and uuids are kept).
fn existing(remote: Option<&Remote>, location: &VolumeLocation) -> Option<MokuroVolume> {
    let path = if is_remote_format(&location.format) {
        remote?.text_path(location, None)?
    } else {
        PathBuf::from(location.mokuro_path.as_ref()?)
    };
    mokuro::load(&path).ok()
}

/// A page with no text: its image's size when it can be read cheaply.
fn empty_page(remote: Option<&Remote>, location: &VolumeLocation, index: usize) -> MokuroPage {
    let size = if is_remote_format(&location.format) {
        // Not worth downloading every page for its size.
        None
    } else {
        super::super::protocol::page_bytes(remote, location, index).and_then(|bytes| {
            image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().ok()?.into_dimensions().ok()
        })
    };
    MokuroPage {
        img_path: location.pages.get(index).cloned().unwrap_or_default(),
        img_width: size.map_or(0.0, |(w, _)| f64::from(w)),
        img_height: size.map_or(0.0, |(_, h)| f64::from(h)),
        blocks: Vec::new(),
    }
}

/// The whole volume as a `.mokuro`.
pub fn build(db: &Db, remote: Option<&Remote>, volume_id: &str) -> Result<MokuroVolume, String> {
    let location = db.manga_volume_location(volume_id).map_err(|e| e.to_string())?.ok_or("not-found")?;
    let volume = db.manga_volume(volume_id).map_err(|e| e.to_string())?.ok_or("not-found")?;
    let series = db.manga_series(&volume.series_id).map_err(|e| e.to_string())?.ok_or("not-found")?;
    let text = mokuro::volume_text(db, remote, volume_id)?;
    if text.iter().all(Option::is_none) {
        return Err("no-text".into());
    }
    let before = existing(remote, &location);
    let uuid_of = |name: &str| uuid::Uuid::new_v5(&NAMESPACE, name.as_bytes()).to_string();
    let pages = text
        .into_iter()
        .enumerate()
        .map(|(index, page)| match page {
            Some(mut page) => {
                if page.img_path.is_empty() {
                    page.img_path = location.pages.get(index).cloned().unwrap_or_default();
                }
                page
            }
            None => empty_page(remote, &location, index),
        })
        .collect();
    Ok(MokuroVolume {
        version: Some(VERSION.to_string()),
        title: before.as_ref().and_then(|v| v.title.clone()).or(Some(series.title.clone())),
        title_uuid: before.as_ref().and_then(|v| v.title_uuid.clone()).or_else(|| Some(uuid_of(&series.id))),
        volume: before.as_ref().and_then(|v| v.volume.clone()).or(Some(volume.title.clone())),
        volume_uuid: before.as_ref().and_then(|v| v.volume_uuid.clone()).or_else(|| Some(uuid_of(&volume.id))),
        pages,
    })
}

fn summary(file: &MokuroVolume, path: String, remote: bool) -> OcrExport {
    OcrExport {
        path,
        pages: file.pages.len() as i64,
        pages_with_text: file.pages.iter().filter(|page| !page.blocks.is_empty()).count() as i64,
        remote,
    }
}

/// Writes the `.mokuro` beside a local volume. `exists` unless `overwrite`.
pub fn to_local(db: &Db, volume_id: &str, overwrite: bool) -> Result<OcrExport, String> {
    let location = db.manga_volume_location(volume_id).map_err(|e| e.to_string())?.ok_or("not-found")?;
    let target = local_target(&location).ok_or("not-local")?;
    if target.exists() && !overwrite {
        return Err("exists".into());
    }
    let file = build(db, None, volume_id)?;
    let json = serde_json::to_vec(&file).map_err(|e| e.to_string())?;
    let partial = target.with_extension("mokuro.part");
    std::fs::write(&partial, &json).and_then(|()| std::fs::rename(&partial, &target)).map_err(|error| {
        let _ = std::fs::remove_file(&partial);
        log::warn!("export of {volume_id} to {}: {error}", target.display());
        "disk".to_string()
    })?;
    // Now it is the volume's `.mokuro`, as the next scan would find.
    db.set_manga_volume_mokuro(volume_id, &target.to_string_lossy()).map_err(|e| e.to_string())?;
    Ok(summary(&file, target.to_string_lossy().into_owned(), false))
}

/// Uploads the `.mokuro` beside a volume on its WebDAV server. Without
/// `overwrite` it is only written where there is none (`exists` otherwise);
/// with it, only over the version just read. A server that does not let
/// this user write answers `read-only`.
pub fn to_webdav(db: &Db, remote: &Remote, volume_id: &str, overwrite: bool) -> Result<OcrExport, String> {
    let location = db.manga_volume_location(volume_id).map_err(|e| e.to_string())?.ok_or("not-found")?;
    if !is_remote_format(&location.format) {
        return Err("not-remote".into());
    }
    let target = remote_target(&location).ok_or("not-remote")?;
    let file = build(db, Some(remote), volume_id)?;
    let json = serde_json::to_vec(&file).map_err(|e| e.to_string())?;
    let client = remote.client(&location.source_id).map_err(|e| e.code().to_string())?;
    let current = remote.block_on(&location.source_id, client.get_tagged(&target, 64 * 1024 * 1024)).map_err(|e| e.code().to_string())?;
    let etag = match &current {
        Some(_) if !overwrite => return Err("exists".into()),
        Some((_, etag)) => etag.clone(),
        None => None,
    };
    let precondition = match (&current, etag.as_deref()) {
        (None, _) => Precondition::Absent,
        (Some(_), Some(etag)) => Precondition::Match(etag),
        (Some(_), None) => Precondition::Unconditional,
    };
    remote
        .block_on(&location.source_id, client.put_if_unchanged(&target, json, precondition))
        .map_err(|error| match error {
            DavError::Unauthorized | DavError::Status(403 | 405 | 501) => "read-only".to_string(),
            DavError::Changed => "changed".to_string(),
            other => other.code().to_string(),
        })?;
    // Mokuro Reader's progress is keyed by this uuid.
    if let Some(uuid) = &file.volume_uuid {
        let _ = db.set_kv(&crate::manga::webdav::progress::uuid_key(volume_id), uuid);
    }
    Ok(summary(&file, target, true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manga::mokuro::MokuroBlock;
    use crate::manga::store::tests::{series, temp_db, volume};
    use crate::manga::store::{VolumeRecord, ENGINE_BUILTIN, ENGINE_MANUAL, SOURCE_LOCAL};

    fn location(format: &str, source_ref: &str) -> VolumeLocation {
        VolumeLocation {
            id: "v".into(), series_id: "s".into(), title: "Vol".into(), source_ref: source_ref.into(), format: format.into(),
            pages: Vec::new(), signature: None, mokuro_path: None, source_id: "src".into(), cover: None, layers: Vec::new(), size: None,
        }
    }

    #[test]
    fn the_file_goes_beside_the_volume_with_its_name() {
        assert_eq!(local_target(&location(FORMAT_ARCHIVE, "/m/S/第1巻.cbz")).unwrap(), PathBuf::from("/m/S/第1巻.mokuro"));
        assert_eq!(local_target(&location(FORMAT_FOLDER, "/m/S/Vol 2")).unwrap(), PathBuf::from("/m/S/Vol 2.mokuro"));
        assert_eq!(remote_target(&location(FORMAT_REMOTE_ARCHIVE, "/mokuro-reader/S/Vol.1.cbz")).unwrap(), "/mokuro-reader/S/Vol.1.mokuro");
        assert_eq!(remote_target(&location("remote-folder", "/S/Vol/")).unwrap(), "/S/Vol.mokuro");
        assert!(local_target(&location(FORMAT_REMOTE_ARCHIVE, "/S/V.cbz")).is_none());
    }

    #[test]
    fn a_local_volume_gets_a_mokuro_with_ocr_and_corrections_and_never_overwrites_by_surprise() {
        let (db, path) = temp_db();
        let dir = std::env::temp_dir().join(format!("ntdc-export-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("Vol")).unwrap();
        for name in ["001.png", "002.png", "003.png"] {
            std::fs::write(dir.join("Vol").join(name), crate::manga::thumbs::tests::png(40, 60)).unwrap();
        }
        let record = VolumeRecord {
            source_ref: dir.join("Vol").to_string_lossy().into_owned(),
            format: FORMAT_FOLDER,
            pages: vec!["001.png".into(), "002.png".into(), "003.png".into()],
            ..volume("v1", 1, 0)
        };
        db.add_manga_source("src", SOURCE_LOCAL, "M", "/m").unwrap();
        db.apply_manga_scan("src", SOURCE_LOCAL, &[series("s", "星", vec![record])]).unwrap();
        assert_eq!(build(&db, None, "v1"), Err("no-text".to_string()));

        let page = |text: &str, name: &str| MokuroPage {
            img_path: name.into(), img_width: 40.0, img_height: 60.0,
            blocks: vec![MokuroBlock { bbox: [1.0, 1.0, 10.0, 30.0], vertical: true, lines: vec![text.into()], ..Default::default() }],
        };
        db.set_manga_page_text("v1", 0, &serde_json::to_string(&page("読んだ", "001.png")).unwrap(), ENGINE_BUILTIN).unwrap();
        db.set_manga_page_text("v1", 2, &serde_json::to_string(&page("直した", "003.png")).unwrap(), ENGINE_MANUAL).unwrap();

        let done = to_local(&db, "v1", false).unwrap();
        assert_eq!((done.pages, done.pages_with_text), (3, 2));
        let written = mokuro::load(&dir.join("Vol.mokuro")).unwrap();
        assert_eq!(written.title.as_deref(), Some("星"));
        assert_eq!(written.volume.as_deref(), Some("Vol 1"));
        assert_eq!(mokuro::page_text(&written.pages[0]), ["読んだ"]);
        // The page without text is there, with its image's size.
        assert!(written.pages[1].blocks.is_empty());
        assert_eq!((written.pages[1].img_path.as_str(), written.pages[1].img_width), ("002.png", 40.0));
        assert_eq!(mokuro::page_text(&written.pages[2]), ["直した"]);
        // It is the volume's `.mokuro` now.
        assert_eq!(db.manga_volume("v1").unwrap().unwrap().ocr_state, "mokuro");

        assert_eq!(to_local(&db, "v1", false), Err("exists".to_string()));
        let again = to_local(&db, "v1", true).unwrap();
        // Its uuids stay the same from one export to the next.
        assert_eq!(mokuro::load(&dir.join("Vol.mokuro")).unwrap().volume_uuid, written.volume_uuid);
        assert_eq!(again.pages_with_text, 2);
        drop(db);
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_remote_volume_is_uploaded_beside_it_only_where_the_server_allows() {
        use crate::manga::store::{RemoteVolumeRecord, SOURCE_WEBDAV};
        use crate::manga::webdav::test_server::{Reply, TestServer};
        use std::sync::{Arc, Mutex};

        // The server: a `.mokuro` (with its uuid) for V, a writable folder
        // for W's, and a read-only folder for X's.
        let files = Arc::new(Mutex::new(std::collections::HashMap::<String, (Vec<u8>, u32)>::new()));
        files.lock().unwrap().insert("/m/S/V.mokuro".into(), (br#"{"volume_uuid":"uuid-v","title":"S","pages":[]}"#.to_vec(), 1));
        let state = files.clone();
        let server = TestServer::start(move |request| {
            let mut files = state.lock().unwrap();
            let path = percent_encoding::percent_decode_str(&request.path).decode_utf8_lossy().into_owned();
            match request.method.as_str() {
                "GET" => match files.get(&path) {
                    Some((body, version)) => Reply::new(200, body.clone()).header("ETag", format!("\"{version}\"")),
                    None => Reply::status(404),
                },
                "PUT" if path.starts_with("/m/X/") => Reply::status(403),
                "PUT" => {
                    let current = files.get(&path).map(|(_, version)| format!("\"{version}\""));
                    let allowed = match (request.header("if-none-match"), request.header("if-match")) {
                        (Some("*"), _) => current.is_none(),
                        (_, Some(tag)) => current.as_deref() == Some(tag),
                        _ => true,
                    };
                    if !allowed {
                        return Reply::status(412);
                    }
                    let version = files.get(&path).map_or(1, |(_, version)| version + 1);
                    files.insert(path, (request.body.clone(), version));
                    Reply::status(201)
                }
                _ => Reply::status(405),
            }
        });
        let (db, db_path) = temp_db();
        let db = Arc::new(db);
        db.add_manga_source("dav", SOURCE_WEBDAV, "S", &server.url("/")).unwrap();
        db.apply_remote_series("dav", &[("s".into(), "/m/S/".into(), "S".into())]).unwrap();
        let record = |id: &str, folder: &str, mokuro: Option<&str>| RemoteVolumeRecord {
            id: id.into(), title: id.into(), source_ref: format!("/m/{folder}/{id}.cbz"), format: FORMAT_REMOTE_ARCHIVE,
            position: 1, pages: vec!["1.jpg".into(), "2.jpg".into()], signature: Some("x".into()), mokuro_path: mokuro.map(str::to_string),
            layers: Vec::new(), cover: None, size: Some(10),
        };
        db.apply_remote_volumes("s", None, None, &[record("V", "S", Some("/m/S/V.mokuro")), record("W", "S", None), record("X", "X", None)]).unwrap();
        let root = std::env::temp_dir().join(format!("ntdc-export-dav-{}", uuid::Uuid::new_v4()));
        let remote = Remote::new(db.clone(), root.join("c"), root.join("o"), u64::MAX, None);
        let page = MokuroPage { img_path: "2.jpg".into(), img_width: 10.0, img_height: 10.0, blocks: vec![MokuroBlock { lines: vec!["あ".into()], ..Default::default() }] };
        for id in ["V", "W", "X"] {
            db.set_manga_page_text(id, 1, &serde_json::to_string(&page).unwrap(), ENGINE_BUILTIN).unwrap();
        }

        // W has none: written, with a fresh uuid remembered for progress.
        let done = to_webdav(&db, &remote, "W", false).unwrap();
        assert_eq!(done.path, "/m/S/W.mokuro");
        let written: MokuroVolume = serde_json::from_slice(&files.lock().unwrap()["/m/S/W.mokuro"].0).unwrap();
        assert_eq!(mokuro::page_text(&written.pages[1]), ["あ"]);
        assert_eq!(db.get_kv("manga:uuid:W"), written.volume_uuid);
        // V has one: not without confirming; then over it, keeping its uuid.
        assert_eq!(to_webdav(&db, &remote, "V", false), Err("exists".to_string()));
        to_webdav(&db, &remote, "V", true).unwrap();
        let replaced: MokuroVolume = serde_json::from_slice(&files.lock().unwrap()["/m/S/V.mokuro"].0).unwrap();
        assert_eq!(replaced.volume_uuid.as_deref(), Some("uuid-v"));
        // X's folder is read-only for this user.
        assert_eq!(to_webdav(&db, &remote, "X", false), Err("read-only".to_string()));
        drop(remote);
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_file(db_path);
    }
}
