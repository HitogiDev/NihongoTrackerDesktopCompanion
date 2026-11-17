//! Reading in the manga reader, as NihongoTracker counts it.
//!
//! The reader window says which volume it shows (`open`) and which pages are
//! on screen (`show`); everything else is decided here, on the watcher's tick:
//! a page counts once it has been up for `manga_page_dwell_secs`, with the
//! characters of its blocks (kana and kanji only, `count_japanese`), and only
//! the first time (`manga_page_read`). A page without text still counts as a
//! page.

use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Manager};

use super::mokuro::MokuroPage;
use crate::config::Settings;
use crate::db::Db;
use crate::state::AppState;

/// The volume the reader window shows.
#[derive(Debug, Clone)]
pub struct OpenVolume {
    pub volume_id: String,
    pub series_id: String,
    pub series_title: String,
    pub volume_title: String,
    pub media_id: Option<String>,
    pub media_title: Option<String>,
    pub is_adult: bool,
    pub page_count: i64,
    /// Its text, page by page (`None` where there is none).
    pub text: Arc<Vec<Option<MokuroPage>>>,
    /// The pages on screen now: two in a spread, none past the last page.
    pub visible: Vec<i64>,
    /// How long they have been on screen, counting only reading time.
    dwell: Duration,
    /// They already counted: nothing more to do until the pages change.
    credited: bool,
}

impl OpenVolume {
    pub fn new(volume: &super::reader::ReaderVolume, text: Vec<Option<MokuroPage>>) -> Self {
        OpenVolume {
            volume_id: volume.volume.id.clone(),
            series_id: volume.series.id.clone(),
            series_title: volume.series.title.clone(),
            volume_title: volume.volume.title.clone(),
            media_id: volume.series.media_id.clone(),
            media_title: volume.series.media_title.clone(),
            is_adult: volume.series.is_adult,
            page_count: volume.volume.page_count,
            text: Arc::new(text),
            visible: Vec::new(),
            dwell: Duration::ZERO,
            credited: false,
        }
    }

    /// The pages now on screen. Out-of-range numbers are dropped; the same
    /// pages again change nothing, so their time keeps adding up.
    pub fn show(&mut self, pages: &[i64]) {
        let mut pages: Vec<i64> = pages.iter().copied().filter(|page| (0..self.page_count).contains(page)).collect();
        pages.sort_unstable();
        pages.dedup();
        if pages != self.visible {
            self.visible = pages;
            self.dwell = Duration::ZERO;
            self.credited = false;
        }
    }

    /// Adds reading time to the pages on screen. Once they have been up for
    /// `threshold`, returns them, once.
    pub fn dwell(&mut self, elapsed: Duration, threshold: Duration) -> Option<Vec<i64>> {
        if self.credited || self.visible.is_empty() {
            return None;
        }
        self.dwell += elapsed;
        if self.dwell < threshold {
            return None;
        }
        self.credited = true;
        Some(self.visible.clone())
    }

    pub fn page_text(&self, page: i64) -> Option<&MokuroPage> {
        usize::try_from(page).ok().and_then(|page| self.text.get(page)).and_then(Option::as_ref)
    }
}

/// What the reader is doing, kept in `manga::Runtime`.
#[derive(Debug, Default)]
pub struct Tracking {
    pub open: Option<OpenVolume>,
}

/// Kana and kanji on a page, the way the site counts them.
pub fn page_chars(page: Option<&MokuroPage>) -> i64 {
    page.map_or(0, |page| {
        page.blocks.iter().map(|block| crate::texthooker::count_japanese(&block.lines.concat())).sum()
    })
}

/// Pages and characters that were read for the first time.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Credited {
    pub pages: i64,
    pub chars: i64,
}

/// Records `pages` of the open volume as read. Pages read before add nothing.
pub fn credit_pages(db: &Db, open: &OpenVolume, pages: &[i64]) -> Credited {
    let mut credited = Credited::default();
    for &page in pages {
        let chars = page_chars(open.page_text(page));
        match db.mark_manga_page_read(&open.volume_id, page, chars) {
            Ok(true) => {
                credited.pages += 1;
                credited.chars += chars;
            }
            Ok(false) => {}
            Err(error) => log::warn!("manga page {}/{page} not recorded: {error}", open.volume_id),
        }
    }
    credited
}

pub fn dwell_threshold(settings: &Settings) -> Duration {
    Duration::from_secs(settings.manga_page_dwell_secs.clamp(1, 120))
}

/// Called on every watcher tick. Only the database is touched: no network.
pub fn tick(app: &AppHandle, settings: &Settings, elapsed: Duration) {
    let state = app.state::<AppState>();
    let mut tracking = state.manga.tracking.lock().unwrap();
    let Some(open) = tracking.open.as_mut() else { return };
    if let Some(pages) = open.dwell(elapsed, dwell_threshold(settings)) {
        credit_pages(&state.db, open, &pages);
    }
}

/// The reader window is gone: nothing is on screen any more.
pub fn reader_closed(app: &AppHandle) {
    app.state::<AppState>().manga.tracking.lock().unwrap().open = None;
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::manga::mokuro::MokuroBlock;
    use crate::manga::store::tests::temp_db;

    pub fn page(lines: &[&str]) -> MokuroPage {
        MokuroPage {
            img_path: String::new(),
            img_width: 800.0,
            img_height: 1200.0,
            blocks: lines
                .iter()
                .map(|line| MokuroBlock { bbox: [10.0, 10.0, 100.0, 300.0], vertical: true, lines: vec![line.to_string()], ..Default::default() })
                .collect(),
        }
    }

    pub fn open_volume(text: Vec<Option<MokuroPage>>) -> OpenVolume {
        OpenVolume {
            volume_id: "v1".into(),
            series_id: "s1".into(),
            series_title: "よつばと!".into(),
            volume_title: "第1巻".into(),
            media_id: None,
            media_title: None,
            is_adult: false,
            page_count: text.len() as i64,
            text: Arc::new(text),
            visible: Vec::new(),
            dwell: Duration::ZERO,
            credited: false,
        }
    }

    const SECOND: Duration = Duration::from_secs(1);

    #[test]
    fn characters_are_kana_and_kanji_only() {
        assert_eq!(page_chars(Some(&page(&["こんにちは、世界!", "ABC 123"]))), 7);
        assert_eq!(page_chars(None), 0);
    }

    #[test]
    fn a_page_counts_after_the_dwell_time_and_only_once() {
        let (db, path) = temp_db();
        let mut open = open_volume(vec![Some(page(&["あいう"])), Some(page(&["かきくけこ"])), None]);
        open.show(&[0]);
        assert_eq!(open.dwell(SECOND, 3 * SECOND), None);
        assert_eq!(open.dwell(SECOND, 3 * SECOND), None);
        let pages = open.dwell(SECOND, 3 * SECOND).expect("three seconds on screen");
        assert_eq!(credit_pages(&db, &open, &pages), Credited { pages: 1, chars: 3 });
        assert_eq!(open.dwell(10 * SECOND, 3 * SECOND), None, "already counted");

        // Back and forth: page 0 again adds nothing.
        open.show(&[1]);
        open.show(&[0]);
        let pages = open.dwell(5 * SECOND, 3 * SECOND).unwrap();
        assert_eq!(credit_pages(&db, &open, &pages), Credited::default());
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_page_turned_too_soon_does_not_count() {
        let mut open = open_volume(vec![None, None]);
        open.show(&[0]);
        assert_eq!(open.dwell(2 * SECOND, 3 * SECOND), None);
        open.show(&[1]);
        assert_eq!(open.dwell(2 * SECOND, 3 * SECOND), None, "the time does not carry over");
        assert_eq!(open.dwell(SECOND, 3 * SECOND), Some(vec![1]));
    }

    #[test]
    fn both_pages_of_a_spread_count_and_a_page_without_text_counts_as_a_page() {
        let (db, path) = temp_db();
        let mut open = open_volume(vec![None, Some(page(&["日本語"])), None]);
        open.show(&[2, 1, 1, 7, -1]);
        assert_eq!(open.visible, [1, 2], "sorted, deduplicated, in range");
        let pages = open.dwell(3 * SECOND, 3 * SECOND).unwrap();
        assert_eq!(credit_pages(&db, &open, &pages), Credited { pages: 2, chars: 3 });
        assert_eq!(db.manga_read_totals("v1").unwrap(), (2, 3));
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn nothing_on_screen_counts_nothing() {
        let mut open = open_volume(vec![None]);
        open.show(&[]);
        assert_eq!(open.dwell(60 * SECOND, 3 * SECOND), None);
    }
}
