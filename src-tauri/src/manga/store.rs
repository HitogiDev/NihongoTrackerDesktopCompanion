//! The manga tables: sources, series, volumes, reading progress and the text
//! of each page.
//!
//! Ids are opaque to the UI. Series and volumes found on disk get ids derived
//! from where they are (see `library::stable_id`), so a volume that disappears
//! for a while (an unplugged drive) and comes back keeps its progress, which is
//! why `manga_progress` has no foreign key.
//!
//! A source's credentials (`auth_json`, for WebDAV) stay in Rust: no struct
//! sent to the UI carries them, only what kind they are (`MangaSource::auth`),
//! like the account's API key never leaves `Settings` for anything but the
//! settings screen.

use std::collections::{HashMap, HashSet};

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::{now_iso, Db, Result};

use super::natural_cmp;

/// `manga_sources.kind` for a folder on this computer.
pub const SOURCE_LOCAL: &str = "local";
/// `manga_sources.kind` for a WebDAV server (see `manga::webdav`).
pub const SOURCE_WEBDAV: &str = "webdav";

/// `manga_volumes.format`.
pub const FORMAT_FOLDER: &str = "folder";
pub const FORMAT_ARCHIVE: &str = "archive";
/// On a WebDAV server: `source_ref` is the remote path, not a file.
pub const FORMAT_REMOTE_ARCHIVE: &str = "remote-archive";
pub const FORMAT_REMOTE_FOLDER: &str = "remote-folder";

pub fn is_remote_format(format: &str) -> bool {
    format == FORMAT_REMOTE_ARCHIVE || format == FORMAT_REMOTE_FOLDER
}

/// A chapter of a series from an extension (`manga::suwayomi`): `source_ref`
/// is Suwayomi's id of the chapter, `pages_json` its pages' server paths.
pub const FORMAT_SUWAYOMI_CHAPTER: &str = "suwayomi-chapter";

/// Pages that come from a server through `webdav::remote::Remote` (WebDAV or
/// Suwayomi), not from a file on this computer.
pub fn is_served_format(format: &str) -> bool {
    is_remote_format(format) || format == FORMAT_SUWAYOMI_CHAPTER
}

// `manga_volumes.ocr_state` is `none` (no text), `mokuro` (a `.mokuro` file
// next to the volume, set by `apply_manga_scan`) or `ocr` (no `.mokuro`, but
// pages read by the built-in OCR or owocr, or written by hand: `manga_page_text`).

/// `manga_page_text.engine` of the built-in OCR.
pub const ENGINE_BUILTIN: &str = "builtin";
/// `manga_page_text.engine` of a page read by owocr (`manga::ocr::owocr`).
pub const ENGINE_OWOCR: &str = "owocr";
/// `manga_page_text.engine` of a page read by the built-in meikiocr
/// (`manga::ocr::meiki`).
pub const ENGINE_MEIKI: &str = "meiki";
/// `manga_page_text.engine` of a page the user corrected: it wins over the
/// `.mokuro` too.
pub const ENGINE_MANUAL: &str = "manual";

/// A library root as the UI lists it. Never with its credentials.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct MangaSource {
    pub id: String,
    /// `local`, `webdav` or `suwayomi`.
    pub kind: String,
    pub name: String,
    /// A folder for `local`, a URL (never with credentials) for the others.
    pub url: String,
    pub created_at: String,
    pub series_count: i64,
    /// `none`, `basic` or `bearer`: what kind of credentials it has, never
    /// the credentials.
    pub auth: String,
    /// The last time its server was listed in full.
    pub last_sync_at: Option<String>,
    /// Why the last request to its server failed (`offline`, `unauthorized`…),
    /// until one succeeds.
    pub last_error: Option<String>,
    /// A sync is running now (filled in by the command, not stored).
    pub syncing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct MangaSeries {
    pub id: String,
    /// The kind of its source (`local`…), for the origin tabs.
    pub source: String,
    pub title: String,
    /// Where it lives, for display only: the protocol never takes it back.
    pub location: String,
    /// The NihongoTracker media it logs to, if linked.
    pub media_id: Option<String>,
    pub media_title: Option<String>,
    pub is_adult: bool,
    pub created_at: String,
    pub volume_count: i64,
    pub page_count: i64,
    /// Some volume has text (a `.mokuro`, or OCR later).
    pub has_text: bool,
    pub volumes_finished: i64,
    /// Pages up to where the user is in each volume, finished ones in full.
    pub pages_read: i64,
    pub last_read_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct MangaVolume {
    pub id: String,
    pub series_id: String,
    pub title: String,
    /// `folder` or `archive`, `remote-folder` or `remote-archive`.
    pub format: String,
    pub position: i64,
    pub page_count: i64,
    /// `none`, or where its text comes from (`mokuro`).
    pub ocr_state: String,
    pub added_at: String,
    /// The page the user is on, if they ever opened it (0-based).
    pub page: Option<i64>,
    pub finished: bool,
    pub read_at: Option<String>,
    /// Bytes on the server, for a remote volume.
    pub size: Option<i64>,
    /// Alternative texts (`<Vol>.<layer>.mokuro`), by layer name.
    pub text_layers: Vec<String>,
    /// The layer the reader shows; `None` is the main `.mokuro`.
    pub text_layer: Option<String>,
    /// A remote volume downloaded for offline reading.
    pub downloaded: bool,
    /// Pages with text of their own (OCR or corrected by hand) instead of, or
    /// besides, a `.mokuro`.
    pub ocr_pages: i64,
}

/// One cover on the "continue reading" shelf.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct MangaContinue {
    pub series_id: String,
    pub series_title: String,
    pub volume_id: String,
    pub volume_title: String,
    pub page: i64,
    pub page_count: i64,
    /// The previous volume was finished and this one is next, not started.
    pub up_next: bool,
    pub updated_at: String,
}

/// The Manga screen in one call.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct MangaLibrary {
    pub sources: Vec<MangaSource>,
    pub series: Vec<MangaSeries>,
    pub continue_reading: Vec<MangaContinue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct MangaSeriesDetail {
    pub series: MangaSeries,
    pub volumes: Vec<MangaVolume>,
    /// Where it comes from, for a series on a server (offline or not).
    pub source: Option<MangaSource>,
}

/// A series as a scan found it, with all of its volumes.
#[derive(Debug, Clone, PartialEq)]
pub struct SeriesRecord {
    pub id: String,
    pub source_ref: String,
    pub title: String,
    /// An image that is the series' cover, if the folder has one.
    pub cover: Option<String>,
    pub volumes: Vec<VolumeRecord>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VolumeRecord {
    pub id: String,
    pub title: String,
    pub source_ref: String,
    pub format: &'static str,
    pub position: i64,
    pub pages: Vec<String>,
    /// Size and modification time: when it changes, cached thumbnails go.
    pub signature: String,
    pub mokuro_path: Option<String>,
}

/// A volume as a WebDAV listing found it (see `webdav::sync`).
#[derive(Debug, Clone, PartialEq)]
pub struct RemoteVolumeRecord {
    pub id: String,
    pub title: String,
    /// Remote path of the archive or folder.
    pub source_ref: String,
    pub format: &'static str,
    pub position: i64,
    /// A folder's pages; empty for an archive until its index is read.
    pub pages: Vec<String>,
    pub signature: Option<String>,
    pub mokuro_path: Option<String>,
    pub layers: Vec<(String, String)>,
    pub cover: Option<String>,
    pub size: Option<i64>,
}

/// A source as Rust needs it, with its credentials. Never for the UI.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceRow {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub url: String,
    pub auth_json: Option<String>,
}

/// What it takes to serve a volume's pages. Rust only.
#[derive(Debug, Clone, PartialEq)]
pub struct VolumeLocation {
    pub id: String,
    pub series_id: String,
    pub title: String,
    pub source_ref: String,
    pub format: String,
    pub pages: Vec<String>,
    pub signature: Option<String>,
    pub mokuro_path: Option<String>,
    pub source_id: String,
    /// A remote volume's own cover image (its sidecar), as a remote path.
    pub cover: Option<String>,
    /// `(layer, remote path)` of its alternative texts.
    pub layers: Vec<(String, String)>,
    pub size: Option<i64>,
}

/// What a scan changed, for the caches that hang off volume ids.
#[derive(Debug, Default, PartialEq)]
pub struct ScanChanges {
    /// Volumes whose files changed or that are gone: their thumbnails are stale.
    pub stale_volumes: Vec<String>,
    pub series: usize,
    pub volumes: usize,
}

fn map_volume_location(r: &rusqlite::Row<'_>) -> Result<VolumeLocation> {
    let pages: String = r.get(5)?;
    let layers: Option<String> = r.get(10)?;
    Ok(VolumeLocation {
        id: r.get(0)?,
        series_id: r.get(1)?,
        title: r.get(2)?,
        source_ref: r.get(3)?,
        format: r.get(4)?,
        pages: serde_json::from_str(&pages).unwrap_or_default(),
        signature: r.get(6)?,
        mokuro_path: r.get(7)?,
        source_id: r.get(8)?,
        cover: r.get(9)?,
        layers: layers.and_then(|json| serde_json::from_str(&json).ok()).unwrap_or_default(),
        size: r.get(11)?,
    })
}

const VOLUME_LOCATION: &str = "SELECT v.id, v.series_id, v.title, v.source_ref, v.format, v.pages_json, v.signature, v.mokuro_path,
            s.source_id, v.cover, v.layers_json, v.size
     FROM manga_volumes v JOIN manga_series s ON s.id = v.series_id";

fn layer_key(volume_id: &str) -> String {
    format!("manga:layer:{volume_id}")
}

impl Db {
    // ── sources ────────────────────────────────────────────────────────

    pub fn add_manga_source(&self, id: &str, kind: &str, name: &str, url: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO manga_sources (id, kind, name, url, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, kind, name, url, now_iso()],
            )
            .map(|_| ())
        })
    }

    pub fn manga_sources(&self) -> Result<Vec<MangaSource>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT m.id, m.kind, m.name, m.url, m.created_at,
                        (SELECT COUNT(*) FROM manga_series s WHERE s.source_id = m.id),
                        m.auth_json, m.last_sync_at, m.last_error
                 FROM manga_sources m ORDER BY m.created_at",
            )?;
            let rows = stmt.query_map([], |r| {
                // Only the kind of the credentials is read out of them.
                let auth: Option<String> = r.get(6)?;
                Ok(MangaSource {
                    id: r.get(0)?,
                    kind: r.get(1)?,
                    name: r.get(2)?,
                    url: r.get(3)?,
                    created_at: r.get(4)?,
                    series_count: r.get(5)?,
                    auth: crate::manga::webdav::client::Auth::from_json(auth.as_deref()).kind().to_string(),
                    last_sync_at: r.get(7)?,
                    last_error: r.get(8)?,
                    syncing: false,
                })
            })?;
            rows.collect()
        })
    }

    /// The source's credentials, for Rust's own requests. Never for the UI.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn manga_source_auth(&self, id: &str) -> Result<Option<String>> {
        self.with(|c| {
            c.query_row("SELECT auth_json FROM manga_sources WHERE id = ?1", params![id], |r| r.get(0))
                .optional()
                .map(Option::flatten)
        })
    }

    /// A source with its credentials, for Rust's own requests.
    pub fn manga_source_row(&self, id: &str) -> Result<Option<SourceRow>> {
        self.with(|c| {
            c.query_row("SELECT id, kind, name, url, auth_json FROM manga_sources WHERE id = ?1", params![id], |r| {
                Ok(SourceRow { id: r.get(0)?, kind: r.get(1)?, name: r.get(2)?, url: r.get(3)?, auth_json: r.get(4)? })
            })
            .optional()
        })
    }

    pub fn set_manga_source_auth(&self, id: &str, auth_json: Option<&str>) -> Result<()> {
        self.with(|c| c.execute("UPDATE manga_sources SET auth_json = ?2 WHERE id = ?1", params![id, auth_json]).map(|_| ()))
    }

    pub fn update_manga_source(&self, id: &str, name: &str, url: &str) -> Result<bool> {
        self.with(|c| {
            c.execute("UPDATE manga_sources SET name = ?2, url = ?3 WHERE id = ?1", params![id, name, url]).map(|n| n > 0)
        })
    }

    /// The outcome of the last request to a source's server. A success
    /// (`error: None`) with `synced` also stamps a full listing.
    pub fn set_manga_source_status(&self, id: &str, error: Option<&str>, synced: bool) -> Result<bool> {
        self.with(|c| {
            let before: Option<Option<String>> = c
                .query_row("SELECT last_error FROM manga_sources WHERE id = ?1", params![id], |r| r.get(0))
                .optional()?;
            if synced {
                c.execute("UPDATE manga_sources SET last_error = ?2, last_sync_at = ?3 WHERE id = ?1", params![id, error, now_iso()])?;
            } else {
                c.execute("UPDATE manga_sources SET last_error = ?2 WHERE id = ?1", params![id, error])?;
            }
            // Whether the status changed: the library is told only then.
            Ok(before.is_some_and(|before| before.as_deref() != error))
        })
    }

    /// Forgets a source and its series (the files stay where they are). The
    /// ids of its volumes are returned so their cached thumbnails can go too.
    pub fn delete_manga_source(&self, id: &str) -> Result<Vec<String>> {
        self.with(|c| {
            let tx = c.unchecked_transaction()?;
            let volumes = {
                let mut stmt = tx.prepare(
                    "SELECT v.id FROM manga_volumes v JOIN manga_series s ON s.id = v.series_id WHERE s.source_id = ?1",
                )?;
                let rows = stmt.query_map(params![id], |r| r.get::<_, String>(0))?;
                rows.collect::<Result<Vec<_>>>()?
            };
            tx.execute("DELETE FROM manga_series WHERE source_id = ?1", params![id])?;
            tx.execute("DELETE FROM manga_sources WHERE id = ?1", params![id])?;
            tx.commit()?;
            Ok(volumes)
        })
    }

    // ── scans ──────────────────────────────────────────────────────────

    /// Makes the source's series and volumes exactly what a scan found:
    /// new ones are added, changed ones updated (their links to NihongoTracker
    /// kept) and missing ones removed. Reading progress is never touched.
    pub fn apply_manga_scan(&self, source_id: &str, kind: &str, found: &[SeriesRecord]) -> Result<ScanChanges> {
        self.with(|c| {
            let tx = c.unchecked_transaction()?;
            let before: HashMap<String, Option<String>> = {
                let mut stmt = tx.prepare(
                    "SELECT v.id, v.signature FROM manga_volumes v JOIN manga_series s ON s.id = v.series_id WHERE s.source_id = ?1",
                )?;
                let rows = stmt.query_map(params![source_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
                rows.collect::<Result<_>>()?
            };
            let now = now_iso();
            let mut changes = ScanChanges::default();
            let mut series_ids = HashSet::new();
            let mut volume_ids = HashSet::new();
            for series in found {
                tx.execute(
                    "INSERT INTO manga_series (id, source, source_id, source_ref, title, cover, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                     ON CONFLICT(id) DO UPDATE SET
                        source = excluded.source,
                        source_id = excluded.source_id,
                        source_ref = excluded.source_ref,
                        title = excluded.title,
                        cover = excluded.cover",
                    params![series.id, kind, source_id, series.source_ref, series.title, series.cover, now],
                )?;
                series_ids.insert(series.id.as_str());
                for volume in &series.volumes {
                    let pages = serde_json::to_string(&volume.pages).unwrap_or_else(|_| "[]".into());
                    tx.execute(
                        "INSERT INTO manga_volumes (id, series_id, title, source_ref, format, position, page_count, pages_json, signature, mokuro_path, ocr_state, added_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, CASE WHEN ?10 IS NULL THEN 'none' ELSE 'mokuro' END, ?11)
                         ON CONFLICT(id) DO UPDATE SET
                            series_id = excluded.series_id,
                            title = excluded.title,
                            source_ref = excluded.source_ref,
                            format = excluded.format,
                            position = excluded.position,
                            page_count = excluded.page_count,
                            pages_json = excluded.pages_json,
                            signature = excluded.signature,
                            mokuro_path = excluded.mokuro_path,
                            ocr_state = CASE
                                WHEN excluded.mokuro_path IS NOT NULL THEN 'mokuro'
                                WHEN manga_volumes.ocr_state = 'mokuro' THEN 'none'
                                ELSE manga_volumes.ocr_state END",
                        params![
                            volume.id,
                            series.id,
                            volume.title,
                            volume.source_ref,
                            volume.format,
                            volume.position,
                            volume.pages.len() as i64,
                            pages,
                            volume.signature,
                            volume.mokuro_path,
                            now
                        ],
                    )?;
                    volume_ids.insert(volume.id.as_str());
                    if before.get(&volume.id).is_some_and(|old| old.as_deref() != Some(volume.signature.as_str())) {
                        changes.stale_volumes.push(volume.id.clone());
                    }
                }
            }
            for id in before.keys().filter(|id| !volume_ids.contains(id.as_str())) {
                tx.execute("DELETE FROM manga_volumes WHERE id = ?1", params![id])?;
                changes.stale_volumes.push(id.clone());
            }
            let gone: Vec<String> = {
                let mut stmt = tx.prepare("SELECT id FROM manga_series WHERE source_id = ?1")?;
                let rows = stmt.query_map(params![source_id], |r| r.get::<_, String>(0))?;
                rows.filter_map(|row| row.ok()).filter(|id| !series_ids.contains(id.as_str())).collect()
            };
            for id in gone {
                tx.execute("DELETE FROM manga_series WHERE id = ?1", params![id])?;
            }
            tx.commit()?;
            changes.series = series_ids.len();
            changes.volumes = volume_ids.len();
            Ok(changes)
        })
    }

    /// `volume id → (signature, pages)` for a source's volumes, so a rescan
    /// does not reopen archives that did not change.
    pub fn manga_known_volumes(&self, source_id: &str) -> Result<HashMap<String, (String, Vec<String>)>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT v.id, v.signature, v.pages_json FROM manga_volumes v
                 JOIN manga_series s ON s.id = v.series_id
                 WHERE s.source_id = ?1 AND v.signature IS NOT NULL",
            )?;
            let rows = stmt.query_map(params![source_id], |r| {
                let pages: String = r.get(2)?;
                Ok((r.get::<_, String>(0)?, (r.get::<_, String>(1)?, serde_json::from_str(&pages).unwrap_or_default())))
            })?;
            rows.collect()
        })
    }

    // ── remote sources ─────────────────────────────────────────────────

    /// Makes a WebDAV source's series exactly the folders a listing found
    /// (`(id, remote path, title)`): new ones added, gone ones removed with
    /// their volumes, whose ids are returned. A series' volumes, cover and
    /// signature are left as they are: they come from its own listing.
    pub fn apply_remote_series(&self, source_id: &str, found: &[(String, String, String)]) -> Result<Vec<String>> {
        self.with(|c| {
            let tx = c.unchecked_transaction()?;
            let now = now_iso();
            let ids: HashSet<&str> = found.iter().map(|(id, ..)| id.as_str()).collect();
            for (id, path, title) in found {
                tx.execute(
                    "INSERT INTO manga_series (id, source, source_id, source_ref, title, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT(id) DO UPDATE SET source_ref = excluded.source_ref, title = excluded.title",
                    params![id, SOURCE_WEBDAV, source_id, path, title, now],
                )?;
            }
            let gone: Vec<String> = {
                let mut stmt = tx.prepare("SELECT id FROM manga_series WHERE source_id = ?1")?;
                let rows = stmt.query_map(params![source_id], |r| r.get::<_, String>(0))?;
                rows.filter_map(|row| row.ok()).filter(|id| !ids.contains(id.as_str())).collect()
            };
            let mut stale = Vec::new();
            for id in gone {
                let mut stmt = tx.prepare("SELECT id FROM manga_volumes WHERE series_id = ?1")?;
                let volumes = stmt.query_map(params![id], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>>>()?;
                stale.extend(volumes);
                tx.execute("DELETE FROM manga_series WHERE id = ?1", params![id])?;
            }
            tx.commit()?;
            Ok(stale)
        })
    }

    /// `series id → signature of its folder when it was last listed`.
    pub fn remote_series_signatures(&self, source_id: &str) -> Result<HashMap<String, Option<String>>> {
        self.with(|c| {
            let mut stmt = c.prepare("SELECT id, signature FROM manga_series WHERE source_id = ?1")?;
            let rows = stmt.query_map(params![source_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect()
        })
    }

    /// Makes a remote series' volumes what its listing found. An archive whose
    /// signature did not change keeps the page list read from it; one that
    /// changed (or is gone) is returned, so its caches go.
    pub fn apply_remote_volumes(
        &self,
        series_id: &str,
        signature: Option<&str>,
        cover: Option<&str>,
        volumes: &[RemoteVolumeRecord],
    ) -> Result<Vec<String>> {
        self.with(|c| {
            let tx = c.unchecked_transaction()?;
            let before: HashMap<String, Option<String>> = {
                let mut stmt = tx.prepare("SELECT id, signature FROM manga_volumes WHERE series_id = ?1")?;
                let rows = stmt.query_map(params![series_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
                rows.collect::<Result<_>>()?
            };
            let now = now_iso();
            let mut stale = Vec::new();
            for volume in volumes {
                let changed = before.get(&volume.id).is_some_and(|old| old != &volume.signature);
                if changed {
                    stale.push(volume.id.clone());
                }
                let keep_pages = before.contains_key(&volume.id) && !changed && volume.pages.is_empty();
                let pages = serde_json::to_string(&volume.pages).unwrap_or_else(|_| "[]".into());
                let layers = serde_json::to_string(&volume.layers).unwrap_or_else(|_| "[]".into());
                tx.execute(
                    "INSERT INTO manga_volumes (id, series_id, title, source_ref, format, position, page_count, pages_json, signature, mokuro_path, ocr_state, added_at, cover, layers_json, size)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, CASE WHEN ?10 IS NULL THEN 'none' ELSE 'mokuro' END, ?11, ?12, ?13, ?14)
                     ON CONFLICT(id) DO UPDATE SET
                        series_id = excluded.series_id,
                        title = excluded.title,
                        source_ref = excluded.source_ref,
                        format = excluded.format,
                        position = excluded.position,
                        page_count = CASE WHEN ?15 THEN manga_volumes.page_count ELSE excluded.page_count END,
                        pages_json = CASE WHEN ?15 THEN manga_volumes.pages_json ELSE excluded.pages_json END,
                        signature = excluded.signature,
                        mokuro_path = excluded.mokuro_path,
                        ocr_state = CASE
                            WHEN excluded.mokuro_path IS NOT NULL THEN 'mokuro'
                            WHEN manga_volumes.ocr_state = 'mokuro' THEN 'none'
                            ELSE manga_volumes.ocr_state END,
                        cover = excluded.cover,
                        layers_json = excluded.layers_json,
                        size = excluded.size",
                    params![
                        volume.id,
                        series_id,
                        volume.title,
                        volume.source_ref,
                        volume.format,
                        volume.position,
                        volume.pages.len() as i64,
                        pages,
                        volume.signature,
                        volume.mokuro_path,
                        now,
                        volume.cover,
                        layers,
                        volume.size,
                        keep_pages,
                    ],
                )?;
            }
            let ids: HashSet<&str> = volumes.iter().map(|volume| volume.id.as_str()).collect();
            for id in before.keys().filter(|id| !ids.contains(id.as_str())) {
                tx.execute("DELETE FROM manga_volumes WHERE id = ?1", params![id])?;
                stale.push(id.clone());
            }
            tx.execute("UPDATE manga_series SET signature = ?2, cover = ?3 WHERE id = ?1", params![series_id, signature, cover])?;
            tx.commit()?;
            Ok(stale)
        })
    }

    pub fn set_manga_offline(&self, volume_id: &str, bytes: i64) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO manga_offline (volume_id, bytes, downloaded_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(volume_id) DO UPDATE SET bytes = excluded.bytes, downloaded_at = excluded.downloaded_at",
                params![volume_id, bytes, now_iso()],
            )
            .map(|_| ())
        })
    }

    pub fn delete_manga_offline(&self, volume_id: &str) -> Result<()> {
        self.with(|c| c.execute("DELETE FROM manga_offline WHERE volume_id = ?1", params![volume_id]).map(|_| ()))
    }

    /// `(volumes, bytes)` downloaded for offline reading.
    pub fn manga_offline_totals(&self) -> Result<(i64, i64)> {
        self.with(|c| c.query_row("SELECT COUNT(*), COALESCE(SUM(bytes), 0) FROM manga_offline", [], |r| Ok((r.get(0)?, r.get(1)?))))
    }

    /// The pages of a remote archive, once its index has been read.
    pub fn set_manga_volume_pages(&self, volume_id: &str, pages: &[String]) -> Result<()> {
        let json = serde_json::to_string(pages).unwrap_or_else(|_| "[]".into());
        self.with(|c| {
            c.execute(
                "UPDATE manga_volumes SET pages_json = ?2, page_count = ?3 WHERE id = ?1",
                params![volume_id, json, pages.len() as i64],
            )
            .map(|_| ())
        })
    }

    // ── library ────────────────────────────────────────────────────────

    fn query_manga_series(&self, filter: Option<&str>) -> Result<Vec<MangaSeries>> {
        self.with(|c| {
            let sql = format!(
                "SELECT s.id, s.source, s.title, s.source_ref, s.media_id, s.media_title, s.is_adult, s.created_at,
                        COUNT(v.id),
                        COALESCE(SUM(v.page_count), 0),
                        COALESCE(MAX(v.ocr_state != 'none'), 0),
                        COALESCE(SUM(p.finished = 1), 0),
                        COALESCE(SUM(CASE WHEN p.finished = 1 THEN v.page_count
                                          WHEN p.page IS NULL THEN 0
                                          ELSE MIN(p.page + 1, v.page_count) END), 0),
                        MAX(p.updated_at)
                 FROM manga_series s
                 LEFT JOIN manga_volumes v ON v.series_id = s.id
                 LEFT JOIN manga_progress p ON p.volume_id = v.id
                 {}
                 GROUP BY s.id",
                if filter.is_some() { "WHERE s.id = ?1" } else { "" }
            );
            let mut stmt = c.prepare(&sql)?;
            let map = |r: &rusqlite::Row<'_>| {
                Ok(MangaSeries {
                    id: r.get(0)?,
                    source: r.get(1)?,
                    title: r.get(2)?,
                    location: r.get(3)?,
                    media_id: r.get(4)?,
                    media_title: r.get(5)?,
                    is_adult: r.get::<_, i64>(6)? != 0,
                    created_at: r.get(7)?,
                    volume_count: r.get(8)?,
                    page_count: r.get(9)?,
                    has_text: r.get::<_, i64>(10)? != 0,
                    volumes_finished: r.get(11)?,
                    pages_read: r.get(12)?,
                    last_read_at: r.get(13)?,
                })
            };
            let rows = match filter {
                Some(id) => stmt.query_map(params![id], map)?.collect::<Result<Vec<_>>>()?,
                None => stmt.query_map([], map)?.collect::<Result<Vec<_>>>()?,
            };
            Ok(rows)
        })
    }

    /// The id of every series in the library.
    pub fn manga_series_ids(&self) -> Result<HashSet<String>> {
        self.with(|c| {
            let mut stmt = c.prepare("SELECT id FROM manga_series")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            rows.collect()
        })
    }

    /// Every series, in natural title order.
    pub fn manga_series_list(&self) -> Result<Vec<MangaSeries>> {
        let mut list = self.query_manga_series(None)?;
        list.sort_by(|a, b| natural_cmp(&a.title, &b.title));
        Ok(list)
    }

    pub fn manga_series(&self, id: &str) -> Result<Option<MangaSeries>> {
        Ok(self.query_manga_series(Some(id))?.into_iter().next())
    }

    pub fn manga_volumes(&self, series_id: &str) -> Result<Vec<MangaVolume>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT v.id, v.series_id, v.title, v.format, v.position, v.page_count, v.ocr_state, v.added_at,
                        p.page, COALESCE(p.finished, 0), p.updated_at, v.size, v.layers_json,
                        EXISTS(SELECT 1 FROM manga_offline o WHERE o.volume_id = v.id),
                        (SELECT COUNT(*) FROM manga_page_text t WHERE t.volume_id = v.id)
                 FROM manga_volumes v LEFT JOIN manga_progress p ON p.volume_id = v.id
                 WHERE v.series_id = ?1 ORDER BY v.position, v.title",
            )?;
            let rows = stmt.query_map(params![series_id], |r| {
                let layers: Option<String> = r.get(12)?;
                let layers: Vec<(String, String)> = layers.and_then(|json| serde_json::from_str(&json).ok()).unwrap_or_default();
                Ok(MangaVolume {
                    id: r.get(0)?,
                    series_id: r.get(1)?,
                    title: r.get(2)?,
                    format: r.get(3)?,
                    position: r.get(4)?,
                    page_count: r.get(5)?,
                    ocr_state: r.get(6)?,
                    added_at: r.get(7)?,
                    page: r.get(8)?,
                    finished: r.get::<_, i64>(9)? != 0,
                    read_at: r.get(10)?,
                    size: r.get(11)?,
                    text_layers: layers.into_iter().map(|(name, _)| name).collect(),
                    text_layer: None,
                    downloaded: r.get::<_, i64>(13)? != 0,
                    ocr_pages: r.get(14)?,
                })
            })?;
            rows.collect::<Result<Vec<_>>>()
        })
        .map(|mut volumes| {
            for volume in volumes.iter_mut().filter(|volume| !volume.text_layers.is_empty()) {
                volume.text_layer = self.get_kv(&layer_key(&volume.id)).filter(|layer| volume.text_layers.contains(layer));
            }
            volumes
        })
    }

    /// The source a series belongs to.
    pub fn manga_series_source(&self, id: &str) -> Result<Option<String>> {
        self.with(|c| c.query_row("SELECT source_id FROM manga_series WHERE id = ?1", params![id], |r| r.get(0)).optional())
    }

    /// The text layer a volume shows (`None` = its main `.mokuro`).
    pub fn manga_text_layer(&self, volume_id: &str) -> Option<String> {
        self.get_kv(&layer_key(volume_id))
    }

    pub fn set_manga_text_layer(&self, volume_id: &str, layer: Option<&str>) -> Result<()> {
        match layer {
            Some(layer) => self.set_kv(&layer_key(volume_id), layer),
            None => self.delete_kv(&layer_key(volume_id)),
        }
    }

    pub fn manga_volume(&self, id: &str) -> Result<Option<MangaVolume>> {
        let series_id: Option<String> = self.with(|c| {
            c.query_row("SELECT series_id FROM manga_volumes WHERE id = ?1", params![id], |r| r.get(0)).optional()
        })?;
        let Some(series_id) = series_id else { return Ok(None) };
        Ok(self.manga_volumes(&series_id)?.into_iter().find(|volume| volume.id == id))
    }

    /// Where a volume's pages are. Only Rust asks: the protocol and the reader.
    pub fn manga_volume_location(&self, id: &str) -> Result<Option<VolumeLocation>> {
        self.with(|c| {
            c.query_row(&format!("{VOLUME_LOCATION} WHERE v.id = ?1"), params![id], map_volume_location)
                .optional()
        })
    }

    /// The series' own cover image if it has one, and its first volume, whose
    /// first page is the cover otherwise.
    pub fn manga_series_cover(&self, id: &str) -> Result<Option<(Option<String>, Option<VolumeLocation>)>> {
        let cover: Option<Option<String>> = self.with(|c| {
            c.query_row("SELECT cover FROM manga_series WHERE id = ?1", params![id], |r| r.get(0)).optional()
        })?;
        let Some(cover) = cover else { return Ok(None) };
        let first = self.with(|c| {
            c.query_row(
                &format!("{VOLUME_LOCATION} WHERE v.series_id = ?1 AND (v.page_count > 0 OR v.cover IS NOT NULL) ORDER BY v.position, v.title LIMIT 1"),
                params![id],
                map_volume_location,
            )
            .optional()
        })?;
        Ok(Some((cover, first)))
    }

    /// A series from an extension, added or refreshed. `adult` comes from its
    /// source's rating and sticks (`source_adult`): linking it to a media
    /// that is not adult does not make it safe.
    pub fn upsert_extension_series(&self, id: &str, source_id: &str, manga_id: &str, title: &str, cover: Option<&str>, adult: bool) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO manga_series (id, source, source_id, source_ref, title, cover, is_adult, source_adult, created_at)
                 VALUES (?1, 'suwayomi', ?2, ?3, ?4, ?5, ?6, ?6, ?7)
                 ON CONFLICT(id) DO UPDATE SET
                    title = excluded.title,
                    cover = excluded.cover,
                    source_adult = MAX(manga_series.source_adult, excluded.source_adult),
                    is_adult = MAX(manga_series.is_adult, excluded.source_adult)",
                params![id, source_id, manga_id, title, cover, adult as i64, now_iso()],
            )
            .map(|_| ())
        })
    }

    /// `(source kind, source id, cover)` of a series. Rust only: the cover
    /// may be a path on a server.
    pub fn manga_series_origin(&self, id: &str) -> Result<Option<(String, String, Option<String>)>> {
        self.with(|c| {
            c.query_row(
                "SELECT COALESCE(m.kind, s.source), s.source_id, s.cover FROM manga_series s LEFT JOIN manga_sources m ON m.id = s.source_id WHERE s.id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
        })
    }

    pub fn link_manga_series(&self, id: &str, media_id: Option<&str>, media_title: Option<&str>, is_adult: bool) -> Result<bool> {
        self.with(|c| {
            c.execute(
                "UPDATE manga_series SET media_id = ?2, media_title = ?3, is_adult = MAX(?4, source_adult) WHERE id = ?1",
                params![id, media_id, media_title, is_adult as i64],
            )
            .map(|changed| changed > 0)
        })
    }

    // ── progress ───────────────────────────────────────────────────────

    /// Where the user is in a volume. Finishing it sticks: going back to look
    /// at a page does not make a read volume unread.
    pub fn set_manga_progress(&self, volume_id: &str, page: i64, finished: bool) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO manga_progress (volume_id, page, finished, updated_at) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(volume_id) DO UPDATE SET
                    page = excluded.page,
                    finished = MAX(manga_progress.finished, excluded.finished),
                    updated_at = excluded.updated_at",
                params![volume_id, page.max(0), finished as i64, now_iso()],
            )
            .map(|_| ())
        })
    }

    /// The last volume read in each series, newest first. A series whose last
    /// volume was finished offers the next one instead; a finished series
    /// leaves the shelf.
    pub fn manga_continue_reading(&self, limit: usize) -> Result<Vec<MangaContinue>> {
        let latest: Vec<(String, String, String, String, i64, i64, bool, String, i64)> = self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT s.id, s.title, v.id, v.title, p.page, v.page_count, p.finished, p.updated_at, v.position
                 FROM manga_progress p
                 JOIN manga_volumes v ON v.id = p.volume_id
                 JOIN manga_series s ON s.id = v.series_id
                 ORDER BY p.updated_at DESC",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get::<_, i64>(6)? != 0, r.get(7)?, r.get(8)?))
            })?;
            rows.collect()
        })?;
        let mut seen = HashSet::new();
        let mut shelf = Vec::new();
        for (series_id, series_title, volume_id, volume_title, page, page_count, finished, updated_at, position) in latest {
            if !seen.insert(series_id.clone()) {
                continue;
            }
            if !finished {
                shelf.push(MangaContinue {
                    series_id, series_title, volume_id, volume_title,
                    page: page.min((page_count - 1).max(0)), page_count, up_next: false, updated_at,
                });
            } else if let Some(next) = self
                .manga_volumes(&series_id)?
                .into_iter()
                .find(|volume| volume.position > position && !volume.finished && volume.page_count > 0)
            {
                shelf.push(MangaContinue {
                    series_id, series_title,
                    volume_id: next.id, volume_title: next.title,
                    page: next.page.unwrap_or(0), page_count: next.page_count,
                    up_next: next.page.is_none(), updated_at,
                });
            }
            if shelf.len() >= limit {
                break;
            }
        }
        Ok(shelf)
    }

    // ── text ───────────────────────────────────────────────────────────

    /// Text stored for a volume's pages (OCR results), as `(page, blocks, engine)`.
    pub fn manga_page_text(&self, volume_id: &str) -> Result<Vec<(i64, String, String)>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT page, blocks_json, engine FROM manga_page_text WHERE volume_id = ?1 ORDER BY page",
            )?;
            let rows = stmt.query_map(params![volume_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            rows.collect()
        })
    }

    // ── reading ────────────────────────────────────────────────────────

    /// Records that a page was read, with the characters it had. `true` only
    /// the first time: a page counts once, however often it is shown again.
    pub fn mark_manga_page_read(&self, volume_id: &str, page: i64, chars: i64) -> Result<bool> {
        self.with(|c| {
            c.execute(
                "INSERT OR IGNORE INTO manga_page_read (volume_id, page, chars, read_at) VALUES (?1, ?2, ?3, ?4)",
                params![volume_id, page, chars.max(0), now_iso()],
            )
            .map(|changed| changed > 0)
        })
    }

    /// A page counted without text gets its characters, once it has text.
    /// `true` only for a page that was counted with none.
    pub fn fill_manga_page_chars(&self, volume_id: &str, page: i64, chars: i64) -> Result<bool> {
        self.with(|c| {
            c.execute(
                "UPDATE manga_page_read SET chars = ?3 WHERE volume_id = ?1 AND page = ?2 AND chars = 0",
                params![volume_id, page, chars.max(0)],
            )
            .map(|changed| changed > 0)
        })
    }

    /// Pages of a volume already counted, and their characters.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn manga_read_totals(&self, volume_id: &str) -> Result<(i64, i64)> {
        self.with(|c| {
            c.query_row(
                "SELECT COUNT(*), COALESCE(SUM(chars), 0) FROM manga_page_read WHERE volume_id = ?1",
                params![volume_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
        })
    }

    /// Stores a page's text. A volume with no text of any kind becomes an
    /// `ocr` one.
    pub fn set_manga_page_text(&self, volume_id: &str, page: i64, blocks_json: &str, engine: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO manga_page_text (volume_id, page, blocks_json, engine) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(volume_id, page) DO UPDATE SET blocks_json = excluded.blocks_json, engine = excluded.engine",
                params![volume_id, page, blocks_json, engine],
            )?;
            c.execute("UPDATE manga_volumes SET ocr_state = 'ocr' WHERE id = ?1 AND ocr_state = 'none'", params![volume_id])
                .map(|_| ())
        })
    }

    /// A `.mokuro` was written beside a volume: it is the volume's now.
    pub fn set_manga_volume_mokuro(&self, volume_id: &str, path: &str) -> Result<()> {
        self.with(|c| {
            c.execute("UPDATE manga_volumes SET mokuro_path = ?2, ocr_state = 'mokuro' WHERE id = ?1", params![volume_id, path]).map(|_| ())
        })
    }

    /// The engine that wrote a page's stored text, if any.
    pub fn manga_page_text_engine(&self, volume_id: &str, page: i64) -> Result<Option<String>> {
        self.with(|c| {
            c.query_row(
                "SELECT engine FROM manga_page_text WHERE volume_id = ?1 AND page = ?2",
                params![volume_id, page],
                |r| r.get(0),
            )
            .optional()
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn temp_db() -> (Db, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!("ntdc-manga-{}.sqlite3", uuid::Uuid::new_v4()));
        (Db::open(&path).unwrap(), path)
    }

    pub fn volume(id: &str, position: i64, pages: usize) -> VolumeRecord {
        VolumeRecord {
            id: id.into(),
            title: format!("Vol {position}"),
            source_ref: format!("/manga/{id}.cbz"),
            format: FORMAT_ARCHIVE,
            position,
            pages: (1..=pages).map(|n| format!("{n:03}.jpg")).collect(),
            signature: "1:1".into(),
            mokuro_path: None,
        }
    }

    pub fn series(id: &str, title: &str, volumes: Vec<VolumeRecord>) -> SeriesRecord {
        SeriesRecord { id: id.into(), source_ref: format!("/manga/{title}"), title: title.into(), cover: None, volumes }
    }

    #[test]
    fn a_scan_adds_updates_and_removes_without_losing_links_or_progress() {
        let (db, path) = temp_db();
        db.add_manga_source("src", SOURCE_LOCAL, "Manga", "/manga").unwrap();
        let mut first = series("s1", "Yotsuba", vec![volume("v1", 1, 3), volume("v2", 2, 5)]);
        db.apply_manga_scan("src", SOURCE_LOCAL, &[first.clone(), series("s2", "Other", vec![volume("v3", 1, 2)])]).unwrap();
        db.link_manga_series("s1", Some("30104"), Some("Yotsuba&!"), false).unwrap();
        db.set_manga_progress("v2", 2, false).unwrap();

        // v2 changed on disk, v1 is gone, "Other" is gone.
        first.volumes = vec![VolumeRecord { signature: "2:2".into(), ..volume("v2", 2, 6) }];
        let changes = db.apply_manga_scan("src", SOURCE_LOCAL, &[first]).unwrap();
        let mut stale = changes.stale_volumes.clone();
        stale.sort();
        assert_eq!(stale, ["v1", "v2", "v3"]);

        let list = db.manga_series_list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].media_id.as_deref(), Some("30104"));
        assert_eq!(list[0].volume_count, 1);
        assert_eq!(list[0].page_count, 6);
        assert_eq!(list[0].pages_read, 3);
        let volumes = db.manga_volumes("s1").unwrap();
        assert_eq!(volumes[0].page, Some(2));

        // Coming back with the same id finds its progress again.
        db.set_manga_progress("v1", 1, false).unwrap();
        db.apply_manga_scan("src", SOURCE_LOCAL, &[series("s1", "Yotsuba", vec![volume("v1", 1, 3), volume("v2", 2, 6)])]).unwrap();
        assert_eq!(db.manga_volume("v1").unwrap().unwrap().page, Some(1));
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn sources_never_carry_their_credentials_to_the_ui() {
        let (db, path) = temp_db();
        db.add_manga_source("src", SOURCE_LOCAL, "Manga", "/manga").unwrap();
        db.with(|c| c.execute("UPDATE manga_sources SET auth_json = '{\"password\":\"hunter2\"}'", []).map(|_| ())).unwrap();
        let listed = serde_json::to_string(&db.manga_sources().unwrap()).unwrap();
        assert!(!listed.contains("hunter2"));
        assert!(db.manga_source_auth("src").unwrap().unwrap().contains("hunter2"));

        // A WebDAV source: the UI learns the kind of its credentials, never them.
        let auth = crate::manga::webdav::client::Auth::Basic { username: "yuki-login".into(), password: "hunter2".into() };
        db.add_manga_source("dav", SOURCE_WEBDAV, "Server", "https://files.example/").unwrap();
        db.set_manga_source_auth("dav", auth.to_json().as_deref()).unwrap();
        db.apply_remote_series("dav", &[("rs".into(), "/m/S/".into(), "S".into())]).unwrap();
        let sources = db.manga_sources().unwrap();
        assert_eq!(sources.iter().find(|s| s.id == "dav").unwrap().auth, "basic");
        let library = MangaLibrary { sources: sources.clone(), series: db.manga_series_list().unwrap(), continue_reading: Vec::new() };
        let detail = MangaSeriesDetail { series: db.manga_series("rs").unwrap().unwrap(), volumes: db.manga_volumes("rs").unwrap(), source: sources.into_iter().find(|s| s.id == "dav") };
        for json in [serde_json::to_string(&library).unwrap(), serde_json::to_string(&detail).unwrap()] {
            assert!(!json.contains("hunter2") && !json.contains("yuki-login"), "{json}");
        }
        db.delete_manga_source("dav").unwrap();

        db.apply_manga_scan("src", SOURCE_LOCAL, &[series("s1", "A", vec![volume("v1", 1, 2)])]).unwrap();
        assert_eq!(db.delete_manga_source("src").unwrap(), ["v1"]);
        assert!(db.manga_series_list().unwrap().is_empty());
        assert!(db.manga_volume_location("v1").unwrap().is_none());
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn continue_reading_offers_the_next_volume_after_a_finished_one() {
        let (db, path) = temp_db();
        db.add_manga_source("src", SOURCE_LOCAL, "Manga", "/manga").unwrap();
        db.apply_manga_scan(
            "src",
            SOURCE_LOCAL,
            &[
                series("s1", "A", vec![volume("a1", 1, 10), volume("a2", 2, 10)]),
                series("s2", "B", vec![volume("b1", 1, 10)]),
                series("s3", "C", vec![volume("c1", 1, 10)]),
            ],
        )
        .unwrap();
        db.set_manga_progress("c1", 9, true).unwrap();
        db.set_manga_progress("b1", 4, false).unwrap();
        db.set_manga_progress("a1", 9, true).unwrap();
        // Looking back at a page of a finished volume keeps it finished.
        db.set_manga_progress("a1", 3, false).unwrap();

        let shelf = db.manga_continue_reading(10).unwrap();
        let ids: Vec<_> = shelf.iter().map(|item| (item.volume_id.as_str(), item.page, item.up_next)).collect();
        assert_eq!(ids, [("a2", 0, true), ("b1", 4, false)]);
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_page_counts_once() {
        let (db, path) = temp_db();
        assert!(db.mark_manga_page_read("v1", 3, 120).unwrap());
        assert!(!db.mark_manga_page_read("v1", 3, 999).unwrap(), "shown again: not read again");
        assert!(db.mark_manga_page_read("v1", 4, 0).unwrap());
        assert!(db.mark_manga_page_read("v2", 3, 10).unwrap(), "another volume's page 3");
        assert_eq!(db.manga_read_totals("v1").unwrap(), (2, 120));
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn page_text_is_stored_per_page() {
        let (db, path) = temp_db();
        db.set_manga_page_text("v1", 2, "[]", "mokuro").unwrap();
        db.set_manga_page_text("v1", 2, "[{\"lines\":[\"あ\"]}]", "builtin").unwrap();
        db.set_manga_page_text("v1", 0, "[]", "builtin").unwrap();
        let text = db.manga_page_text("v1").unwrap();
        assert_eq!(text.len(), 2);
        assert_eq!(text[1], (2, "[{\"lines\":[\"あ\"]}]".to_string(), "builtin".to_string()));
        drop(db);
        let _ = std::fs::remove_file(path);
    }
}
