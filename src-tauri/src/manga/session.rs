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
//!
//! A session becomes a log when the reader closes, when it moves to another
//! volume, or after `manga_idle_log_minutes` with the clock stopped. The log
//! goes through `sync` like every other: to Pendientes when the user wants to
//! confirm it (`manga_confirm_before_log`) or the series is not linked to
//! NihongoTracker yet, to the outbox otherwise.

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
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
/// To every window: a session ended, and where its log went.
pub const ENDED_EVENT: &str = "manga://session-ended";

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
    /// End the session after this long with the clock stopped; zero never does.
    pub idle_log: Duration,
}

impl Tracking {
    /// Advances the session by one tick: starts it when the reader is open and
    /// being read, runs or stops its clock, and counts the pages that have
    /// been on screen long enough. Returns the session when it has been idle
    /// for so long that it is over.
    pub fn step(&mut self, db: &Db, input: TickInput, now: impl FnOnce() -> String) -> Option<MangaSession> {
        let Some(open) = self.open.as_mut() else { return self.finish() };
        let away = input.afk_secs > 0 && input.idle_secs >= input.afk_secs;
        let counting = input.focused && !away;
        if self.session.is_none() {
            if !counting {
                return None;
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
            if !input.idle_log.is_zero() && self.inactive >= input.idle_log {
                return self.finish();
            }
        }
        None
    }

    /// Ends the session: it is the caller's to log.
    pub fn finish(&mut self) -> Option<MangaSession> {
        self.elapsed = Duration::ZERO;
        self.inactive = Duration::ZERO;
        self.session.take()
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

/// Whole minutes for the log, rounded: the site takes `time` in minutes.
pub fn log_minutes(seconds: u64) -> u64 {
    (seconds + 30) / 60
}

/// The log for a finished session, or `None` when there is nothing to log
/// (no page read for the first time and under half a minute). Follows
/// `src/lib/logRules.ts` for manga: `pages`, `chars` or `time`, `time` in
/// minutes, and always a `description`.
pub fn log_payload(session: &MangaSession, date: &str) -> Option<Value> {
    let minutes = log_minutes(session.elapsed_secs);
    if session.pages_read <= 0 && session.chars_read <= 0 && minutes == 0 {
        return None;
    }
    let mut payload = json!({
        "type": "manga",
        "mediaTitle": session.title(),
        "description": format!("{} — {}", session.series_title, session.volume_title),
        "date": date,
    });
    if let Some(media_id) = session.media_id.as_deref().filter(|id| !id.trim().is_empty()) {
        payload["mediaId"] = json!(media_id);
    }
    if session.pages_read > 0 {
        payload["pages"] = json!(session.pages_read);
    }
    if session.chars_read > 0 {
        payload["chars"] = json!(session.chars_read);
    }
    if minutes > 0 {
        payload["time"] = json!(minutes);
    }
    // What the Pendientes card shows; dropped before the log is sent.
    payload["_detected"] = json!({
        "source": "manga",
        "seriesId": session.series_id,
        "volumeId": session.volume_id,
        "volume": session.volume_title,
        "seconds": session.elapsed_secs,
    });
    Some(payload)
}

/// Where a finished session went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Logged,
    Pending,
    Discarded,
}

/// Turns a finished session into a log, through `sync`, and tells the user.
pub fn log_session(app: &AppHandle, session: &MangaSession, settings: &Settings) -> Outcome {
    let payload = log_payload(session, &chrono::Utc::now().to_rfc3339());
    // Unlinked, the log has no `mediaId`: it waits in Pendientes to be linked.
    let confirm = settings.manga_confirm_before_log || session.media_id.is_none();
    let outcome = match (&payload, confirm) {
        (None, _) => Outcome::Discarded,
        (Some(_), true) => Outcome::Pending,
        (Some(_), false) => Outcome::Logged,
    };
    if let Some(payload) = &payload {
        let lang = settings.language.as_str();
        let summary = format!(
            "{} — {}",
            session.title(),
            crate::i18n::manga_session_summary(lang, log_minutes(session.elapsed_secs), session.pages_read, session.chars_read)
        );
        if confirm {
            crate::sync::add_pending(app, payload, "manga");
        } else {
            crate::sync::enqueue_log(app, payload);
            let app = app.clone();
            tauri::async_runtime::spawn(async move { crate::sync::flush(&app).await });
        }
        if settings.notifications {
            let title = if confirm { crate::i18n::session_awaiting_confirmation(lang) } else { crate::i18n::session_logged(lang) };
            crate::notifications::show(app, title, &summary);
        }
    }
    let _ = app.emit(
        ENDED_EVENT,
        json!({
            "volumeId": session.volume_id,
            "title": session.title(),
            "pages": session.pages_read,
            "chars": session.chars_read,
            "minutes": log_minutes(session.elapsed_secs),
            "outcome": outcome,
        }),
    );
    outcome
}

/// Logs a session that ended outside the tick (the reader closed, another
/// volume opened), from a thread that cannot wait on the settings lock.
pub fn log_later(app: &AppHandle, session: MangaSession) {
    let state = app.state::<AppState>();
    if let Ok(settings) = state.settings.try_read() {
        let settings = settings.clone();
        log_session(app, &session, &settings);
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let settings = app.state::<AppState>().settings_snapshot().await;
        log_session(&app, &session, &settings);
    });
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
        idle_log: Duration::from_secs(settings.manga_idle_log_minutes.saturating_mul(60)),
    };
    let (session, changed, finished) = {
        let mut tracking = state.manga.tracking.lock().unwrap();
        let finished = tracking.step(&state.db, input, crate::db::now_iso);
        (tracking.session.clone(), tracking.changed(), finished)
    };
    if let Some(finished) = finished {
        log_session(app, &finished, settings);
    }
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
    let finished = {
        let state = app.state::<AppState>();
        let mut tracking = state.manga.tracking.lock().unwrap();
        tracking.open = None;
        tracking.finish()
    };
    if let Some(session) = finished {
        log_later(app, session);
    }
}

/// A series was linked (or unlinked) while it is being read: the session and
/// its log follow, `is_adult` included.
pub fn series_linked(app: &AppHandle, series_id: &str, media_id: Option<&str>, media_title: Option<&str>, is_adult: bool) {
    let state = app.state::<AppState>();
    let mut tracking = state.manga.tracking.lock().unwrap();
    if let Some(open) = tracking.open.as_mut().filter(|open| open.series_id == series_id) {
        open.media_id = media_id.map(str::to_string);
        open.media_title = media_title.map(str::to_string);
        open.is_adult = is_adult;
    }
    if let Some(session) = tracking.session.as_mut().filter(|session| session.series_id == series_id) {
        session.media_id = media_id.map(str::to_string);
        session.media_title = media_title.map(str::to_string);
        session.is_adult = is_adult;
    }
}

/// The reader shows `open` now. A session of another volume ends and is
/// logged; the same volume again changes nothing.
pub fn open_volume(app: &AppHandle, open: OpenVolume) {
    let finished = {
        let state = app.state::<AppState>();
        let mut tracking = state.manga.tracking.lock().unwrap();
        if tracking.open.as_ref().is_some_and(|current| current.volume_id == open.volume_id) {
            return;
        }
        tracking.open = Some(open);
        tracking.finish()
    };
    if let Some(session) = finished {
        log_later(app, session);
    }
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
        TickInput { elapsed, focused, idle_secs, afk_secs: 120, dwell: 3 * SECOND, idle_log: 600 * SECOND }
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
    fn ten_minutes_with_the_clock_stopped_end_the_session() {
        let (db, path) = temp_db();
        let mut tracking = reading(vec![None]);
        tracking.step(&db, input(SECOND, true, 0), || "t".into());
        for _ in 0..599 {
            assert!(tracking.step(&db, input(SECOND, false, 0), || "t".into()).is_none());
        }
        let ended = tracking.step(&db, input(SECOND, false, 0), || "t".into()).expect("ten minutes unfocused");
        assert_eq!(ended.elapsed_secs, 1);
        assert!(tracking.session.is_none());
        assert!(tracking.open.is_some(), "the reader is still open: coming back starts a new session");
        tracking.step(&db, input(SECOND, true, 0), || "t2".into());
        assert_eq!(tracking.session.as_ref().unwrap().started_at, "t2");
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn closing_the_reader_hands_back_the_session() {
        let (db, path) = temp_db();
        let mut tracking = reading(vec![None]);
        tracking.step(&db, input(SECOND, true, 0), || "t".into());
        tracking.open = None;
        assert!(tracking.step(&db, input(SECOND, true, 0), || "t".into()).is_some());
        assert!(tracking.finish().is_none(), "only once");
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    fn session(pages: i64, chars: i64, seconds: u64, media_id: Option<&str>) -> MangaSession {
        let mut session = MangaSession::start(&open_volume(vec![None]), "t".into());
        session.pages_read = pages;
        session.chars_read = chars;
        session.elapsed_secs = seconds;
        session.media_id = media_id.map(str::to_string);
        session.media_title = media_id.map(|_| "Yotsuba&!".to_string());
        session
    }

    /// `src/lib/logRules.ts` for manga, written out: a description, and at
    /// least one of pages, chars or time, each a positive whole number.
    fn assert_valid_manga_log(payload: &Value) {
        assert_eq!(payload["type"], "manga");
        assert!(payload["description"].as_str().is_some_and(|d| !d.trim().is_empty()), "description is required");
        assert!(payload["date"].as_str().is_some());
        let amounts: Vec<i64> = ["pages", "chars", "time"].iter().filter_map(|field| payload.get(*field)).map(|v| v.as_i64().expect("a whole number")).collect();
        assert!(!amounts.is_empty(), "manga needs pages, chars or time");
        assert!(amounts.iter().all(|amount| *amount > 0));
        assert!(payload.get("episodes").is_none());
    }

    #[test]
    fn the_log_follows_the_rules_for_manga() {
        let linked = log_payload(&session(12, 3_400, 25 * 60 + 40, Some("30104")), "2026-11-17T10:00:00Z").unwrap();
        assert_valid_manga_log(&linked);
        assert_eq!(linked["description"], "よつばと! — 第1巻");
        assert_eq!(linked["mediaId"], "30104");
        assert_eq!(linked["mediaTitle"], "Yotsuba&!");
        assert_eq!((linked["pages"].clone(), linked["chars"].clone(), linked["time"].clone()), (json!(12), json!(3_400), json!(26)));
        assert_eq!(linked["_detected"]["seriesId"], "s1");

        // Unlinked: no mediaId, the folder's title stands in.
        let unlinked = log_payload(&session(3, 0, 50, None), "2026-11-17T10:00:00Z").unwrap();
        assert_valid_manga_log(&unlinked);
        assert!(unlinked.get("mediaId").is_none());
        assert_eq!(unlinked["mediaTitle"], "よつばと!");
        assert!(unlinked.get("chars").is_none(), "no text, no characters");

        // A volume read again: no new pages, the time still counts.
        let reread = log_payload(&session(0, 0, 9 * 60, Some("30104")), "d").unwrap();
        assert_valid_manga_log(&reread);
        assert_eq!(reread["time"], 9);
        assert!(reread.get("pages").is_none());
    }

    #[test]
    fn a_session_with_nothing_read_is_not_logged() {
        assert!(log_payload(&session(0, 0, 29, None), "d").is_none());
        assert!(log_payload(&session(0, 0, 30, None), "d").is_some(), "half a minute rounds up");
    }

    #[test]
    fn nothing_on_screen_counts_nothing() {
        let mut open = open_volume(vec![None]);
        open.show(&[]);
        assert_eq!(open.dwell(60 * SECOND, 3 * SECOND), None);
    }
}
