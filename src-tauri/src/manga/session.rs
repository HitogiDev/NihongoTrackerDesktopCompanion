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
//!
//! The blocks of a page that counts become `text_lines` (`content_id` = the
//! linked media, or `manga:<series id>`), with ids made from where they are
//! (`manga:<volume>:<page>:<block>`) so the same block is always the same
//! line: mining one, or looking a word up in it, finds its sentence and its
//! work. They are not `texthooker::ingest`ed (that follows the hooker's
//! timer) and never pushed to the texthooker API: they are stored as synced.

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
    /// No input for the reader's AFK timeout.
    Idle,
    /// The user stopped it from the clock menu.
    Stopped,
}

/// What the reader's clock is doing, with or without a session (there is
/// none until it first runs): what its menu shows.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub enum ClockStatus {
    Running,
    /// Started, but the user is away or the window is in the background:
    /// it runs again by itself.
    Paused,
    /// Stopped by the user: it only runs again from the menu.
    Stopped,
    /// It will start by itself once the user reads (`starts_in_secs`).
    Waiting,
    /// Automatic start is off and it has not been started.
    Off,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct MangaClock {
    pub volume_id: String,
    pub status: ClockStatus,
    /// Why it is paused (`Paused`, `Stopped`).
    pub pause: Option<MangaPause>,
    /// Waiting with a delay: seconds of reading left before it starts.
    pub starts_in_secs: Option<u64>,
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
    /// The clock has been started (by itself or from the menu): it runs
    /// whenever the user is `present`.
    armed: bool,
    /// The user stopped it: nothing starts it again but the menu.
    stopped: bool,
    /// Reading time towards an automatic start.
    warmup: Duration,
    /// From the last tick: the user is reading (focused, if that is asked
    /// for, and not away), why not, and the automatic start settings.
    present: bool,
    pause: Option<MangaPause>,
    auto_start: bool,
    auto_start_delay: Duration,
    clock_emitted: Option<Option<MangaClock>>,
    /// Lines of the pages `step` credited, still to be stored. The tick takes
    /// them and stores them off its thread (`store_lines`), after letting go
    /// of the lock: the count itself (`manga_page_read`) is already final.
    pub unsaved_lines: Vec<crate::db::TextLine>,
}

/// A session whose clock alone moved is sent again only after this many
/// seconds: the windows advance it themselves in between (`useMangaSession`).
pub const SESSION_RESYNC_SECS: u64 = 15;

/// One tick's worth of what the watcher observed.
#[derive(Debug, Clone, Copy)]
pub struct TickInput {
    pub elapsed: Duration,
    /// The reader window has the focus.
    pub focused: bool,
    pub idle_secs: u64,
    /// The reader's AFK timeout; 0 never pauses.
    pub afk_secs: u64,
    pub dwell: Duration,
    /// End the session after this long with the clock stopped; zero never does.
    pub idle_log: Duration,
    /// Losing the focus pauses the clock (`manga_pause_on_unfocus`).
    pub pause_on_unfocus: bool,
    /// The clock starts by itself after this much reading (`manga_auto_start`).
    pub auto_start: bool,
    pub auto_start_delay: Duration,
}

impl Tracking {
    /// Advances the session by one tick: starts it when the reader is open and
    /// being read, runs or stops its clock, and counts the pages that have
    /// been on screen long enough. Returns the session when it has been idle
    /// for so long that it is over.
    pub fn step(&mut self, db: &Db, input: TickInput, now: impl FnOnce() -> String) -> Option<MangaSession> {
        let Some(open) = self.open.as_mut() else { return self.finish() };
        let away = input.afk_secs > 0 && input.idle_secs >= input.afk_secs;
        let unfocused = !input.focused && input.pause_on_unfocus;
        let present = !unfocused && !away;
        self.present = present;
        // Turning the automatic start on is asking for it: a clock the user
        // had stopped is no longer held back.
        if input.auto_start && !self.auto_start {
            self.stopped = false;
        }
        self.auto_start = input.auto_start;
        self.auto_start_delay = input.auto_start_delay;
        // Started by itself after enough reading, unless the user stopped it.
        if !self.armed && !self.stopped && input.auto_start && present {
            self.warmup += input.elapsed;
            if self.warmup >= input.auto_start_delay {
                self.armed = true;
                self.warmup = Duration::ZERO;
            }
        } else if !self.armed && !present {
            self.warmup = Duration::ZERO;
        }
        let counting = self.armed && present;
        self.pause = if counting {
            None
        } else if self.stopped {
            Some(MangaPause::Stopped)
        } else if unfocused {
            Some(MangaPause::Unfocused)
        } else if away {
            Some(MangaPause::Idle)
        } else {
            None
        };
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
        session.pause = if counting { None } else { self.pause.or(Some(MangaPause::Stopped)) };
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
                let (credited, lines) = count_pages(db, open, &pages);
                session.pages_read += credited.pages;
                session.chars_read += credited.chars;
                self.unsaved_lines.extend(lines);
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

    /// Ends the session: it is the caller's to log. The clock goes back to
    /// waiting for an automatic start (or the menu).
    pub fn finish(&mut self) -> Option<MangaSession> {
        self.elapsed = Duration::ZERO;
        self.inactive = Duration::ZERO;
        self.armed = false;
        self.stopped = false;
        self.warmup = Duration::ZERO;
        self.session.take()
    }

    /// Started from the menu: it runs whenever the user is reading, whatever
    /// the automatic start says.
    pub fn start_clock(&mut self) {
        self.armed = true;
        self.stopped = false;
        self.warmup = Duration::ZERO;
        self.sync_session();
    }

    /// Stopped from the menu: it stays stopped until started again. The
    /// session stays open (and is logged after `manga_idle_log_minutes`).
    pub fn stop_clock(&mut self) {
        self.armed = false;
        self.stopped = true;
        self.warmup = Duration::ZERO;
        self.sync_session();
    }

    /// "Finish and log" from the menu: the session ends now. The next one
    /// starts like any other: by itself once the user reads on (after the
    /// delay), or from the menu when the automatic start is off.
    pub fn finish_now(&mut self) -> Option<MangaSession> {
        let session = self.finish();
        self.sync_session();
        session
    }

    /// The time edited by hand in the clock menu: the session's clock shows
    /// and logs `secs` from now on (capped at a day). With no session yet
    /// (the clock has not run), one starts with that time, so time read
    /// before opening the reader can be added. `false` with no volume open.
    pub fn set_elapsed(&mut self, secs: u64, now: impl FnOnce() -> String) -> bool {
        let Some(open) = self.open.as_ref() else { return false };
        if self.session.is_none() {
            self.session = Some(MangaSession::start(open, now()));
            self.inactive = Duration::ZERO;
        }
        self.elapsed = Duration::ZERO;
        if let Some(session) = self.session.as_mut() {
            session.elapsed_secs = secs.min(MAX_EDITED_SECS);
        }
        self.sync_session();
        // A time typed by hand always goes out, however close it is to the
        // last one sent (`changed` holds back a clock that only advanced).
        self.emitted = None;
        true
    }

    /// The session's running/paused state after a menu action, without
    /// waiting for the next tick.
    fn sync_session(&mut self) {
        let counting = self.armed && self.present;
        self.pause = if counting {
            None
        } else if self.stopped {
            Some(MangaPause::Stopped)
        } else {
            self.pause.filter(|pause| *pause != MangaPause::Stopped)
        };
        if let Some(session) = self.session.as_mut() {
            session.counting = counting;
            session.pause = if counting { None } else { self.pause.or(Some(MangaPause::Stopped)) };
        }
    }

    /// What the clock is doing, while a volume is open.
    pub fn clock(&self) -> Option<MangaClock> {
        let open = self.open.as_ref()?;
        let status = if self.armed && self.present {
            ClockStatus::Running
        } else if self.armed {
            ClockStatus::Paused
        } else if self.stopped {
            ClockStatus::Stopped
        } else if self.auto_start {
            ClockStatus::Waiting
        } else {
            ClockStatus::Off
        };
        let starts_in_secs = (status == ClockStatus::Waiting && !self.auto_start_delay.is_zero())
            .then(|| self.auto_start_delay.saturating_sub(self.warmup).as_secs_f64().ceil() as u64);
        Some(MangaClock {
            volume_id: open.volume_id.clone(),
            status,
            pause: if status == ClockStatus::Running { None } else { self.pause },
            starts_in_secs,
        })
    }

    /// The clock, if it changed since the last time this was asked.
    fn clock_changed(&mut self) -> Option<Option<MangaClock>> {
        let clock = self.clock();
        if self.clock_emitted.as_ref() == Some(&clock) {
            return None;
        }
        self.clock_emitted = Some(clock.clone());
        Some(clock)
    }

    /// The session, if it changed since the last time this was asked. One
    /// whose clocks (`elapsed_secs`, `inactive_secs`) only went forward, by
    /// less than `SESSION_RESYNC_SECS`, has not changed: the windows count
    /// a running clock on their own and are set right every so often.
    fn changed(&mut self) -> Option<Option<MangaSession>> {
        if self.emitted.as_ref() == Some(&self.session) {
            return None;
        }
        if let (Some(Some(last)), Some(now)) = (&self.emitted, &self.session) {
            if only_clocks_advanced(last, now) {
                return None;
            }
        }
        self.emitted = Some(self.session.clone());
        self.emitted.clone()
    }
}

/// `now` is `last` with its clocks a little further on, and nothing else
/// different.
fn only_clocks_advanced(last: &MangaSession, now: &MangaSession) -> bool {
    let near = |before: u64, after: u64| after >= before && after - before < SESSION_RESYNC_SECS;
    if !near(last.elapsed_secs, now.elapsed_secs) || !near(last.inactive_secs, now.inactive_secs) {
        return false;
    }
    let same = MangaSession { elapsed_secs: last.elapsed_secs, inactive_secs: last.inactive_secs, ..now.clone() };
    same == *last
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

/// Where the lines of a series go in `text_lines`.
pub fn content_id(media_id: Option<&str>, series_id: &str) -> String {
    match media_id.filter(|id| !id.trim().is_empty()) {
        Some(id) => id.to_string(),
        None => format!("manga:{series_id}"),
    }
}

/// The line id of a block: the same block is always the same line.
pub fn line_id(volume_id: &str, page: i64, block: usize) -> String {
    format!("manga:{volume_id}:{page}:{block}")
}

/// `(volume, page, block)` of a manga line id, or `None` for any other line.
pub fn parse_line_id(id: &str) -> Option<(&str, i64, usize)> {
    let mut parts = id.strip_prefix("manga:")?.split(':');
    let (volume, page, block) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || !super::protocol::is_opaque_id(volume) {
        return None;
    }
    Some((volume, page.parse().ok()?, block.parse().ok()?))
}

/// The block as a line of text, if it has Japanese in it.
fn block_line(volume_id: &str, page: i64, index: usize, block: &super::mokuro::MokuroBlock, content_id: &str) -> Option<crate::db::TextLine> {
    let text = block.lines.concat();
    let chars = crate::texthooker::count_japanese(&text);
    (chars > 0).then(|| crate::db::TextLine {
        id: line_id(volume_id, page, index),
        content_id: content_id.to_string(),
        text: text.trim().to_string(),
        chars,
        created_at: crate::db::now_iso(),
        // Local context only: the texthooker API never sees manga pages.
        synced: true,
    })
}

/// The blocks of a page as lines (the ones with Japanese in them).
fn page_lines(volume_id: &str, page: i64, text: Option<&MokuroPage>, content_id: &str) -> Vec<crate::db::TextLine> {
    let Some(text) = text else { return Vec::new() };
    text.blocks
        .iter()
        .enumerate()
        .filter_map(|(index, block)| block_line(volume_id, page, index, block, content_id))
        .collect()
}

/// Stores lines. Ones already stored stay as they are.
pub fn store_lines(db: &Db, lines: &[crate::db::TextLine]) {
    for line in lines {
        if let Err(error) = db.insert_line(line) {
            log::warn!("manga line {} not stored: {error}", line.id);
        }
    }
}

/// Stores the blocks of a page as lines. Ones already stored stay as they are.
pub fn insert_page_lines(db: &Db, volume_id: &str, page: i64, text: Option<&MokuroPage>, content_id: &str) {
    store_lines(db, &page_lines(volume_id, page, text, content_id));
}

/// Records `pages` of the open volume as read, and returns what they add
/// with the lines of their blocks, for the caller to store (`store_lines`).
/// Pages read before add nothing.
pub fn count_pages(db: &Db, open: &OpenVolume, pages: &[i64]) -> (Credited, Vec<crate::db::TextLine>) {
    let mut credited = Credited::default();
    let mut lines = Vec::new();
    let content = content_id(open.media_id.as_deref(), &open.series_id);
    for &page in pages {
        let chars = page_chars(open.page_text(page));
        match db.mark_manga_page_read(&open.volume_id, page, chars) {
            Ok(true) => {
                credited.pages += 1;
                credited.chars += chars;
                lines.extend(page_lines(&open.volume_id, page, open.page_text(page), &content));
            }
            Ok(false) => {}
            Err(error) => log::warn!("manga page {}/{page} not recorded: {error}", open.volume_id),
        }
    }
    (credited, lines)
}

/// Records `pages` of the open volume as read, and their blocks as lines.
/// Pages read before add nothing.
#[cfg(test)]
pub fn credit_pages(db: &Db, open: &OpenVolume, pages: &[i64]) -> Credited {
    let (credited, lines) = count_pages(db, open, pages);
    store_lines(db, &lines);
    credited
}

/// A page of a volume got new text: the OCR read it, or the user corrected
/// it. The reader's session counts with it from now on, and a page it already
/// counted without text (read before the OCR finished) gets its characters
/// and lines now, once.
pub fn page_text_changed(app: &AppHandle, volume_id: &str, page: i64, text: Option<MokuroPage>) {
    let state = app.state::<AppState>();
    let mut tracking = state.manga.tracking.lock().unwrap();
    let Tracking { open, session, .. } = &mut *tracking;
    let Some(open) = open.as_mut().filter(|open| open.volume_id == volume_id) else { return };
    let Some(slot) = usize::try_from(page).ok().and_then(|page| Arc::make_mut(&mut open.text).get_mut(page)) else { return };
    *slot = text;
    let chars = page_chars(open.page_text(page));
    let Some(session) = session.as_mut().filter(|session| session.volume_id == volume_id) else { return };
    session.has_text |= open.page_text(page).is_some();
    if chars > 0 && state.db.fill_manga_page_chars(volume_id, page, chars).unwrap_or(false) {
        session.chars_read += chars;
        let content = content_id(open.media_id.as_deref(), &open.series_id);
        insert_page_lines(&state.db, volume_id, page, open.page_text(page), &content);
    }
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

// ── mining ─────────────────────────────────────────────────────────────

/// The block a line id names, with what it takes to store and title it.
struct BlockRef {
    line: crate::db::TextLine,
    title: String,
    bbox: [f64; 4],
    text_size: (f64, f64),
    location: super::store::VolumeLocation,
    page: i64,
}

/// Finds a block by line id, using the open volume's text when it is that
/// volume and reading it from disk otherwise. Blocking: call it off the
/// async runtime.
fn find_block(app: &AppHandle, volume_id: &str, page: i64, block: usize) -> Result<BlockRef, String> {
    let state = app.state::<AppState>();
    let db = &state.db;
    let reader = super::reader::volume(db, volume_id).map_err(|e| e.to_string())?.ok_or("errors.lineNotFound")?;
    let location = db.manga_volume_location(volume_id).map_err(|e| e.to_string())?.ok_or("errors.lineNotFound")?;
    let cached = {
        let tracking = state.manga.tracking.lock().unwrap();
        tracking.open.as_ref().filter(|open| open.volume_id == volume_id).map(|open| open.text.clone())
    };
    let text = match cached {
        Some(text) => text,
        None => Arc::new(super::mokuro::volume_text(db, state.manga.remote().as_deref(), volume_id)?),
    };
    let page_text = usize::try_from(page).ok().and_then(|index| text.get(index)).and_then(Option::as_ref).ok_or("errors.lineNotFound")?;
    let found = page_text.blocks.get(block).ok_or("errors.lineNotFound")?;
    let content = content_id(reader.series.media_id.as_deref(), &reader.series.id);
    let line = block_line(volume_id, page, block, found, &content).ok_or("errors.noJapanese")?;
    let title = reader.series.media_title.clone().filter(|title| !title.trim().is_empty()).unwrap_or(reader.series.title.clone());
    Ok(BlockRef { line, title, bbox: found.bbox, text_size: (page_text.img_width, page_text.img_height), location, page })
}

/// The stored line for a manga line id, storing it first if its page has
/// not been read yet: a word can be looked up before the page counts.
/// Blocking. `None` for a line id that is not a manga block.
pub fn ensure_line(app: &AppHandle, line_id: &str) -> Option<Result<(crate::db::TextLine, String), String>> {
    let (volume_id, page, block) = parse_line_id(line_id)?;
    Some(find_block(app, volume_id, page, block).and_then(|found| {
        let db = &app.state::<AppState>().db;
        db.insert_line(&found.line).map_err(|e| e.to_string())?;
        let line = db.line_by_id(&found.line.id).map_err(|e| e.to_string())?.ok_or("errors.lineNotFound")?;
        Ok((line, found.title))
    }))
}

/// Mines a block: its sentence goes to the mined list like a texthooker
/// line, and the panel around it becomes the card's picture
/// (`app_data_dir/mined/<id>.png`, through `set_mined_screenshot`). A page
/// that cannot be cropped still mines the sentence. Blocking.
pub fn mine_block(app: &AppHandle, volume_id: &str, page: i64, block: usize) -> Result<crate::db::MinedSentence, String> {
    let found = find_block(app, volume_id, page, block)?;
    let state = app.state::<AppState>();
    let db = &state.db;
    db.insert_line(&found.line).map_err(|e| e.to_string())?;
    let line = db.line_by_id(&found.line.id).map_err(|e| e.to_string())?.ok_or("errors.lineNotFound")?;
    let sentence = db.mine_line(&line, Some(&found.title)).map_err(|e| e.to_string())?;
    let _ = app.emit(crate::mining::EVT_MINED, &sentence);
    if sentence.screenshot_path.is_some() {
        return Ok(sentence);
    }
    let picture = usize::try_from(found.page)
        .ok()
        .and_then(|index| super::protocol::page_bytes(state.manga.remote().as_deref(), &found.location, index))
        .ok_or_else(|| "the page could not be read".to_string())
        .and_then(|bytes| super::crop::crop_png(&bytes, found.bbox, found.text_size));
    let saved = picture.and_then(|png| {
        let folder = app.path().app_data_dir().map_err(|e| e.to_string())?.join("mined");
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let path = folder.join(format!("{}.png", sentence.id));
        std::fs::write(&path, png).map_err(|e| e.to_string())?;
        if let Err(error) = db.set_mined_screenshot(&sentence.id, &path.to_string_lossy()) {
            let _ = std::fs::remove_file(&path);
            return Err(error.to_string());
        }
        Ok(())
    });
    match saved {
        Ok(()) => {
            let _ = app.emit(crate::mining::EVT_MINED, json!({ "id": sentence.id }));
            Ok(db.mined_by_id(&sentence.id).map_err(|e| e.to_string())?.unwrap_or(sentence))
        }
        Err(error) => {
            log::warn!("manga crop for {volume_id}/{page}/{block} failed: {error}");
            Ok(sentence)
        }
    }
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
        afk_secs: settings.manga_afk_timeout_secs.unwrap_or(settings.afk_timeout_secs),
        dwell: dwell_threshold(settings),
        idle_log: Duration::from_secs(settings.manga_idle_log_minutes.saturating_mul(60)),
        pause_on_unfocus: settings.manga_pause_on_unfocus,
        auto_start: settings.manga_auto_start,
        auto_start_delay: Duration::from_secs(settings.manga_auto_start_delay_secs.min(3600)),
    };
    let (session, changed, clock, finished, lines) = {
        let mut tracking = state.manga.tracking.lock().unwrap();
        let finished = tracking.step(&state.db, input, crate::db::now_iso);
        let lines = std::mem::take(&mut tracking.unsaved_lines);
        (tracking.session.clone(), tracking.changed(), tracking.clock_changed(), finished, lines)
    };
    // The page is already counted; its lines (one insert per block) are
    // stored off the tick.
    if !lines.is_empty() {
        let db = state.db.clone();
        tauri::async_runtime::spawn_blocking(move || store_lines(&db, &lines));
    }
    if let Some(finished) = finished {
        log_session(app, &finished, settings);
    }
    if let Some(changed) = changed {
        let _ = app.emit(SESSION_EVENT, &changed);
    }
    if let Some(clock) = clock {
        let _ = app.emit(CLOCK_EVENT, &clock);
    }
    session
}

/// To every window: the reader's clock (or `null` with no volume open)
/// whenever it changes.
pub const CLOCK_EVENT: &str = "manga://clock";

/// The most a session's time can be edited to: a day.
pub const MAX_EDITED_SECS: u64 = 24 * 60 * 60;

/// The session's time, edited by hand in the clock menu.
pub fn set_elapsed(app: &AppHandle, secs: u64) -> Result<(), String> {
    let state = app.state::<AppState>();
    let (changed, clock) = {
        let mut tracking = state.manga.tracking.lock().unwrap();
        if !tracking.set_elapsed(secs, crate::db::now_iso) {
            return Err("no volume open".into());
        }
        (tracking.changed(), tracking.clock_changed())
    };
    if let Some(changed) = changed {
        let _ = app.emit(SESSION_EVENT, &changed);
    }
    if let Some(clock) = clock {
        let _ = app.emit(CLOCK_EVENT, &clock);
    }
    Ok(())
}

/// What a clock menu action asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClockAction {
    Start,
    Stop,
    /// End the session now and log it, then stay stopped.
    Finish,
}

/// Runs a clock menu action and tells the windows at once, without waiting
/// for the next tick. A finished session is logged like any other.
pub fn clock_action(app: &AppHandle, action: ClockAction) -> Option<MangaClock> {
    let state = app.state::<AppState>();
    let (changed, clock, finished) = {
        let mut tracking = state.manga.tracking.lock().unwrap();
        let finished = match action {
            ClockAction::Start => {
                tracking.start_clock();
                None
            }
            ClockAction::Stop => {
                tracking.stop_clock();
                None
            }
            ClockAction::Finish => tracking.finish_now(),
        };
        (tracking.changed(), tracking.clock_changed(), finished)
    };
    if let Some(session) = finished {
        log_later(app, session);
    }
    if let Some(changed) = changed {
        let _ = app.emit(SESSION_EVENT, &changed);
    }
    if let Some(clock) = &clock {
        let _ = app.emit(CLOCK_EVENT, clock);
    }
    clock_state(app)
}

/// The clock now, for a window that opens (or reloads) between events.
pub fn clock_state(app: &AppHandle) -> Option<MangaClock> {
    app.state::<AppState>().manga.tracking.lock().unwrap().clock()
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
        TickInput {
            elapsed,
            focused,
            idle_secs,
            afk_secs: 120,
            dwell: 3 * SECOND,
            idle_log: 600 * SECOND,
            pause_on_unfocus: true,
            auto_start: true,
            auto_start_delay: Duration::ZERO,
        }
    }

    fn status(tracking: &Tracking) -> ClockStatus {
        tracking.clock().unwrap().status
    }

    #[test]
    fn the_clock_starts_by_itself_after_the_delay_without_counting_it() {
        let (db, path) = temp_db();
        let mut tracking = reading(vec![None]);
        let delayed = TickInput { auto_start_delay: 5 * SECOND, ..input(SECOND, true, 0) };
        for _ in 0..3 {
            tracking.step(&db, delayed, || "t".into());
        }
        assert!(tracking.session.is_none());
        assert_eq!(tracking.clock().unwrap().starts_in_secs, Some(2));
        assert_eq!(status(&tracking), ClockStatus::Waiting);
        // Looking away resets the countdown: it is reading time.
        tracking.step(&db, TickInput { focused: false, ..delayed }, || "t".into());
        assert_eq!(tracking.clock().unwrap().starts_in_secs, Some(5));
        for _ in 0..6 {
            tracking.step(&db, delayed, || "t".into());
        }
        // Five ticks to start (the fifth counts), one more running.
        assert_eq!(tracking.session.as_ref().unwrap().elapsed_secs, 2);
        assert_eq!(status(&tracking), ClockStatus::Running);
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn without_automatic_start_only_the_menu_starts_it() {
        let (db, path) = temp_db();
        let mut tracking = reading(vec![None]);
        let manual = TickInput { auto_start: false, ..input(SECOND, true, 0) };
        tracking.step(&db, manual, || "t".into());
        assert!(tracking.session.is_none());
        assert_eq!(status(&tracking), ClockStatus::Off);
        tracking.start_clock();
        assert_eq!(status(&tracking), ClockStatus::Running);
        tracking.step(&db, manual, || "t".into());
        assert_eq!(tracking.session.as_ref().unwrap().elapsed_secs, 1);
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_stopped_clock_stays_stopped_until_started_again() {
        let (db, path) = temp_db();
        let mut tracking = reading(vec![None]);
        tracking.step(&db, input(SECOND, true, 0), || "t".into());
        tracking.stop_clock();
        let session = tracking.session.clone().unwrap();
        assert_eq!((session.counting, session.pause), (false, Some(MangaPause::Stopped)));
        for _ in 0..3 {
            tracking.step(&db, input(SECOND, true, 0), || "t".into());
        }
        assert_eq!(tracking.session.as_ref().unwrap().elapsed_secs, 1, "reading does not start it");
        assert_eq!(status(&tracking), ClockStatus::Stopped);
        tracking.start_clock();
        tracking.step(&db, input(SECOND, true, 0), || "t".into());
        assert_eq!(tracking.session.as_ref().unwrap().elapsed_secs, 2);
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn after_finishing_from_the_menu_the_next_session_starts_like_any_other() {
        let (db, path) = temp_db();
        let mut tracking = reading(vec![None]);
        tracking.step(&db, input(SECOND, true, 0), || "t1".into());
        assert_eq!(tracking.finish_now().unwrap().elapsed_secs, 1);
        assert!(tracking.session.is_none());
        // Automatic start: reading on starts the next one.
        tracking.step(&db, input(SECOND, true, 0), || "t2".into());
        let session = tracking.session.as_ref().unwrap();
        assert_eq!((session.started_at.as_str(), session.elapsed_secs), ("t2", 1));
        // Without it, the next one waits for the menu.
        tracking.finish_now();
        let manual = TickInput { auto_start: false, ..input(SECOND, true, 0) };
        tracking.step(&db, manual, || "t3".into());
        assert!(tracking.session.is_none());
        assert_eq!(status(&tracking), ClockStatus::Off);
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn the_time_can_be_edited_and_keeps_counting_from_there() {
        let (db, path) = temp_db();
        let mut tracking = reading(vec![None]);
        assert!(!Tracking::default().set_elapsed(60, || "t".into()), "no volume, nothing to edit");
        // Before the clock has run: a session starts with that time.
        let manual = TickInput { auto_start: false, ..input(SECOND, true, 0) };
        tracking.step(&db, manual, || "t".into());
        assert!(tracking.set_elapsed(600, || "t0".into()));
        let session = tracking.session.clone().unwrap();
        assert_eq!((session.elapsed_secs, session.counting, session.started_at.as_str()), (600, false, "t0"));
        tracking.start_clock();
        tracking.step(&db, manual, || "t".into());
        assert_eq!(tracking.session.as_ref().unwrap().elapsed_secs, 601);
        // Down too, and never past a day.
        tracking.set_elapsed(5, || "t".into());
        assert_eq!(tracking.session.as_ref().unwrap().elapsed_secs, 5);
        tracking.set_elapsed(u64::MAX, || "t".into());
        assert_eq!(tracking.session.as_ref().unwrap().elapsed_secs, MAX_EDITED_SECS);
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn turning_the_automatic_start_on_wakes_a_stopped_clock() {
        let (db, path) = temp_db();
        let mut tracking = reading(vec![None]);
        let manual = TickInput { auto_start: false, ..input(SECOND, true, 0) };
        tracking.step(&db, manual, || "t".into());
        tracking.start_clock();
        tracking.step(&db, manual, || "t".into());
        tracking.stop_clock();
        tracking.step(&db, manual, || "t".into());
        assert_eq!(status(&tracking), ClockStatus::Stopped);
        tracking.step(&db, input(SECOND, true, 0), || "t".into());
        assert_eq!(status(&tracking), ClockStatus::Running);
        // While it stays on, a stop holds.
        tracking.stop_clock();
        tracking.step(&db, input(SECOND, true, 0), || "t".into());
        assert_eq!(status(&tracking), ClockStatus::Stopped);
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn the_focus_can_be_left_out_and_a_pause_resumes_by_itself() {
        let (db, path) = temp_db();
        let mut tracking = reading(vec![None]);
        let anywhere = TickInput { pause_on_unfocus: false, ..input(SECOND, false, 0) };
        tracking.step(&db, anywhere, || "t".into());
        assert_eq!(tracking.session.as_ref().unwrap().elapsed_secs, 1, "counts in the background");
        tracking.step(&db, TickInput { idle_secs: 130, ..anywhere }, || "t".into());
        assert_eq!(status(&tracking), ClockStatus::Paused);
        assert_eq!(tracking.clock().unwrap().pause, Some(MangaPause::Idle));
        tracking.step(&db, anywhere, || "t".into());
        assert_eq!(status(&tracking), ClockStatus::Running);
        assert_eq!(tracking.session.as_ref().unwrap().elapsed_secs, 2);
        drop(db);
        let _ = std::fs::remove_file(path);
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
    fn a_clock_that_only_runs_is_sent_every_so_often_and_anything_else_at_once() {
        let (db, path) = temp_db();
        let mut tracking = reading(vec![None, None]);
        tracking.step(&db, input(SECOND, true, 0), || "t".into());
        assert!(tracking.changed().unwrap().is_some());
        for _ in 1..SESSION_RESYNC_SECS {
            tracking.step(&db, input(SECOND, true, 0), || "t".into());
            assert_eq!(tracking.changed(), None, "the windows count it themselves");
        }
        tracking.step(&db, input(SECOND, true, 0), || "t".into());
        let sent = tracking.changed().unwrap().unwrap();
        assert_eq!(sent.elapsed_secs, 1 + SESSION_RESYNC_SECS, "set right after a while");
        // A pause goes out at once…
        tracking.step(&db, input(SECOND, false, 0), || "t".into());
        assert!(!tracking.changed().unwrap().unwrap().counting);
        // …and so does a page on screen, or a time typed by hand.
        tracking.step(&db, input(SECOND, true, 0), || "t".into());
        tracking.changed();
        tracking.open.as_mut().unwrap().show(&[1]);
        tracking.step(&db, input(SECOND, true, 0), || "t".into());
        assert_eq!(tracking.changed().unwrap().unwrap().page, 1);
        let at = tracking.session.as_ref().unwrap().elapsed_secs;
        tracking.set_elapsed(at + 2, || "t".into());
        assert_eq!(tracking.changed().unwrap().unwrap().elapsed_secs, at + 2);
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_credited_page_counts_at_once_and_leaves_its_lines_to_be_stored() {
        let (db, path) = temp_db();
        let mut tracking = reading(vec![Some(page(&["あいう", "ABC"]))]);
        tracking.open.as_mut().unwrap().show(&[0]);
        for _ in 0..3 {
            tracking.step(&db, input(SECOND, true, 0), || "t".into());
        }
        let session = tracking.session.clone().unwrap();
        assert_eq!((session.pages_read, session.chars_read), (1, 3));
        assert_eq!(db.manga_read_totals("v1").unwrap(), (1, 3));
        let lines = std::mem::take(&mut tracking.unsaved_lines);
        assert_eq!(lines.iter().map(|line| line.id.as_str()).collect::<Vec<_>>(), ["manga:v1:0:0"]);
        assert!(db.line_by_id("manga:v1:0:0").unwrap().is_none(), "not stored inside the step");
        store_lines(&db, &lines);
        assert_eq!(db.line_by_id("manga:v1:0:0").unwrap().unwrap().text, "あいう");
        // Read again: nothing more to count or store.
        tracking.step(&db, input(10 * SECOND, true, 0), || "t".into());
        assert!(tracking.unsaved_lines.is_empty());
        assert_eq!(tracking.session.as_ref().unwrap().pages_read, 1);
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
    fn a_page_that_counts_stores_its_blocks_as_lines_once() {
        let (db, path) = temp_db();
        let mut tracking = reading(vec![Some(page(&["よつばと!", "ABC", "とーちゃん"]))]);
        tracking.open.as_mut().unwrap().show(&[0]);
        for _ in 0..3 {
            tracking.step(&db, input(SECOND, true, 0), || "t".into());
        }
        // As the tick does, after the step.
        store_lines(&db, &std::mem::take(&mut tracking.unsaved_lines));
        let lines = db.lines_for("manga:s1", 10).unwrap();
        let ids: Vec<&str> = lines.iter().map(|line| line.id.as_str()).collect();
        assert_eq!(ids, ["manga:v1:0:0", "manga:v1:0:2"], "a block without Japanese is not a line");
        assert!(lines.iter().all(|line| line.synced), "never pushed to the texthooker API");
        assert_eq!(lines[0].chars, 4);
        // Shown again: no new page, no new lines.
        tracking.open.as_mut().unwrap().show(&[]);
        tracking.open.as_mut().unwrap().show(&[0]);
        for _ in 0..3 {
            tracking.step(&db, input(SECOND, true, 0), || "t".into());
        }
        assert!(tracking.unsaved_lines.is_empty());
        assert_eq!(db.lines_for("manga:s1", 10).unwrap().len(), 2);
        assert_eq!(tracking.session.as_ref().unwrap().pages_read, 1);
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn lines_go_to_the_linked_media_or_to_the_series() {
        assert_eq!(content_id(Some("30104"), "s1"), "30104");
        assert_eq!(content_id(None, "s1"), "manga:s1");
        assert_eq!(content_id(Some(" "), "s1"), "manga:s1");
    }

    #[test]
    fn line_ids_name_their_block_and_nothing_else() {
        let id = line_id("0b5c7d0e-0000-4000-8000-000000000001", 12, 3);
        assert_eq!(parse_line_id(&id), Some(("0b5c7d0e-0000-4000-8000-000000000001", 12, 3)));
        assert_eq!(parse_line_id("3f2a-uuid-of-a-hooker-line"), None);
        assert_eq!(parse_line_id("manga:../etc:1:2"), None);
        assert_eq!(parse_line_id("manga:v1:1:2:3"), None);
        assert_eq!(parse_line_id("manga:v1:x:2"), None);
    }

    #[test]
    fn nothing_on_screen_counts_nothing() {
        let mut open = open_volume(vec![None]);
        open.show(&[]);
        assert_eq!(open.dwell(60 * SECOND, 3 * SECOND), None);
    }
}
