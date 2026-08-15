//! "Streak at risk" reminders.
//!
//! The feature lives or dies on not being annoying: at most one notification per
//! day, only inside the window before the day ends, and always re-checked
//! against the server right before firing — the log may have come from the web
//! or the phone a minute ago.
//!
//! The day is the **user's** day. The backend derives a `YYYY-MM-DD` key with
//! `Intl` in `settings.timezone` (`Backend/src/services/streaks.ts`), so the same
//! key has to come out of here or we would warn the wrong person on the wrong
//! day. `lastStreakDate` is a *UTC midnight anchor* for one of those keys, which
//! is why it is read back in UTC and never in the user's timezone — the backend
//! does exactly that in `getDayKeyFromUTCDate`.
//!
//! Note that a log with `unknownDate: true` is excluded from the streak, so
//! "the app sent a log today" proves nothing: only `lastStreakDate` does.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use chrono::offset::LocalResult;
use chrono::{DateTime, Duration as ChronoDuration, TimeZone, Timelike, Utc};
use chrono_tz::{Tz, UTC};
use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::api::ApiError;
use crate::state::AppState;

pub const EVT_REMINDER: &str = "streak://reminder";
pub const EVT_OPEN_LOG: &str = "streak://open-log";
pub const EVT_AUTH_REQUIRED: &str = "streak://auth-required";

/// Day key of the last reminder we showed. One notification per day, and it has
/// to survive a restart, so it goes to SQLite rather than memory.
const KV_NOTIFIED: &str = "streak:notified_day";

const FALLBACK_TZ: &str = "UTC";
/// How long the fixed-hour reminder stays valid. Without it, an app that was
/// closed at that hour would shout the moment it opens, hours late.
const FIXED_GRACE_MINUTES: i64 = 10;
/// Poll budget: inside the risk window and outside it.
const POLL_IN_WINDOW: Duration = Duration::from_secs(5 * 60);
const POLL_IDLE: Duration = Duration::from_secs(60 * 60);
/// A cached answer younger than this counts as "just checked".
const FRESH: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Serialize)]
pub struct StreakStatus {
    #[serde(rename = "currentStreak")]
    pub current_streak: i64,
    /// Day key of `lastStreakDate`, or `None` when the user never logged.
    #[serde(rename = "lastDayKey")]
    pub last_day_key: Option<String>,
    #[serde(rename = "todayKey")]
    pub today_key: String,
    pub timezone: String,
    /// A streak is running and today has nothing on it yet.
    #[serde(rename = "atRisk")]
    pub at_risk: bool,
    /// Minutes until the user's day ends.
    #[serde(rename = "minutesLeft")]
    pub minutes_left: i64,
}

#[derive(Default)]
pub struct StreakCache {
    pub status: Option<StreakStatus>,
    pub fetched_at: Option<Instant>,
}

// ── day maths ──────────────────────────────────────────────────────────

pub fn parse_tz(name: &str) -> Tz {
    name.trim().parse::<Tz>().unwrap_or(UTC)
}

/// `YYYY-MM-DD` in the user's timezone — the backend's `getUserDayKey`.
pub fn day_key(instant: DateTime<Utc>, tz: Tz) -> String {
    instant.with_timezone(&tz).format("%Y-%m-%d").to_string()
}

/// The day key `lastStreakDate` anchors. Read in UTC on purpose: the backend
/// stores UTC midnight of the key and reads it back the same way, so converting
/// to the user's timezone would move the day for everyone west of Greenwich.
pub fn day_key_from_anchor(value: &str) -> Option<String> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(value) {
        return Some(parsed.with_timezone(&Utc).format("%Y-%m-%d").to_string());
    }
    // Tolerate a bare date, in case the field is ever serialised that way.
    let head = value.get(..10)?;
    chrono::NaiveDate::parse_from_str(head, "%Y-%m-%d")
        .ok()
        .map(|d| d.format("%Y-%m-%d").to_string())
}

/// Minutes until midnight in the user's timezone.
pub fn minutes_to_midnight(now: DateTime<Utc>, tz: Tz) -> i64 {
    let local = now.with_timezone(&tz);
    let Some(next_day) = local.date_naive().succ_opt() else {
        return 0;
    };
    let Some(mut candidate) = next_day.and_hms_opt(0, 0, 0) else {
        return 0;
    };
    // DST can delete local midnight outright (it happens in Chile, Brazil,
    // Iran…). When it does, the day starts at the first wall clock that exists.
    for _ in 0..180 {
        match tz.from_local_datetime(&candidate) {
            LocalResult::Single(dt) | LocalResult::Ambiguous(dt, _) => {
                return (dt.with_timezone(&Utc) - now).num_minutes();
            }
            LocalResult::None => candidate += ChronoDuration::minutes(1),
        }
    }
    0
}

pub fn in_risk_window(minutes_left: i64, hours: u8) -> bool {
    minutes_left <= i64::from(hours) * 60
}

fn parse_hhmm(value: &str) -> Option<(i64, i64)> {
    let (h, m) = value.trim().split_once(':')?;
    let h: i64 = h.trim().parse().ok()?;
    let m: i64 = m.trim().parse().ok()?;
    if (0..24).contains(&h) && (0..60).contains(&m) {
        Some((h, m))
    } else {
        None
    }
}

/// `HH:MM` in the user's timezone, with a grace period so the reminder is not
/// fired hours late by an app that was closed at that time.
pub fn fixed_time_hit(now: DateTime<Utc>, tz: Tz, at: &str) -> bool {
    let Some((hour, minute)) = parse_hhmm(at) else {
        return false;
    };
    let local = now.with_timezone(&tz);
    let current = i64::from(local.hour()) * 60 + i64::from(local.minute());
    let target = hour * 60 + minute;
    (0..FIXED_GRACE_MINUTES).contains(&(current - target))
}

/// Build the status out of an `/api/auth/verify` body.
pub fn status_from_verify(body: &Value, now: DateTime<Utc>) -> StreakStatus {
    let user = body.get("user").unwrap_or(body);
    let stats = user.get("stats");
    let timezone = user
        .get("settings")
        .and_then(|s| s.get("timezone"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(FALLBACK_TZ)
        .to_string();
    let tz = parse_tz(&timezone);

    let current_streak = stats
        .and_then(|s| s.get("currentStreak"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let last_day_key = stats
        .and_then(|s| s.get("lastStreakDate"))
        .and_then(Value::as_str)
        .and_then(day_key_from_anchor);
    let today_key = day_key(now, tz);
    let at_risk = current_streak > 0 && last_day_key.as_deref() != Some(today_key.as_str());

    StreakStatus {
        current_streak,
        last_day_key,
        today_key,
        timezone,
        at_risk,
        minutes_left: minutes_to_midnight(now, tz),
    }
}

// ── fetching ───────────────────────────────────────────────────────────

/// Return the streak status, hitting the network only when the cached answer is
/// older than `max_age`. Every caller goes through here, so the polling budget
/// holds no matter who asks.
pub async fn refresh(app: &AppHandle, max_age: Duration) -> Result<Option<StreakStatus>, ApiError> {
    let state = app.state::<AppState>();
    if !state.settings_snapshot().await.is_authenticated() {
        return Ok(None);
    }

    let (cached, fetched_at) = {
        let cache = state.streak.read().await;
        (cache.status.clone(), cache.fetched_at)
    };
    if let (Some(status), Some(at)) = (&cached, fetched_at) {
        if at.elapsed() < max_age {
            return Ok(Some(status.clone()));
        }
    }

    let client = state.client().await;
    match client.verify().await {
        Ok(body) => {
            let status = status_from_verify(&body, Utc::now());
            let mut cache = state.streak.write().await;
            cache.status = Some(status.clone());
            cache.fetched_at = Some(Instant::now());
            Ok(Some(status))
        }
        Err(err) => {
            // A revoked key never fixes itself: stop the loop and send the user
            // to re-authenticate instead of retrying for the rest of the day.
            if matches!(&err, ApiError::Http { status, .. } if *status == 401) {
                state.streak_auth_failed.store(true, Ordering::Release);
                let _ = app.emit(EVT_AUTH_REQUIRED, json!({ "reason": err.to_string() }));
            }
            Err(err)
        }
    }
}

/// How stale an answer may be right now: tight inside the risk window, lazy
/// outside it. Every caller shares it, so opening the dashboard doesn't turn
/// into an extra request every five minutes at three in the afternoon.
pub async fn budget(app: &AppHandle) -> Duration {
    let state = app.state::<AppState>();
    let hours = state.settings_snapshot().await.streak_reminder_hours;
    let now = Utc::now();
    let cache = state.streak.read().await;
    let in_window = cache.status.as_ref().is_some_and(|status| {
        let tz = parse_tz(&status.timezone);
        in_risk_window(minutes_to_midnight(now, tz), hours)
    });
    if in_window {
        POLL_IN_WINDOW
    } else {
        POLL_IDLE
    }
}

// ── the loop ───────────────────────────────────────────────────────────

pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Let the app finish starting before the first request.
        tokio::time::sleep(Duration::from_secs(15)).await;
        loop {
            tick(&app).await;
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });
}

async fn tick(app: &AppHandle) {
    let state = app.state::<AppState>();
    let settings = state.settings_snapshot().await;
    if !settings.notifications || !settings.streak_reminder || !settings.is_authenticated() {
        return;
    }
    if state.streak_auth_failed.load(Ordering::Acquire) {
        return;
    }

    let now = Utc::now();
    // Until the first answer arrives the timezone is unknown, so `budget` says
    // "outside the window" and the loop starts on the cheap cadence.
    let allowed_age = budget(app).await;
    let status = match refresh(app, allowed_age).await {
        Ok(Some(status)) => status,
        _ => return,
    };

    let tz = parse_tz(&status.timezone);
    let minutes_left = minutes_to_midnight(now, tz);
    let due = in_risk_window(minutes_left, settings.streak_reminder_hours)
        || fixed_time_hit(now, tz, &settings.streak_reminder_at);
    if !due || !status.at_risk {
        return;
    }
    // Only ever notify for today: an app that was closed through the window
    // never fires yesterday's reminder on the way in.
    if state.db.get_kv(KV_NOTIFIED).as_deref() == Some(status.today_key.as_str()) {
        return;
    }

    // Last look before making noise.
    let fresh = match refresh(app, FRESH).await {
        Ok(Some(fresh)) => fresh,
        _ => return,
    };
    if !fresh.at_risk {
        return;
    }

    let _ = state.db.set_kv(KV_NOTIFIED, &fresh.today_key);
    let _ = app.emit(EVT_REMINDER, &fresh);
    show(app, &settings.language, &fresh);
}

// ── the notification ───────────────────────────────────────────────────

fn show(app: &AppHandle, lang: &str, status: &StreakStatus) {
    let title = crate::i18n::streak_title(lang);
    let body = crate::i18n::streak_body(lang, status.current_streak, status.minutes_left);

    // On Windows the toast is built directly so it can carry a button and an
    // activation callback; the plugin's desktop path has neither.
    #[cfg(windows)]
    if show_windows(app, lang, title, &body) {
        return;
    }

    use tauri_plugin_notification::NotificationExt;
    let _ = app.notification().builder().title(title).body(body).show();
}

#[cfg(windows)]
fn show_windows(app: &AppHandle, lang: &str, title: &str, body: &str) -> bool {
    use tauri_winrt_notification::Toast;

    let handle = app.clone();
    let build = |app_id: &str| {
        let handle = handle.clone();
        Toast::new(app_id)
            .title(title)
            .text1(body)
            .add_button(crate::i18n::streak_action(lang), "log")
            .on_activated(move |_action| {
                open_log_form(&handle);
                Ok(())
            })
    };

    let identifier = app.config().identifier.clone();
    // An unregistered AppUserModelID makes WinRT refuse the toast (typical in
    // `tauri dev`, where no shortcut was installed); PowerShell's own id always
    // exists, and it is what notify-rust falls back to as well.
    build(&identifier)
        .show()
        .or_else(|_| build(Toast::POWERSHELL_APP_ID).show())
        .is_ok()
}

/// Bring the window up and ask the UI for its log form.
pub fn open_log_form(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
    let _ = app.emit(EVT_OPEN_LOG, json!({ "source": "streak" }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn at(iso: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(iso).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn day_key_follows_the_users_timezone_not_the_machine() {
        // 22:30 in Buenos Aires is already the next day in UTC.
        let instant = at("2026-08-13T01:30:00Z");
        assert_eq!(day_key(instant, parse_tz("America/Buenos_Aires")), "2026-08-12");
        assert_eq!(day_key(instant, parse_tz("UTC")), "2026-08-13");
        assert_eq!(day_key(instant, parse_tz("Asia/Tokyo")), "2026-08-13");
    }

    /// Golden values taken from the backend's own `getUserDayKey` (Intl with
    /// `timeZone`) — if this drifts, the reminder fires on the wrong day.
    #[test]
    fn day_keys_match_the_backends_intl_output() {
        let cases = [
            ("2026-08-13T01:30:00Z", "UTC", "2026-08-13"),
            ("2026-08-13T01:30:00Z", "America/Buenos_Aires", "2026-08-12"),
            ("2026-08-13T01:30:00Z", "America/Santiago", "2026-08-12"),
            ("2026-08-12T20:00:00Z", "Asia/Tokyo", "2026-08-13"),
            ("2026-08-12T20:00:00Z", "Asia/Kolkata", "2026-08-13"),
            ("2026-08-12T20:00:00Z", "Pacific/Auckland", "2026-08-13"),
            ("2026-01-01T18:45:00Z", "Asia/Kolkata", "2026-01-02"),
            ("2026-01-01T18:45:00Z", "America/Santiago", "2026-01-01"),
        ];
        for (iso, tz, expected) in cases {
            assert_eq!(day_key(at(iso), parse_tz(tz)), expected, "{iso} in {tz}");
        }
    }

    #[test]
    fn unknown_timezone_falls_back_to_utc() {
        assert_eq!(parse_tz("Mars/Olympus"), UTC);
        assert_eq!(parse_tz(""), UTC);
    }

    #[test]
    fn last_streak_date_is_read_as_a_utc_anchor() {
        // The backend stores UTC midnight of the day key; reading it in the
        // user's timezone would slide it to the day before.
        assert_eq!(
            day_key_from_anchor("2026-08-12T00:00:00.000Z").as_deref(),
            Some("2026-08-12")
        );
        assert_eq!(day_key_from_anchor("2026-08-12").as_deref(), Some("2026-08-12"));
        assert_eq!(day_key_from_anchor("nonsense"), None);
    }

    #[test]
    fn at_risk_needs_a_running_streak_and_an_empty_day() {
        let now = at("2026-08-12T20:00:00Z");
        let body = |streak: i64, last: &str| {
            json!({ "valid": true, "user": {
                "stats": { "currentStreak": streak, "lastStreakDate": last },
                "settings": { "timezone": "UTC" }
            }})
        };

        let logged_today = status_from_verify(&body(7, "2026-08-12T00:00:00.000Z"), now);
        assert!(!logged_today.at_risk);

        let logged_yesterday = status_from_verify(&body(7, "2026-08-11T00:00:00.000Z"), now);
        assert!(logged_yesterday.at_risk);
        assert_eq!(logged_yesterday.current_streak, 7);

        let no_streak = status_from_verify(&body(0, "2026-08-01T00:00:00.000Z"), now);
        assert!(!no_streak.at_risk);
    }

    #[test]
    fn missing_timezone_is_utc_like_the_backend() {
        let now = at("2026-08-12T20:00:00Z");
        let status = status_from_verify(
            &json!({ "user": { "stats": { "currentStreak": 3 } } }),
            now,
        );
        assert_eq!(status.timezone, "UTC");
        assert!(status.at_risk);
        assert_eq!(status.last_day_key, None);
    }

    #[test]
    fn the_window_is_measured_against_the_users_midnight() {
        let now = at("2026-08-12T20:00:00Z");
        // 20:00 UTC leaves 4 h, but it is only 16:00 in New York, so 8 h there.
        assert_eq!(minutes_to_midnight(now, parse_tz("UTC")), 240);
        assert_eq!(minutes_to_midnight(now, parse_tz("America/New_York")), 480);

        assert!(!in_risk_window(240, 3));
        assert!(in_risk_window(179, 3));
        assert!(in_risk_window(0, 3));
    }

    #[test]
    fn a_deleted_local_midnight_still_yields_a_day_end() {
        // Chile skips 00:00 on the spring-forward night: the day starts at 01:00.
        let now = at("2026-09-06T02:00:00Z"); // 2026-09-05 23:00 in Santiago
        let minutes = minutes_to_midnight(now, parse_tz("America/Santiago"));
        assert!(minutes > 0 && minutes <= 120, "got {minutes}");
    }

    #[test]
    fn the_fixed_hour_only_fires_inside_its_grace_period() {
        let tz = parse_tz("UTC");
        assert!(fixed_time_hit(at("2026-08-12T21:00:00Z"), tz, "21:00"));
        assert!(fixed_time_hit(at("2026-08-12T21:09:00Z"), tz, "21:00"));
        // Opening the app an hour late must not fire the missed reminder.
        assert!(!fixed_time_hit(at("2026-08-12T22:00:00Z"), tz, "21:00"));
        assert!(!fixed_time_hit(at("2026-08-12T20:59:00Z"), tz, "21:00"));
        assert!(!fixed_time_hit(at("2026-08-12T21:00:00Z"), tz, ""));
        assert!(!fixed_time_hit(at("2026-08-12T21:00:00Z"), tz, "25:00"));
    }
}
