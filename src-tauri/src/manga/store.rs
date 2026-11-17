//! The manga tables: sources, series, volumes, reading progress and the text
//! of each page.
//!
//! Ids are opaque to the UI. Series and volumes found on disk get ids derived
//! from where they are (see `library::stable_id`), so a volume that disappears
//! for a while (an unplugged drive) and comes back keeps its progress, which is
//! why `manga_progress` has no foreign key.
//!
//! A source's credentials (`auth_json`, used by WebDAV later) stay in Rust:
//! no struct sent to the UI carries them, like the account's API key never
//! leaves `Settings` for anything but the settings screen.

use std::collections::{HashMap, HashSet};

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::{now_iso, Db, Result};

use super::natural_cmp;

/// `manga_sources.kind` for a folder on this computer.
pub const SOURCE_LOCAL: &str = "local";

/// `manga_volumes.format`.
pub const FORMAT_FOLDER: &str = "folder";
pub const FORMAT_ARCHIVE: &str = "archive";

/// `manga_volumes.ocr_state`: no text at all, or text from a `.mokuro` file.
/// The OCR states of a later phase (processing, done) join these.
pub const TEXT_NONE: &str = "none";
pub const TEXT_MOKURO: &str = "mokuro";

/// A library root as the UI lists it. Never with its credentials.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct MangaSource {
    pub id: String,
    /// `local` today; `webdav` and `suwayomi` later.
    pub kind: String,
    pub name: String,
    /// A folder for `local`, a URL for the others.
    pub url: String,
    pub created_at: String,
    pub series_count: i64,
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
    /// `folder` or `archive`.
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
    Ok(VolumeLocation {
        id: r.get(0)?,
        series_id: r.get(1)?,
        title: r.get(2)?,
        source_ref: r.get(3)?,
        format: r.get(4)?,
        pages: serde_json::from_str(&pages).unwrap_or_default(),
        signature: r.get(6)?,
        mokuro_path: r.get(7)?,
    })
}

const VOLUME_LOCATION: &str =
    "SELECT id, series_id, title, source_ref, format, pages_json, signature, mokuro_path FROM manga_volumes";

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
                        (SELECT COUNT(*) FROM manga_series s WHERE s.source_id = m.id)
                 FROM manga_sources m ORDER BY m.created_at",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(MangaSource {
                    id: r.get(0)?,
                    kind: r.get(1)?,
                    name: r.get(2)?,
                    url: r.get(3)?,
                    created_at: r.get(4)?,
                    series_count: r.get(5)?,
                })
            })?;
            rows.collect()
        })
    }

    /// The source's credentials, for Rust's own requests. Never for the UI.
    #[allow(dead_code)]
    pub fn manga_source_auth(&self, id: &str) -> Result<Option<String>> {
        self.with(|c| {
            c.query_row("SELECT auth_json FROM manga_sources WHERE id = ?1", params![id], |r| r.get(0))
                .optional()
                .map(Option::flatten)
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
                        p.page, COALESCE(p.finished, 0), p.updated_at
                 FROM manga_volumes v LEFT JOIN manga_progress p ON p.volume_id = v.id
                 WHERE v.series_id = ?1 ORDER BY v.position, v.title",
            )?;
            let rows = stmt.query_map(params![series_id], |r| {
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
                })
            })?;
            rows.collect()
        })
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
            c.query_row(&format!("{VOLUME_LOCATION} WHERE id = ?1"), params![id], map_volume_location)
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
                &format!("{VOLUME_LOCATION} WHERE series_id = ?1 AND page_count > 0 ORDER BY position, title LIMIT 1"),
                params![id],
                map_volume_location,
            )
            .optional()
        })?;
        Ok(Some((cover, first)))
    }

    pub fn link_manga_series(&self, id: &str, media_id: Option<&str>, media_title: Option<&str>, is_adult: bool) -> Result<bool> {
        self.with(|c| {
            c.execute(
                "UPDATE manga_series SET media_id = ?2, media_title = ?3, is_adult = ?4 WHERE id = ?1",
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

    #[allow(dead_code)]
    pub fn set_manga_page_text(&self, volume_id: &str, page: i64, blocks_json: &str, engine: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO manga_page_text (volume_id, page, blocks_json, engine) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(volume_id, page) DO UPDATE SET blocks_json = excluded.blocks_json, engine = excluded.engine",
                params![volume_id, page, blocks_json, engine],
            )
            .map(|_| ())
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
