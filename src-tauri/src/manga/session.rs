//! Reading in the manga reader, as NihongoTracker counts it.
//!
//! The reader window says which volume it shows (`open`) and which pages are
//! on screen (`show`); everything else is decided here, on the watcher's tick:
//! a page counts once it has been up for `manga_page_dwell_secs`, with the
//! characters of its blocks (kana and kanji only, `count_japanese`), and only
//! the first time (`manga_page_read`). A page without text still counts as a
//! page.
//!
//! The session clock is Rust's too, like a VN's: it runs only while the reader
//! window has the focus (`Runtime::reader_focused`, from its window events) and
//! the user is not away (`afk_timeout_secs`). A page's dwell time only adds up
//! while the clock runs. The UI hears about it through `manga://session` and
//! the tracker snapshot.

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

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

/// To every window: the session (or `null`) whenever it changes.
pub const SESSION_EVENT: &str = "manga://session";

/// Why the clock is stopped.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub enum MangaPause {
    /// The reader window is in the background or minimized.
    Unfocused,
    /// No input for `afk_timeout_secs`.
    Idle,
}

/// A reading session in the manga reader, as the UI shows it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct MangaSession {
    pub volume_id: String,
    pub series_id: String,
    pub series_title: String,
    pub volume_title: String,
    /// The NihongoTracker media the series is linked to, if it is.
    pub media_id: Option<String>,
    pub media_title: Option<String>,
    /// Travels with the session so Discord never publishes its title.
    pub is_adult: bool,
    /// Some page of the volume has text: characters can be counted.
    pub has_text: bool,
    /// First page on screen (0-based) and the volume's length.
    pub page: i64,
    pub page_count: i64,
    /// Pages read for the first time in this session, and their characters.
    pub pages_read: i64,
    pub chars_read: i64,
    pub elapsed_secs: u64,
    /// The clock is running at this instant.
    pub counting: bool,
    pub pause: Option<MangaPause>,
    /// How long the clock has been stopped.
    pub inactive_secs: u64,
    pub started_at: String,
}

impl MangaSession {
    fn start(open: &OpenVolume, started_at: String) -> Self {
        MangaSession {
            volume_id: open.volume_id.clone(),
            series_id: open.series_id.clone(),
            series_title: open.series_title.clone(),
            volume_title: open.volume_title.clone(),
            media_id: open.media_id.clone(),
            media_title: open.media_title.clone(),
            is_adult: open.is_adult,
            has_text: open.text.iter().any(Option::is_some),
            page: open.visible.first().copied().unwrap_or(0),
            page_count: open.page_count,
            pages_read: 0,
            chars_read: 0,
            elapsed_secs: 0,
            counting: false,
            pause: None,
            inactive_secs: 0,
            started_at,
        }
    }

    /// The title it is known by: the linked media's, or the series folder's.
    pub fn title(&self) -> &str {
        self.media_title.as_deref().filter(|title| !title.trim().is_empty()).unwrap_or(&self.series_title)
    }
}

/// What the reader is doing, kept in `manga::Runtime`.
#[derive(Debug, Default)]
pub struct Tracking {
    pub open: Option<OpenVolume>,
    pub session: Option<MangaSession>,
    /// Sub-second remainders of the session's clocks, so ticks that are not a
    /// whole second long add up to real time.
    elapsed: Duration,
    inactive: Duration,
    /// The last session sent to the UI, so an unchanged one is not sent again.
    emitted: Option<Option<MangaSession>>,
}

/// One tick's worth of what the watcher observed.
#[derive(Debug, Clone, Copy)]
pub struct TickInput {
    pub elapsed: Duration,
    /// The reader window has the focus.
    pub focused: bool,
    pub idle_secs: u64,
    /// `afk_timeout_secs`; 0 never pauses.
    pub afk_secs: u64,
    pub dwell: Duration,
}

impl Tracking {
    /// Advances the session by one tick: starts it when the reader is open and
    /// being read, runs or stops its clock, and counts the pages that have
    /// been on screen long enough.
    pub fn step(&mut self, db: &Db, input: TickInput, now: impl FnOnce() -> String) {
        let Some(open) = self.open.as_mut() else { return };
        let away = input.afk_secs > 0 && input.idle_secs >= input.afk_secs;
        let counting = input.focused && !away;
        if self.session.is_none() {
            if !counting {
                return;
            }
            self.session = Some(MangaSession::start(open, now()));
            self.elapsed = Duration::ZERO;
            self.inactive = Duration::ZERO;
        }
        let session = self.session.as_mut().expect("started above");
        session.counting = counting;
        session.pause = match (counting, input.focused) {
            (true, _) => None,
            (false, false) => Some(MangaPause::Unfocused),
            (false, true) => Some(MangaPause::Idle),
        };
        if let Some(&first) = open.visible.first() {
            session.page = first;
        }
        if counting {
            self.elapsed += input.elapsed;
            session.elapsed_secs += self.elapsed.as_secs();
            self.elapsed = Duration::from_nanos(self.elapsed.subsec_nanos() as u64);
            self.inactive = Duration::ZERO;
            session.inactive_secs = 0;
            if let Some(pages) = open.dwell(input.elapsed, input.dwell) {
                let credited = credit_pages(db, open, &pages);
                session.pages_read += credited.pages;
                session.chars_read += credited.chars;
            }
        } else {
            self.inactive += input.elapsed;
            session.inactive_secs = self.inactive.as_secs();
        }
    }

    /// The session, if it changed since the last time this was asked.
    fn changed(&mut self) -> Option<Option<MangaSession>> {
        if self.emitted.as_ref() == Some(&self.session) {
            return None;
        }
        self.emitted = Some(self.session.clone());
        self.emitted.clone()
    }
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

/// Called on every watcher tick, returning the session for the tracker
/// snapshot. Only the database is touched: no network, nothing awaited.
pub fn tick(app: &AppHandle, settings: &Settings, elapsed: Duration, idle_secs: u64) -> Option<MangaSession> {
    let state = app.state::<AppState>();
    let input = TickInput {
        elapsed,
        focused: state.manga.reader_focused.load(std::sync::atomic::Ordering::Acquire),
        idle_secs,
        afk_secs: settings.afk_timeout_secs,
        dwell: dwell_threshold(settings),
    };
    let (session, changed) = {
        let mut tracking = state.manga.tracking.lock().unwrap();
        tracking.step(&state.db, input, crate::db::now_iso);
        (tracking.session.clone(), tracking.changed())
    };
    if let Some(changed) = changed {
        let _ = app.emit(SESSION_EVENT, &changed);
    }
    session
}

/// Focus of the reader window, from its window events.
pub fn set_focused(app: &AppHandle, focused: bool) {
    app.state::<AppState>().manga.reader_focused.store(focused, std::sync::atomic::Ordering::Release);
}

/// The reader window is gone: nothing is on screen any more.
pub fn reader_closed(app: &AppHandle) {
    set_focused(app, false);
    let state = app.state::<AppState>();
    let mut tracking = state.manga.tracking.lock().unwrap();
    tracking.open = None;
    tracking.session = None;
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

    fn input(elapsed: Duration, focused: bool, idle_secs: u64) -> TickInput {
        TickInput { elapsed, focused, idle_secs, afk_secs: 120, dwell: 3 * SECOND }
    }

    fn reading(text: Vec<Option<MokuroPage>>) -> Tracking {
        Tracking { open: Some(open_volume(text)), ..Default::default() }
    }

    #[test]
    fn the_session_starts_with_the_first_tick_of_reading() {
        let (db, path) = temp_db();
        let mut tracking = reading(vec![Some(page(&["よつばと"]))]);
        tracking.step(&db, input(SECOND, false, 0), || "t0".into());
        assert!(tracking.session.is_none(), "an unfocused reader starts nothing");
        tracking.open.as_mut().unwrap().show(&[0]);
        tracking.step(&db, input(SECOND, true, 0), || "t1".into());
        let session = tracking.session.as_ref().unwrap();
        assert_eq!((session.started_at.as_str(), session.elapsed_secs, session.counting), ("t1", 1, true));
        assert!(session.has_text);
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn the_clock_stops_when_the_reader_loses_focus_or_the_user_is_away() {
        let (db, path) = temp_db();
        let mut tracking = reading(vec![Some(page(&["あ"])), Some(page(&["い"]))]);
        tracking.open.as_mut().unwrap().show(&[0]);
        tracking.step(&db, input(2 * SECOND, true, 0), || "t".into());

        // Away: the clock and the page's dwell time both stop.
        for _ in 0..5 {
            tracking.step(&db, input(SECOND, true, 130), || "t".into());
        }
        let session = tracking.session.clone().unwrap();
        assert_eq!((session.elapsed_secs, session.counting, session.pause), (2, false, Some(MangaPause::Idle)));
        assert_eq!(session.inactive_secs, 5);
        assert_eq!(session.pages_read, 0, "the page was not read while away");

        tracking.step(&db, input(SECOND, false, 0), || "t".into());
        assert_eq!(tracking.session.as_ref().unwrap().pause, Some(MangaPause::Unfocused));

        // Back: one more second and the page has had its three.
        tracking.step(&db, input(SECOND, true, 0), || "t".into());
        let session = tracking.session.clone().unwrap();
        assert_eq!((session.elapsed_secs, session.inactive_secs, session.pages_read, session.chars_read), (3, 0, 1, 1));
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn sub_second_ticks_add_up_to_whole_seconds() {
        let (db, path) = temp_db();
        let mut tracking = reading(vec![None]);
        for _ in 0..10 {
            tracking.step(&db, input(Duration::from_millis(750), true, 0), || "t".into());
        }
        assert_eq!(tracking.session.as_ref().unwrap().elapsed_secs, 7);
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn an_unchanged_session_is_not_sent_again() {
        let (db, path) = temp_db();
        let mut tracking = reading(vec![None]);
        assert_eq!(tracking.changed(), Some(None), "the first answer always goes out");
        assert_eq!(tracking.changed(), None);
        tracking.step(&db, input(SECOND, true, 0), || "t".into());
        assert!(tracking.changed().unwrap().is_some());
        tracking.step(&db, input(Duration::from_millis(10), true, 0), || "t".into());
        assert_eq!(tracking.changed(), None, "under a second: nothing visible changed");
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
