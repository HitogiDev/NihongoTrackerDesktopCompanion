//! Fushi (github.com/hajisensai/Fushi) plays video through an embedded libmpv
//! and titles its window "Fushi" whatever is on, so neither the window title
//! nor an mpv socket say what is playing. Its own database does: on every whole
//! second of playback it writes `video_books.last_position_ms` and
//! `last_played_at` for the episode, and the episode's length once as the
//! `video_duration_ms_<bookUid>` preference.
//!
//! Read that (read-only, never touching the schema) and Fushi looks like any
//! other player on the media bus: file, position, length, and whether it is
//! paused — by how fresh the last write is.

use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags, OptionalExtension};

use crate::platform::MediaPlayer;

/// The `players.rs` key, which is also the MPRIS-style name we report.
pub const PROCESS: &str = "fushi";

const DB_FILE: &str = "fushi.db";

/// Fushi writes once per second of playback; a little slack for the poll.
const PLAYING_WITHIN_MS: i64 = 5_000;
/// Past this the row is history, not a paused episode.
const STALE_AFTER_MS: i64 = 15 * 60 * 1000;

/// Where Fushi keeps its data, as far as finding `fushi.db` goes.
#[derive(Debug, Clone, Default)]
pub struct Env {
    pub windows: bool,
    pub macos: bool,
    pub appdata: Option<PathBuf>,
    pub xdg_data_home: Option<PathBuf>,
    pub home: Option<PathBuf>,
}

impl Env {
    pub fn current() -> Self {
        let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
        Self {
            windows: cfg!(windows),
            macos: cfg!(target_os = "macos"),
            appdata: var("APPDATA"),
            xdg_data_home: var("XDG_DATA_HOME"),
            home: var("HOME").or_else(|| var("USERPROFILE")),
        }
    }
}

/// Flutter's application support directory for Fushi: `CompanyName\ProductName`
/// under `%APPDATA%` on Windows, the bundle / application id elsewhere.
pub fn support_dirs(env: &Env) -> Vec<PathBuf> {
    if env.windows {
        return env.appdata.iter().map(|dir| dir.join("Fushi").join("Fushi")).collect();
    }
    if env.macos {
        return env
            .home
            .iter()
            .map(|home| home.join("Library/Application Support/app.fushi.reader"))
            .collect();
    }
    let Some(data) = env
        .xdg_data_home
        .clone()
        .or_else(|| env.home.as_ref().map(|home| home.join(".local/share")))
    else {
        return Vec::new();
    };
    vec![data.join("app.fushi.reader"), data.join("fushi")]
}

/// The user can move Fushi's data elsewhere (`data_root`, kept in Flutter's
/// `shared_preferences.json` in the support directory); the database then sits
/// in `<data_root>/support`.
pub fn data_root(shared_preferences: &str) -> Option<PathBuf> {
    let prefs: serde_json::Value = serde_json::from_str(shared_preferences).ok()?;
    let root = prefs.get("flutter.data_root")?.as_str()?.trim();
    (!root.is_empty()).then(|| PathBuf::from(root))
}

fn database_path(env: &Env) -> Option<PathBuf> {
    support_dirs(env).into_iter().find_map(|support| {
        let moved = std::fs::read_to_string(support.join("shared_preferences.json"))
            .ok()
            .as_deref()
            .and_then(data_root)
            .map(|root| root.join("support").join(DB_FILE));
        moved
            .into_iter()
            .chain(std::iter::once(support.join(DB_FILE)))
            .find(|path| path.is_file())
    })
}

/// The episode Fushi last wrote progress for.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub title: String,
    pub video_path: String,
    pub position_ms: i64,
    pub played_at_ms: i64,
    /// Raw `preferences.value`, encoded by Fushi's `PrefCodec` (`i:1420000`).
    pub duration: Option<String>,
}

const QUERY: &str = "
    SELECT b.title, b.video_path, b.last_position_ms, b.last_played_at,
           (SELECT p.value FROM preferences p WHERE p.key = 'video_duration_ms_' || b.book_uid)
    FROM video_books b
    WHERE b.last_played_at IS NOT NULL
    ORDER BY b.last_played_at DESC
    LIMIT 1";

fn latest_row(path: &Path) -> Option<Row> {
    // Read-only: Fushi's database is in WAL mode, so a reader never blocks its
    // writes, and a read-only connection never checkpoints on close.
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    conn.busy_timeout(Duration::from_millis(200)).ok()?;
    conn.query_row(QUERY, [], |row| {
        Ok(Row {
            title: row.get(0)?,
            video_path: row.get(1)?,
            position_ms: row.get(2)?,
            played_at_ms: row.get(3)?,
            duration: row.get(4)?,
        })
    })
    .optional()
    // A schema Fushi changed under us is just "nothing playing".
    .ok()
    .flatten()
}

/// `i:1420000` (or a bare number, from older builds) → milliseconds.
fn duration_ms(raw: &str) -> Option<i64> {
    let raw = raw.trim();
    let value = match raw.split_once(':') {
        Some(("i" | "d" | "s", payload)) => payload,
        _ => raw,
    };
    let ms = value.parse::<f64>().ok()?;
    (ms > 0.0).then_some(ms as i64)
}

/// What the row means at `now_ms`, as a media-bus entry. The file name stands
/// in for the track title, the way `players::media_file_name` reads it.
pub fn interpret(row: &Row, now_ms: i64) -> Option<MediaPlayer> {
    let age = now_ms - row.played_at_ms;
    if !(0..=STALE_AFTER_MS).contains(&age) {
        return None;
    }
    let path = row.video_path.trim();
    let file_name = if path.starts_with("http://") || path.starts_with("https://") {
        // A pasted stream: the URL says nothing, Fushi's title does.
        None
    } else {
        path.rsplit(['/', '\\']).next().map(str::trim).filter(|name| !name.is_empty())
    };
    let title = file_name.unwrap_or(row.title.trim());
    if title.is_empty() {
        return None;
    }
    Some(MediaPlayer {
        name: PROCESS.to_string(),
        title: Some(title.to_string()),
        url: None,
        position_secs: Some(row.position_ms.max(0) as f64 / 1000.0),
        duration_secs: row.duration.as_deref().and_then(duration_ms).map(|ms| ms as f64 / 1000.0),
        paused: age > PLAYING_WITHIN_MS,
    })
}

/// What Fushi is playing, if its database says it is. Only worth asking while
/// the process runs.
pub fn now_playing() -> Option<MediaPlayer> {
    let row = latest_row(&database_path(&Env::current())?)?;
    interpret(&row, chrono::Utc::now().timestamp_millis())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(played_at_ms: i64) -> Row {
        Row {
            title: "葬送のフリーレン 第5話".into(),
            video_path: r"D:\Anime\Frieren\[SubsPlease] Sousou no Frieren - 05 (1080p).mkv".into(),
            position_ms: 600_500,
            played_at_ms,
            duration: Some("i:1420000".into()),
        }
    }

    #[test]
    fn a_fresh_write_is_playing_with_the_file_name() {
        let media = interpret(&row(10_000), 12_000).unwrap();
        assert_eq!(media.name, "fushi");
        assert_eq!(media.title.as_deref(), Some("[SubsPlease] Sousou no Frieren - 05 (1080p).mkv"));
        assert_eq!(media.position_secs, Some(600.5));
        assert_eq!(media.duration_secs, Some(1420.0));
        assert!(!media.paused);
    }

    #[test]
    fn an_old_write_is_paused_and_then_history() {
        assert!(interpret(&row(0), 60_000).unwrap().paused);
        assert!(interpret(&row(0), STALE_AFTER_MS + 1).is_none());
        // Written "in the future" (clock change, synced row): not ours to trust.
        assert!(interpret(&row(10_000), 0).is_none());
    }

    #[test]
    fn streams_and_unix_paths() {
        let mut stream = row(0);
        stream.video_path = "https://example.com/watch?v=x".into();
        assert_eq!(interpret(&stream, 0).unwrap().title.as_deref(), Some("葬送のフリーレン 第5話"));

        let mut unix = row(0);
        unix.video_path = "/home/u/Anime/Show - 03.mkv".into();
        assert_eq!(interpret(&unix, 0).unwrap().title.as_deref(), Some("Show - 03.mkv"));
    }

    #[test]
    fn reads_pref_codec_durations() {
        assert_eq!(duration_ms("i:1420000"), Some(1_420_000));
        assert_eq!(duration_ms("1420000"), Some(1_420_000));
        assert_eq!(duration_ms("d:1420000.0"), Some(1_420_000));
        assert_eq!(duration_ms("z:"), None);
        assert_eq!(duration_ms("i:0"), None);
        let mut unknown = row(0);
        unknown.duration = None;
        assert_eq!(interpret(&unknown, 0).unwrap().duration_secs, None);
    }

    #[test]
    fn finds_the_support_directory_per_os() {
        let windows = Env { windows: true, appdata: Some(r"C:\Users\u\AppData\Roaming".into()), ..Env::default() };
        assert_eq!(support_dirs(&windows), vec![PathBuf::from(r"C:\Users\u\AppData\Roaming").join("Fushi").join("Fushi")]);

        let macos = Env { macos: true, home: Some("/Users/u".into()), ..Env::default() };
        assert_eq!(support_dirs(&macos), vec![PathBuf::from("/Users/u/Library/Application Support/app.fushi.reader")]);

        let linux = Env { home: Some("/home/u".into()), ..Env::default() };
        assert_eq!(support_dirs(&linux)[0], PathBuf::from("/home/u/.local/share/app.fushi.reader"));
    }

    #[test]
    fn follows_a_moved_data_root() {
        let prefs = r#"{"flutter.data_root":"D:\\APP\\FUSHI_date","flutter.other":1}"#;
        assert_eq!(data_root(prefs), Some(PathBuf::from(r"D:\APP\FUSHI_date")));
        assert_eq!(data_root(r#"{"flutter.data_root":""}"#), None);
        assert_eq!(data_root("{}"), None);
    }

    #[test]
    fn reads_a_database_shaped_like_fushis() {
        let dir = std::env::temp_dir().join(format!("nt-fushi-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(DB_FILE);
        let _ = std::fs::remove_file(&path);
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "PRAGMA journal_mode=WAL;
                 CREATE TABLE video_books (book_uid TEXT PRIMARY KEY, title TEXT NOT NULL,
                   video_path TEXT NOT NULL, last_position_ms INTEGER NOT NULL DEFAULT 0,
                   last_played_at INTEGER);
                 CREATE TABLE preferences (key TEXT PRIMARY KEY, value TEXT NOT NULL,
                   updated_at INTEGER NOT NULL DEFAULT 0);
                 INSERT INTO video_books VALUES ('a', 'Old', '/v/Old - 01.mkv', 5000, 100);
                 INSERT INTO video_books VALUES ('b', 'New', '/v/New - 02.mkv', 7000, 200);
                 INSERT INTO video_books VALUES ('c', 'Never', '/v/Never.mkv', 0, NULL);
                 INSERT INTO preferences VALUES ('video_duration_ms_b', 'i:1440000', 0);",
            )
            .unwrap();
            let row = latest_row(&path).unwrap();
            assert_eq!(row.video_path, "/v/New - 02.mkv");
            assert_eq!(row.position_ms, 7000);
            assert_eq!(row.duration.as_deref(), Some("i:1440000"));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
