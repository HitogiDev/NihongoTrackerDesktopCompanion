//! Reading progress shared with Mokuro Reader through its own file,
//! `mokuro-reader/volume-data.json`, on servers with mokuro-bunko's layout.
//! Opt-in (`manga_webdav_sync_progress`), because it writes to the server.
//!
//! The format, as Mokuro Reader writes it (Gnathonic/mokuro-reader,
//! `src/lib/settings/volume-data.ts`): one object whose keys are each
//! volume's `volume_uuid` (from its `.mokuro`) plus two reserved sections,
//! `series` and `tracking`. A volume's record has, among many other fields,
//! `progress` (the page, counting from 1), `completed` and
//! `lastProgressUpdate` (ISO date); copies are merged newest
//! `lastProgressUpdate` first. A record with `deletedOn` is a tombstone.
//!
//! What this does with it, and nothing else:
//!
//! * **pull**: a record newer than our progress moves our page there;
//! * **push**: our page goes into that volume's record (`progress`,
//!   `completed` only ever turned on, `lastProgressUpdate`), every other field
//!   and every other key kept as it was. The write is conditional on the
//!   ETag read, so a write by another device in between is read again, never
//!   overwritten. A file that is not JSON is never touched, nor a tombstone.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde_json::{Map, Value};

use super::client::{DavError, Precondition};
use super::remote::Remote;
use crate::db::Db;
use crate::manga::store::{is_remote_format, VolumeLocation};

pub const FILE_NAME: &str = "volume-data.json";
const RESERVED: &[&str] = &["series", "tracking"];
/// The file holds every volume the user has opened anywhere: not small, but
/// not this big.
const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;

/// A volume's progress as the file says.
#[derive(Debug, Clone, PartialEq)]
pub struct RemoteProgress {
    /// 0-based, like ours.
    pub page: i64,
    pub completed: bool,
    pub updated: Option<DateTime<Utc>>,
}

fn parse_date(value: Option<&Value>) -> Option<DateTime<Utc>> {
    value.and_then(Value::as_str).and_then(|raw| DateTime::parse_from_rfc3339(raw).ok()).map(|date| date.with_timezone(&Utc))
}

/// The live records of a file, by volume uuid.
pub fn read_records(file: &Value) -> HashMap<String, RemoteProgress> {
    let Some(object) = file.as_object() else { return HashMap::new() };
    object
        .iter()
        .filter(|(key, _)| !key.is_empty() && !RESERVED.contains(&key.as_str()))
        .filter_map(|(key, record)| {
            let record = record.as_object()?;
            if record.get("deletedOn").is_some_and(|v| !v.is_null()) {
                return None;
            }
            let progress = record.get("progress").and_then(Value::as_f64).unwrap_or(0.0);
            Some((
                key.clone(),
                RemoteProgress {
                    page: (progress as i64 - 1).max(0),
                    completed: record.get("completed").and_then(Value::as_bool).unwrap_or(false),
                    updated: parse_date(record.get("lastProgressUpdate")),
                },
            ))
        })
        .collect()
}

/// Our progress for one volume, to write.
#[derive(Debug, Clone)]
pub struct LocalProgress<'a> {
    pub uuid: &'a str,
    pub page: i64,
    pub finished: bool,
    pub updated: DateTime<Utc>,
    pub series_title: &'a str,
    pub volume_title: &'a str,
}

/// Puts our progress into the file, touching only that volume's position
/// fields. `false` when there is nothing to write: the file already says the
/// same or something newer, or the volume was deleted there.
pub fn apply_local(file: &mut Map<String, Value>, local: &LocalProgress<'_>) -> bool {
    if local.uuid.is_empty() || RESERVED.contains(&local.uuid) {
        return false;
    }
    let record = file.entry(local.uuid.to_string()).or_insert_with(|| Value::Object(Map::new()));
    let Some(record) = record.as_object_mut() else { return false };
    if record.get("deletedOn").is_some_and(|v| !v.is_null()) {
        return false;
    }
    let progress = local.page + 1;
    let completed = local.finished || record.get("completed").and_then(Value::as_bool).unwrap_or(false);
    let same = record.get("progress").and_then(Value::as_i64) == Some(progress)
        && record.get("completed").and_then(Value::as_bool).unwrap_or(false) == completed;
    let newer_there = parse_date(record.get("lastProgressUpdate")).is_some_and(|there| there >= local.updated);
    if same || newer_there {
        return false;
    }
    record.insert("progress".into(), Value::from(progress));
    if completed {
        record.insert("completed".into(), Value::Bool(true));
    }
    record.insert("lastProgressUpdate".into(), Value::String(local.updated.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)));
    // Self-describing, as Mokuro Reader writes them, but never over its own.
    record.entry("series_title").or_insert_with(|| Value::String(local.series_title.to_string()));
    record.entry("volume_title").or_insert_with(|| Value::String(local.volume_title.to_string()));
    true
}

/// `/mokuro-reader/volume-data.json` for a series in `/mokuro-reader/<Series>/`,
/// or `None` if the server is not laid out like that (then Mokuro Reader would
/// not find the file either).
pub fn file_path(series_path: &str) -> Option<String> {
    let trimmed = series_path.trim_end_matches('/');
    let (base, _) = trimmed.rsplit_once('/')?;
    let base_name = base.rsplit('/').next()?;
    base_name.eq_ignore_ascii_case(super::layout::LIBRARY_FOLDER).then(|| format!("{base}/{FILE_NAME}"))
}

fn uuid_key(volume_id: &str) -> String {
    format!("manga:uuid:{volume_id}")
}

/// A volume's `volume_uuid`, from its `.mokuro` (remembered once read).
fn volume_uuid(remote: &Remote, db: &Db, location: &VolumeLocation) -> Option<String> {
    if let Some(uuid) = db.get_kv(&uuid_key(&location.id)) {
        return Some(uuid);
    }
    let path = remote.text_path(location, None)?;
    let uuid = crate::manga::mokuro::load(&path).ok()?.volume_uuid.filter(|uuid| !uuid.trim().is_empty())?;
    let _ = db.set_kv(&uuid_key(&location.id), &uuid);
    Some(uuid)
}

/// Takes newer progress from the file for a series' volumes. The number of
/// volumes moved. Blocking.
pub fn pull_series(remote: &Remote, db: &Db, series_id: &str) -> Result<usize, DavError> {
    let series = db.manga_series(series_id).map_err(|e| DavError::Other(e.to_string()))?.ok_or(DavError::NotFound)?;
    let Some(path) = file_path(&series.location) else { return Ok(0) };
    let source_id = db.manga_series_source(series_id).map_err(|e| DavError::Other(e.to_string()))?.ok_or(DavError::NotFound)?;
    let client = remote.client(&source_id)?;
    let Some((body, _)) = remote.block_on(&source_id, client.get_tagged(&path, MAX_FILE_BYTES))? else { return Ok(0) };
    let Ok(file) = serde_json::from_slice::<Value>(&body) else { return Ok(0) };
    let records = read_records(&file);
    let mut moved = 0;
    for volume in db.manga_volumes(series_id).map_err(|e| DavError::Other(e.to_string()))? {
        if !is_remote_format(&volume.format) {
            continue;
        }
        let Ok(Some(location)) = db.manga_volume_location(&volume.id) else { continue };
        let Some(record) = volume_uuid(remote, db, &location).and_then(|uuid| records.get(&uuid).cloned()) else { continue };
        let ours = volume.read_at.as_deref().and_then(|raw| DateTime::parse_from_rfc3339(raw).ok()).map(|d| d.with_timezone(&Utc));
        let newer = match (record.updated, ours) {
            (Some(theirs), Some(ours)) => theirs > ours,
            (Some(_), None) => true,
            (None, _) => false,
        };
        if newer && (volume.page != Some(record.page) || (record.completed && !volume.finished)) {
            db.set_manga_progress(&volume.id, record.page, record.completed).map_err(|e| DavError::Other(e.to_string()))?;
            moved += 1;
        }
    }
    Ok(moved)
}

/// Writes a volume's progress into the file. `true` if it wrote. Blocking.
pub fn push_volume(remote: &Remote, db: &Db, volume_id: &str) -> Result<bool, DavError> {
    let other = |e: rusqlite::Error| DavError::Other(e.to_string());
    let location = db.manga_volume_location(volume_id).map_err(other)?.ok_or(DavError::NotFound)?;
    if !is_remote_format(&location.format) {
        return Ok(false);
    }
    let series = db.manga_series(&location.series_id).map_err(other)?.ok_or(DavError::NotFound)?;
    let volume = db.manga_volume(volume_id).map_err(other)?.ok_or(DavError::NotFound)?;
    let (Some(path), Some(page)) = (file_path(&series.location), volume.page) else { return Ok(false) };
    let Some(uuid) = volume_uuid(remote, db, &location) else { return Ok(false) };
    let updated = volume.read_at.as_deref().and_then(|raw| DateTime::parse_from_rfc3339(raw).ok()).map_or_else(Utc::now, |d| d.with_timezone(&Utc));
    let local = LocalProgress { uuid: &uuid, page, finished: volume.finished, updated, series_title: &series.title, volume_title: &volume.title };
    let client = remote.client(&location.source_id)?;
    // Read, change, write if nobody wrote in between; once more if someone did.
    for _ in 0..2 {
        let current = remote.block_on(&location.source_id, client.get_tagged(&path, MAX_FILE_BYTES))?;
        let (mut file, precondition_tag) = match &current {
            Some((body, etag)) => match serde_json::from_slice::<Value>(body) {
                Ok(Value::Object(file)) => (file, Some(etag.clone())),
                // Not ours to fix: leave it alone.
                _ => return Ok(false),
            },
            None => (Map::new(), None),
        };
        if !apply_local(&mut file, &local) {
            return Ok(false);
        }
        let precondition = match &precondition_tag {
            Some(Some(etag)) => Precondition::Match(etag),
            Some(None) => Precondition::Unconditional,
            None => Precondition::Absent,
        };
        let body = serde_json::to_vec(&Value::Object(file)).map_err(|e| DavError::Other(e.to_string()))?;
        match remote.block_on(&location.source_id, client.put_if_unchanged(&path, body, precondition)) {
            Ok(()) => return Ok(true),
            Err(DavError::Changed) => continue,
            Err(error) => return Err(error),
        }
    }
    Err(DavError::Changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manga::store::tests::temp_db;
    use crate::manga::store::{RemoteVolumeRecord, FORMAT_REMOTE_ARCHIVE, SOURCE_WEBDAV};
    use crate::manga::webdav::test_server::{Reply, TestServer};
    use std::sync::{Arc, Mutex};

    /// The shape Mokuro Reader writes, by hand: a volume with fields this app
    /// does not know, a tombstone and both reserved sections.
    const FILE: &str = r#"{
        "uuid-1": {"progress": 12, "chars": 3400, "timeReadInMinutes": 25, "lastProgressUpdate": "2026-11-01T10:00:00.000Z",
                   "settings": {"rightToLeft": true}, "recentPageTurns": [[1, 2, 3]], "series_title": "S"},
        "uuid-gone": {"progress": 5, "deletedOn": "2026-10-01T00:00:00.000Z"},
        "series": {"S": {"lastUpdated": "2026-11-01T10:00:00.000Z"}},
        "tracking": {"idleCutoff": 5}
    }"#;

    fn date(raw: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(raw).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn records_are_read_without_the_reserved_sections_or_tombstones() {
        let records = read_records(&serde_json::from_str(FILE).unwrap());
        assert_eq!(records.len(), 1);
        assert_eq!(records["uuid-1"], RemoteProgress { page: 11, completed: false, updated: Some(date("2026-11-01T10:00:00Z")) });
    }

    #[test]
    fn writing_changes_only_the_position_and_keeps_everything_else() {
        let mut file: Map<String, Value> = serde_json::from_str(FILE).unwrap();
        let local = LocalProgress { uuid: "uuid-1", page: 19, finished: false, updated: date("2026-11-02T08:00:00Z"), series_title: "X", volume_title: "V" };
        assert!(apply_local(&mut file, &local));
        let record = &file["uuid-1"];
        assert_eq!(record["progress"], 20);
        assert_eq!(record["lastProgressUpdate"], "2026-11-02T08:00:00.000Z");
        assert_eq!((record["chars"].clone(), record["timeReadInMinutes"].clone()), (Value::from(3400), Value::from(25)));
        assert_eq!(record["settings"]["rightToLeft"], true);
        assert_eq!(record["series_title"], "S", "its own title is kept");
        assert_eq!(file["tracking"]["idleCutoff"], 5);
        assert!(file["series"].is_object());

        // Older than what is there, the same, a tombstone, a reserved key: nothing.
        let older = LocalProgress { updated: date("2026-10-01T00:00:00Z"), page: 1, ..local.clone() };
        assert!(!apply_local(&mut file, &older));
        assert!(!apply_local(&mut file, &LocalProgress { updated: date("2026-11-03T00:00:00Z"), ..local.clone() }));
        assert!(!apply_local(&mut file, &LocalProgress { uuid: "uuid-gone", updated: date("2026-12-01T00:00:00Z"), ..local.clone() }));
        assert!(!apply_local(&mut file, &LocalProgress { uuid: "series", ..local.clone() }));

        // A new volume gets a record of its own; finishing never un-finishes.
        let new = LocalProgress { uuid: "uuid-2", page: 0, finished: true, updated: date("2026-11-04T00:00:00Z"), series_title: "X", volume_title: "V2" };
        assert!(apply_local(&mut file, &new));
        assert_eq!(file["uuid-2"]["completed"], true);
        assert_eq!(file["uuid-2"]["volume_title"], "V2");
    }

    #[test]
    fn only_a_bunko_layout_has_the_file() {
        assert_eq!(file_path("/mokuro-reader/Hoshi/").as_deref(), Some("/mokuro-reader/volume-data.json"));
        assert_eq!(file_path("/dav/Mokuro-Reader/Hoshi/").as_deref(), Some("/dav/Mokuro-Reader/volume-data.json"));
        assert_eq!(file_path("/dav/Hoshi/"), None);
        assert_eq!(file_path("/"), None);
    }

    #[test]
    fn pull_and_push_against_a_server_with_etags() {
        let stored = Arc::new(Mutex::new((FILE.as_bytes().to_vec(), 1u32)));
        let mokuro = r#"{"volume_uuid":"uuid-1","pages":[]}"#;
        let state = stored.clone();
        let server = TestServer::start(move |request| {
            let mut file = state.lock().unwrap();
            match (request.method.as_str(), request.path.as_str()) {
                ("GET", "/mokuro-reader/volume-data.json") => Reply::new(200, file.0.clone()).header("ETag", format!("\"v{}\"", file.1)),
                ("PUT", "/mokuro-reader/volume-data.json") => {
                    if request.header("if-match") != Some(&format!("\"v{}\"", file.1)) {
                        return Reply::status(412);
                    }
                    file.0 = request.body.clone();
                    file.1 += 1;
                    Reply::status(204)
                }
                ("GET", "/mokuro-reader/S/V.mokuro") => Reply::new(200, mokuro.as_bytes().to_vec()),
                _ => Reply::status(404),
            }
        });
        let (db, db_path) = temp_db();
        let db = Arc::new(db);
        db.add_manga_source("dav", SOURCE_WEBDAV, "S", &server.url("/")).unwrap();
        db.apply_remote_series("dav", &[("s".into(), "/mokuro-reader/S/".into(), "S".into())]).unwrap();
        db.apply_remote_volumes("s", None, None, &[RemoteVolumeRecord {
            id: "v".into(), title: "V".into(), source_ref: "/mokuro-reader/S/V.cbz".into(), format: FORMAT_REMOTE_ARCHIVE,
            position: 1, pages: vec!["1.jpg".into(); 30], signature: Some("x".into()), mokuro_path: Some("/mokuro-reader/S/V.mokuro".into()),
            layers: Vec::new(), cover: None, size: Some(10),
        }]).unwrap();
        let root = std::env::temp_dir().join(format!("ntdc-progress-{}", uuid::Uuid::new_v4()));
        let remote = Remote::new(db.clone(), root.join("c"), root.join("o"), u64::MAX, None);

        // Never read here: the file's page wins.
        assert_eq!(pull_series(&remote, &db, "s").unwrap(), 1);
        assert_eq!(db.manga_volume("v").unwrap().unwrap().page, Some(11));
        assert_eq!(pull_series(&remote, &db, "s").unwrap(), 0, "ours is the newer now");

        // Read on here: ours goes there, the rest of the file untouched.
        db.set_manga_progress("v", 24, false).unwrap();
        assert!(push_volume(&remote, &db, "v").unwrap());
        let written: Value = serde_json::from_slice(&stored.lock().unwrap().0).unwrap();
        assert_eq!(written["uuid-1"]["progress"], 25);
        assert_eq!(written["uuid-1"]["chars"], 3400);
        assert_eq!(written["tracking"]["idleCutoff"], 5);
        assert!(!push_volume(&remote, &db, "v").unwrap(), "nothing new to write");
        drop(remote);
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_file(db_path);
    }
}
