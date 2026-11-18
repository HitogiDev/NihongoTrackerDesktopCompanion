//! Listing a WebDAV source into the library: the series folders on every
//! sync, and each series' own folder only when it changed (its ETag or date),
//! a few at a time. A server that cannot be reached changes nothing: its
//! series stay as they were last seen, and the source says it is offline.

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

use futures_util::stream::{self, StreamExt};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use super::client::{parse_base_url, Auth, Client, DavError};
use super::layout::{self, SeriesContents, SeriesFolder, VolumeKind};
use crate::db::Db;
use crate::manga::library::{stable_id, LIBRARY_CHANGED};
use crate::manga::store::{RemoteVolumeRecord, SourceRow, FORMAT_REMOTE_ARCHIVE, FORMAT_REMOTE_FOLDER, SOURCE_WEBDAV};
use crate::state::AppState;

/// Progress of a sync, to the Manga screen.
pub const SYNC_EVENT: &str = "manga://webdav-sync";
/// Series folders listed at once.
const PARALLEL_LISTINGS: usize = 4;
/// Opening the Manga screen syncs a server, but not more often than this.
const AUTO_SYNC_EVERY: Duration = Duration::from_secs(10 * 60);

/// What "Test connection" found.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct WebdavProbe {
    /// Series folders in the library.
    pub series: usize,
    /// Where the library is on the server (`/mokuro-reader/`).
    pub library_path: String,
    /// The library is a `mokuro-reader/` folder, as mokuro-bunko serves it.
    pub bunko: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct WebdavSyncProgress {
    pub source_id: String,
    /// Series folders listed so far, of `total` that needed it.
    pub done: usize,
    pub total: usize,
    pub running: bool,
    /// The error code, when it stopped on one.
    pub error: Option<String>,
}

pub fn series_id(source_id: &str, path: &str) -> String {
    stable_id("webdav-series", Path::new(&format!("{source_id}\0{path}")))
}

pub fn volume_id(source_id: &str, path: &str) -> String {
    stable_id("webdav", Path::new(&format!("{source_id}\0{path}")))
}

/// The client for a stored source, with its credentials.
pub fn client_for(row: &SourceRow) -> Result<Client, DavError> {
    let (url, _) = parse_base_url(&row.url)?;
    Ok(Client::new(url, Auth::from_json(row.auth_json.as_deref())))
}

/// The library on the server and its series folders.
pub async fn discover(client: &Client) -> Result<(String, Vec<SeriesFolder>), DavError> {
    let root = client.base_path();
    let entries = client.list(&root).await?;
    let base = layout::library_base(&root, &entries);
    let base_entries = if base == root { entries } else { client.list(&base).await? };
    let title = base.trim_end_matches('/').rsplit('/').next().filter(|name| !name.is_empty()).map(str::to_string)
        .unwrap_or_else(|| client.url_for("/").host_str().unwrap_or("WebDAV").to_string());
    Ok((base.clone(), layout::series_folders(&base, &base_entries, &title)))
}

pub async fn probe(client: &Client) -> Result<WebdavProbe, DavError> {
    let (base, series) = discover(client).await?;
    let bunko = base.trim_end_matches('/').rsplit('/').next().is_some_and(|name| name.eq_ignore_ascii_case(layout::LIBRARY_FOLDER));
    Ok(WebdavProbe { series: series.len(), library_path: base, bunko })
}

/// A series folder, and the subfolders that may be volumes.
async fn list_series(client: &Client, folder: &SeriesFolder) -> Result<SeriesContents, DavError> {
    let entries = client.list(&folder.path).await?;
    let mut subfolders = HashMap::new();
    for path in layout::folder_candidates(&entries) {
        match client.list(&path).await {
            Ok(listing) => {
                subfolders.insert(path, listing);
            }
            Err(DavError::NotFound | DavError::NotWebdav | DavError::Unauthorized) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(layout::series_contents(&entries, &subfolders))
}

fn records(source_id: &str, contents: &SeriesContents) -> Vec<RemoteVolumeRecord> {
    contents
        .volumes
        .iter()
        .enumerate()
        .map(|(index, volume)| RemoteVolumeRecord {
            id: volume_id(source_id, &volume.path),
            title: volume.title.clone(),
            source_ref: volume.path.clone(),
            format: match volume.kind {
                VolumeKind::Archive => FORMAT_REMOTE_ARCHIVE,
                VolumeKind::Folder => FORMAT_REMOTE_FOLDER,
            },
            position: index as i64 + 1,
            pages: volume.pages.clone(),
            signature: volume.signature.clone(),
            mokuro_path: volume.mokuro.clone(),
            layers: volume.layers.clone(),
            cover: volume.cover.clone(),
            size: volume.size.and_then(|size| i64::try_from(size).ok()),
        })
        .collect()
}

#[derive(Debug, Default, PartialEq)]
pub struct SyncOutcome {
    pub series: usize,
    pub listed: usize,
    /// Volumes that changed or are gone: their caches are stale.
    pub stale: Vec<String>,
}

/// Lists a source into the database. `force` lists every series folder, not
/// only the ones whose folder changed.
pub async fn sync_with(db: &Db, source_id: &str, force: bool, progress: impl Fn(usize, usize)) -> Result<SyncOutcome, DavError> {
    let row = db.manga_source_row(source_id).map_err(|e| DavError::Other(e.to_string()))?.ok_or(DavError::NotFound)?;
    let client = client_for(&row)?;
    let (_, folders) = discover(&client).await?;
    let found: Vec<(String, String, String)> =
        folders.iter().map(|folder| (series_id(source_id, &folder.path), folder.path.clone(), folder.title.clone())).collect();
    let db_error = |e: rusqlite::Error| DavError::Other(e.to_string());
    let mut outcome = SyncOutcome { series: found.len(), stale: db.apply_remote_series(source_id, &found).map_err(db_error)?, listed: 0 };

    let known = db.remote_series_signatures(source_id).map_err(db_error)?;
    let todo: Vec<&SeriesFolder> = folders
        .iter()
        .filter(|folder| {
            let stored = known.get(&series_id(source_id, &folder.path)).cloned().flatten();
            force || folder.signature.is_none() || stored.is_none() || stored != folder.signature
        })
        .collect();
    let total = todo.len();
    progress(0, total);
    // Owned futures: `buffer_unordered` over borrowing ones is not `Send`
    // enough for a spawned task.
    let todo: Vec<SeriesFolder> = todo.into_iter().cloned().collect();
    let mut listings = stream::iter(todo)
        .map(|folder| {
            let client = client.clone();
            async move {
                let result = list_series(&client, &folder).await;
                (folder, result)
            }
        })
        .buffer_unordered(PARALLEL_LISTINGS);
    let mut done = 0;
    while let Some((folder, result)) = listings.next().await {
        done += 1;
        match result {
            Ok(contents) => {
                let id = series_id(source_id, &folder.path);
                let stale = db
                    .apply_remote_volumes(&id, folder.signature.as_deref(), contents.cover.as_deref(), &records(source_id, &contents))
                    .map_err(db_error)?;
                outcome.stale.extend(stale);
                outcome.listed += 1;
            }
            // Gone between the two listings, or not ours to read: next time.
            Err(DavError::NotFound | DavError::Unauthorized | DavError::NotWebdav) => {}
            Err(error) => return Err(error),
        }
        progress(done, total);
    }
    Ok(outcome)
}

/// Syncs a source for the app, one sync per source at a time, telling the
/// UI how it goes. Without `force` a source synced a moment ago is left alone
/// (`automatic`) or only its changed series are listed (the user's button).
pub async fn sync_source(app: &AppHandle, source_id: &str, force: bool, automatic: bool) -> Result<(), String> {
    let state = app.state::<AppState>();
    {
        let last = state.manga.webdav_last_sync.lock().unwrap();
        if automatic && last.get(source_id).is_some_and(|at| at.elapsed() < AUTO_SYNC_EVERY) {
            return Ok(());
        }
    }
    if !state.manga.webdav_syncing.lock().unwrap().insert(source_id.to_string()) {
        return Ok(());
    }
    let emit = |done: usize, total: usize, running: bool, error: Option<String>| {
        let _ = app.emit(SYNC_EVENT, WebdavSyncProgress { source_id: source_id.to_string(), done, total, running, error });
    };
    emit(0, 0, true, None);
    let db = state.db.clone();
    let last_announced = std::sync::Mutex::new(Instant::now());
    let result = sync_with(&db, source_id, force, |done, total| {
        emit(done, total, true, None);
        // The library fills in as it goes, not only at the end.
        let mut last = last_announced.lock().unwrap();
        if last.elapsed() > Duration::from_secs(2) {
            *last = Instant::now();
            let _ = app.emit(LIBRARY_CHANGED, ());
        }
    })
    .await;
    state.manga.webdav_syncing.lock().unwrap().remove(source_id);
    state.manga.webdav_last_sync.lock().unwrap().insert(source_id.to_string(), Instant::now());
    let outcome = match result {
        Ok(outcome) => {
            let _ = db.set_manga_source_status(source_id, None, true);
            if let Ok(cache_dir) = app.path().app_cache_dir() {
                crate::manga::thumbs::purge(&cache_dir, &outcome.stale);
            }
            emit(outcome.listed, outcome.listed, false, None);
            Ok(())
        }
        Err(error) => {
            log::info!("WebDAV sync of {source_id} failed: {error}");
            let _ = db.set_manga_source_status(source_id, Some(error.code()), false);
            emit(0, 0, false, Some(error.code().to_string()));
            Err(error.code().to_string())
        }
    };
    let _ = app.emit(LIBRARY_CHANGED, ());
    outcome
}

/// Every WebDAV source, in the background.
pub fn sync_all_later(app: &AppHandle, force: bool, automatic: bool) {
    let Ok(sources) = app.state::<AppState>().db.manga_sources() else { return };
    for source in sources.into_iter().filter(|source| source.kind == SOURCE_WEBDAV) {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let _ = sync_source(&app, &source.id, force, automatic).await;
        });
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::manga::store::tests::temp_db;
    use crate::manga::webdav::test_server::{Reply, TestServer};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn response(href: &str, dir: bool, size: u64, etag: &str) -> String {
        let kind = if dir { "<D:resourcetype><D:collection/></D:resourcetype>".to_string() } else { format!("<D:resourcetype/><D:getcontentlength>{size}</D:getcontentlength>") };
        format!("<D:response><D:href>{href}</D:href><D:propstat><D:prop>{kind}<D:getetag>{etag}</D:getetag></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>")
    }

    pub fn multistatus(responses: &[String]) -> Vec<u8> {
        format!("<?xml version=\"1.0\"?><D:multistatus xmlns:D=\"DAV:\">{}</D:multistatus>", responses.concat()).into_bytes()
    }

    /// A bunko-shaped server: `/mokuro-reader/` with two series, one of them
    /// changing its ETag when `bump` is set.
    pub fn bunko(listings: Arc<AtomicUsize>, etag: Arc<std::sync::Mutex<String>>) -> TestServer {
        TestServer::start(move |request| {
            if request.method != "PROPFIND" {
                return Reply::status(404);
            }
            listings.fetch_add(1, Ordering::SeqCst);
            let etag = etag.lock().unwrap().clone();
            let body = match request.path.as_str() {
                "/" => multistatus(&[response("/", true, 0, "r"), response("/mokuro-reader/", true, 0, "m")]),
                "/mokuro-reader/" => multistatus(&[
                    response("/mokuro-reader/", true, 0, "m"),
                    response("/mokuro-reader/Hoshi/", true, 0, &etag),
                    response("/mokuro-reader/Umi%20no%20Atorie/", true, 0, "u1"),
                ]),
                "/mokuro-reader/Hoshi/" => multistatus(&[
                    response("/mokuro-reader/Hoshi/", true, 0, &etag),
                    response("/mokuro-reader/Hoshi/%E6%98%9F%201.cbz", false, 600, &format!("c-{etag}")),
                    response("/mokuro-reader/Hoshi/%E6%98%9F%201.mokuro", false, 5, "t"),
                    response("/mokuro-reader/Hoshi/%E6%98%9F%201.edit.mokuro", false, 5, "t"),
                    response("/mokuro-reader/Hoshi/%E6%98%9F%201.webp", false, 2, "w"),
                ]),
                "/mokuro-reader/Umi%20no%20Atorie/" => multistatus(&[
                    response("/mokuro-reader/Umi%20no%20Atorie/Vol%201/", true, 0, "f"),
                ]),
                "/mokuro-reader/Umi%20no%20Atorie/Vol%201/" => multistatus(&[
                    response("/mokuro-reader/Umi%20no%20Atorie/Vol%201/002.jpg", false, 10, "p2"),
                    response("/mokuro-reader/Umi%20no%20Atorie/Vol%201/001.jpg", false, 10, "p1"),
                ]),
                _ => return Reply::status(404),
            };
            Reply::new(207, body)
        })
    }

    #[test]
    fn a_bunko_server_becomes_series_and_volumes_and_only_changes_are_listed_again() {
        let listings = Arc::new(AtomicUsize::new(0));
        let etag = Arc::new(std::sync::Mutex::new("h1".to_string()));
        let server = bunko(listings.clone(), etag.clone());
        let (db, path) = temp_db();
        db.add_manga_source("dav", SOURCE_WEBDAV, "Bunko", &server.url("/")).unwrap();

        let probe_result = tauri::async_runtime::block_on(probe(&client_for(&db.manga_source_row("dav").unwrap().unwrap()).unwrap())).unwrap();
        assert_eq!(probe_result, WebdavProbe { series: 2, library_path: "/mokuro-reader/".into(), bunko: true });

        let outcome = tauri::async_runtime::block_on(sync_with(&db, "dav", false, |_, _| {})).unwrap();
        assert_eq!((outcome.series, outcome.listed), (2, 2));
        let series = db.manga_series_list().unwrap();
        assert_eq!(series.iter().map(|s| (s.title.as_str(), s.source.as_str(), s.volume_count)).collect::<Vec<_>>(), [("Hoshi", "webdav", 1), ("Umi no Atorie", "webdav", 1)]);
        let hoshi = db.manga_volumes(&series[0].id).unwrap();
        assert_eq!((hoshi[0].title.as_str(), hoshi[0].format.as_str(), hoshi[0].page_count, hoshi[0].ocr_state.as_str()), ("星 1", "remote-archive", 0, "mokuro"));
        assert_eq!(hoshi[0].text_layers, ["edit"]);
        assert_eq!(hoshi[0].size, Some(600));
        let umi = db.manga_volumes(&series[1].id).unwrap();
        assert_eq!((umi[0].format.as_str(), umi[0].page_count), ("remote-folder", 2));
        let location = db.manga_volume_location(&umi[0].id).unwrap().unwrap();
        assert_eq!(location.pages, ["001.jpg", "002.jpg"]);
        assert_eq!(location.source_ref, "/mokuro-reader/Umi no Atorie/Vol 1/");

        // The index read later survives a sync where nothing changed…
        db.set_manga_volume_pages(&hoshi[0].id, &["1.jpg".into(), "2.jpg".into()]).unwrap();
        let before = listings.load(Ordering::SeqCst);
        let outcome = tauri::async_runtime::block_on(sync_with(&db, "dav", false, |_, _| {})).unwrap();
        // Only the root and the library: no series changed.
        assert_eq!((outcome.listed, listings.load(Ordering::SeqCst) - before), (0, 2));
        assert_eq!(db.manga_volumes(&series[0].id).unwrap()[0].page_count, 2);

        // …and goes when the archive changes on the server.
        *etag.lock().unwrap() = "h2".into();
        let outcome = tauri::async_runtime::block_on(sync_with(&db, "dav", false, |_, _| {})).unwrap();
        assert_eq!(outcome.listed, 1);
        assert_eq!(outcome.stale, [hoshi[0].id.clone()]);
        assert_eq!(db.manga_volumes(&series[0].id).unwrap()[0].page_count, 0);
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn an_unreachable_server_keeps_what_was_seen() {
        let listings = Arc::new(AtomicUsize::new(0));
        let server = bunko(listings, Arc::new(std::sync::Mutex::new("h".into())));
        let (db, path) = temp_db();
        db.add_manga_source("dav", SOURCE_WEBDAV, "Bunko", &server.url("/")).unwrap();
        tauri::async_runtime::block_on(sync_with(&db, "dav", false, |_, _| {})).unwrap();
        drop(server);
        std::thread::sleep(Duration::from_millis(50));
        let error = tauri::async_runtime::block_on(sync_with(&db, "dav", true, |_, _| {})).unwrap_err();
        assert_eq!(error, DavError::Offline);
        assert_eq!(db.manga_series_list().unwrap().len(), 2);
        drop(db);
        let _ = std::fs::remove_file(path);
    }
}
