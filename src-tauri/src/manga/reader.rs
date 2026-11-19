//! The reader window (`manga-reader`): which volume it shows, and how the user
//! likes to read.
//!
//! Layout and direction are remembered per series (a webtoon and a tankōbon
//! do not read alike); fit, background and the text layer are the user's,
//! whatever they read. Both live in `kv`. The last layout and direction chosen
//! become the default for series never opened before.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use super::store::{MangaSeries, MangaVolume};
use crate::db::Db;
use crate::state::AppState;

pub const LABEL: &str = "manga-reader";
/// To the reader window, when it already exists and is asked for another volume.
pub const OPEN_EVENT: &str = "manga://reader-open";

const GLOBAL_KEY: &str = "manga:reader";
fn series_key(series_id: &str) -> String {
    format!("manga:reader:{series_id}")
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub enum ReaderLayout {
    Single,
    #[default]
    Double,
    /// A continuous strip, for webtoons.
    Vertical,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub enum ReadingDirection {
    #[default]
    Rtl,
    Ltr,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub enum PageFit {
    #[default]
    Height,
    Width,
    Original,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub enum ReaderBackground {
    #[default]
    Black,
    Gray,
    Paper,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub enum TextLayerMode {
    /// Invisible until the pointer is over a block, like mokuro's reader.
    #[default]
    Hover,
    /// Never drawn, not even on hover (no text, no box), but there: a
    /// Shift/Alt hover still looks it up, and Ctrl+C / Ctrl+M still work.
    Invisible,
    Always,
    Hidden,
}

/// Everything the reader lets the user choose.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct ReaderPrefs {
    pub layout: ReaderLayout,
    pub direction: ReadingDirection,
    /// Double page: the first page stands alone (a cover), so the pairs that
    /// follow are the book's spreads. Off shifts them by one.
    pub cover_alone: bool,
    pub fit: PageFit,
    pub background: ReaderBackground,
    pub text_layer: TextLayerMode,
}

impl Default for ReaderPrefs {
    fn default() -> Self {
        ReaderPrefs {
            layout: ReaderLayout::default(),
            direction: ReadingDirection::default(),
            cover_alone: true,
            fit: PageFit::default(),
            background: ReaderBackground::default(),
            text_layer: TextLayerMode::default(),
        }
    }
}

/// The per-series part.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
struct SeriesPrefs {
    layout: ReaderLayout,
    direction: ReadingDirection,
    cover_alone: bool,
}

fn read<T: for<'de> Deserialize<'de>>(db: &Db, key: &str) -> Option<T> {
    db.get_kv(key).and_then(|raw| serde_json::from_str(&raw).ok())
}

/// The user's preferences for this series: its own layout and direction if it
/// has been read before, the last ones chosen otherwise.
pub fn prefs(db: &Db, series_id: &str) -> ReaderPrefs {
    let mut prefs: ReaderPrefs = read(db, GLOBAL_KEY).unwrap_or_default();
    if let Some(series) = read::<SeriesPrefs>(db, &series_key(series_id)) {
        prefs.layout = series.layout;
        prefs.direction = series.direction;
        prefs.cover_alone = series.cover_alone;
    }
    prefs
}

pub fn save_prefs(db: &Db, series_id: &str, prefs: &ReaderPrefs) -> crate::db::Result<()> {
    let series = SeriesPrefs { layout: prefs.layout, direction: prefs.direction, cover_alone: prefs.cover_alone };
    db.set_kv(&series_key(series_id), &serde_json::to_string(&series).unwrap_or_default())?;
    db.set_kv(GLOBAL_KEY, &serde_json::to_string(prefs).unwrap_or_default())
}

/// What the reader window shows, or is about to: it asks with
/// `manga_reader_target` once its page has loaded.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct ReaderTarget {
    pub volume_id: String,
    /// Open at this page instead of where the user left off.
    pub page: Option<i64>,
}

/// A volume as the reader needs it.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct ReaderVolume {
    pub series: MangaSeries,
    pub volume: MangaVolume,
    pub prefs: ReaderPrefs,
    /// The volumes before and after it in the series, for "next volume".
    pub previous: Option<MangaVolume>,
    pub next: Option<MangaVolume>,
}

pub fn volume(db: &Db, volume_id: &str) -> crate::db::Result<Option<ReaderVolume>> {
    let Some(volume) = db.manga_volume(volume_id)? else { return Ok(None) };
    let Some(series) = db.manga_series(&volume.series_id)? else { return Ok(None) };
    let siblings: Vec<MangaVolume> = db
        .manga_volumes(&series.id)?
        .into_iter()
        // A remote archive has no page list until it is first opened.
        .filter(|v| v.page_count > 0 || super::store::is_served_format(&v.format))
        .collect();
    let index = siblings.iter().position(|v| v.id == volume.id);
    let previous = index.and_then(|i| i.checked_sub(1)).and_then(|i| siblings.get(i).cloned());
    let next = index.and_then(|i| siblings.get(i + 1).cloned());
    Ok(Some(ReaderVolume { prefs: prefs(db, &series.id), series, volume, previous, next }))
}

/// Points the reader at a volume and brings its window up, building it if
/// needed. An open window is told by event; a new one asks once it loads.
pub async fn open(app: &AppHandle, volume_id: &str, page: Option<i64>) -> Result<(), String> {
    let state = app.state::<AppState>();
    let Some(location) = state.db.manga_volume_location(volume_id).map_err(|e| e.to_string())? else {
        return Err("Unknown volume".into());
    };
    // A remote archive opened for the first time: its central directory
    // first (a few requests). A server without ranges answers
    // `needs-download`, and the UI downloads it with progress.
    // A chapter from an extension asks its server for its pages (starting the
    // managed server if needed).
    if super::store::is_served_format(&location.format) && location.pages.is_empty() {
        let remote = state.manga.remote().ok_or("error")?;
        tauri::async_runtime::spawn_blocking(move || remote.ensure_index(&location))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.code().to_string())?;
        let _ = app.emit(super::library::LIBRARY_CHANGED, ());
    }
    let target = ReaderTarget { volume_id: volume_id.to_string(), page };
    *state.manga.reader.lock().unwrap() = Some(target.clone());
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.emit_to(LABEL, OPEN_EVENT, &target);
    }
    crate::commands::open_aux_window_inner(app, LABEL).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manga::store::tests::{series, temp_db, volume as record};
    use crate::manga::store::SOURCE_LOCAL;

    #[test]
    fn layout_and_direction_are_per_series_the_rest_is_global() {
        let (db, path) = temp_db();
        assert_eq!(prefs(&db, "a"), ReaderPrefs::default());

        let webtoon = ReaderPrefs {
            layout: ReaderLayout::Vertical,
            direction: ReadingDirection::Ltr,
            fit: PageFit::Width,
            background: ReaderBackground::Paper,
            ..Default::default()
        };
        save_prefs(&db, "a", &webtoon).unwrap();
        let manga = ReaderPrefs { layout: ReaderLayout::Single, fit: PageFit::Height, ..webtoon };
        save_prefs(&db, "b", &ReaderPrefs { direction: ReadingDirection::Rtl, ..manga }).unwrap();

        let a = prefs(&db, "a");
        assert_eq!((a.layout, a.direction), (ReaderLayout::Vertical, ReadingDirection::Ltr));
        // Fit is global: the last one chosen.
        assert_eq!(a.fit, PageFit::Height);
        assert_eq!(a.background, ReaderBackground::Paper);
        // A series never opened takes the last layout and direction.
        let c = prefs(&db, "c");
        assert_eq!((c.layout, c.direction), (ReaderLayout::Single, ReadingDirection::Rtl));
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn unknown_values_in_storage_fall_back_to_defaults() {
        let (db, path) = temp_db();
        db.set_kv(GLOBAL_KEY, r#"{"fit":"sideways","background":"gray"}"#).unwrap();
        assert_eq!(prefs(&db, "x"), ReaderPrefs::default());
        db.set_kv(GLOBAL_KEY, r#"{"background":"gray"}"#).unwrap();
        assert_eq!(prefs(&db, "x").background, ReaderBackground::Gray);
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_volume_knows_its_neighbours() {
        let (db, path) = temp_db();
        db.add_manga_source("src", SOURCE_LOCAL, "Manga", "/m").unwrap();
        db.apply_manga_scan("src", SOURCE_LOCAL, &[series("s", "S", vec![record("v1", 1, 3), record("v2", 2, 3), record("v3", 3, 3)])]).unwrap();
        let middle = volume(&db, "v2").unwrap().unwrap();
        assert_eq!(middle.previous.unwrap().id, "v1");
        assert_eq!(middle.next.unwrap().id, "v3");
        assert!(volume(&db, "v1").unwrap().unwrap().previous.is_none());
        assert!(volume(&db, "nope").unwrap().is_none());
        drop(db);
        let _ = std::fs::remove_file(path);
    }
}
