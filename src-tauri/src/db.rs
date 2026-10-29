//! Local SQLite store: settings, VN registry, the offline outbox and the
//! texthooker line buffer. Everything the app needs to keep working with no
//! network, and to catch up once it comes back.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::config::Settings;

pub type Result<T> = std::result::Result<T, rusqlite::Error>;

pub struct Db {
    conn: Mutex<Connection>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub enum ReaderMode {
    #[default]
    Integrated,
    Browser,
    Overlay,
}

impl ReaderMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Integrated => "integrated",
            Self::Browser => "browser",
            Self::Overlay => "overlay",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct VnApp {
    /// Executable file name, lowercased. Primary key.
    pub exe: String,
    pub media_id: String,
    pub media_type: String,
    pub media_title: String,
    pub image: Option<String>,
    #[serde(default)]
    pub launch_path: Option<String>,
    #[serde(default)]
    pub hooker_path: Option<String>,
    /// Attach LunaHook directly to this registered game when it starts.
    #[serde(default)]
    pub native_hook_enabled: bool,
    /// Hook code selected for this executable, if the user chose one.
    #[serde(default)]
    pub native_hook_code: Option<String>,
    #[serde(default)]
    pub reader_mode: ReaderMode,
    pub enabled: bool,
    /// Use the work rating for games and VNs with no cached cover rating.
    #[serde(default)]
    pub is_adult: bool,
    /// Use the VN cover rating for Discord, including while offline.
    #[serde(default)]
    pub is_adult_image: Option<bool>,
    /// Chinese filter for this work's lines; `None` follows the global setting.
    #[serde(default)]
    pub filter_chinese: Option<bool>,
}

/// One entry of the Discord presence blacklist: a media contentId or an
/// executable the user never wants published.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct BlacklistEntry {
    /// contentId or executable, lowercased.
    pub key: String,
    /// `media` | `process`
    pub kind: String,
    /// What to call it in the settings list.
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboxItem {
    pub id: String,
    /// `log` | `texthooker_lines` | `texthooker_timer` | `texthooker_history`
    pub kind: String,
    pub payload: String,
    pub created_at: String,
    pub attempts: i64,
    pub last_error: Option<String>,
    /// `queued` | `sent` | `failed`
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingLog {
    pub id: String,
    pub payload: String,
    pub created_at: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct TextLine {
    pub id: String,
    pub content_id: String,
    pub text: String,
    pub chars: i64,
    pub created_at: String,
    pub synced: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct MinedSentence {
    pub id: String,
    pub line_id: String,
    pub content_id: String,
    pub text: String,
    pub media_title: Option<String>,
    pub created_at: String,
    pub anki_note_id: Option<i64>,
    pub screenshot_path: Option<String>,
}

/// One "log session" event, kept locally so the reader can show its history
/// with no network — the server has the same rows under `sessionHistory`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextHistoryEntry {
    pub id: String,
    #[serde(rename = "contentId")]
    pub content_id: String,
    #[serde(rename = "loggedAt")]
    pub logged_at: String,
    #[serde(rename = "charactersLogged")]
    pub chars: i64,
    #[serde(rename = "linesLogged")]
    pub lines: i64,
    #[serde(rename = "sessionSeconds")]
    pub seconds: i64,
    #[serde(rename = "readingSpeed")]
    pub speed: i64,
    #[serde(default)]
    pub intelligence: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TextState {
    pub content_id: String,
    pub timer_seconds: i64,
    pub baseline_chars: i64,
    pub baseline_lines: i64,
    pub media_type: Option<String>,
    pub media_title: Option<String>,
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS kv (
    k TEXT PRIMARY KEY,
    v TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS vn_apps (
    exe TEXT PRIMARY KEY,
    media_id TEXT NOT NULL,
    media_type TEXT NOT NULL DEFAULT 'vn',
    media_title TEXT NOT NULL,
    image TEXT,
    texthooker_url TEXT,
    enabled INTEGER NOT NULL DEFAULT 1
);
CREATE TABLE IF NOT EXISTS outbox (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    payload TEXT NOT NULL,
    created_at TEXT NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    status TEXT NOT NULL DEFAULT 'queued'
);
CREATE INDEX IF NOT EXISTS outbox_status ON outbox(status, created_at);
CREATE TABLE IF NOT EXISTS pending_logs (
    id TEXT PRIMARY KEY,
    payload TEXT NOT NULL,
    created_at TEXT NOT NULL,
    source TEXT NOT NULL DEFAULT 'anime'
);
CREATE TABLE IF NOT EXISTS text_lines (
    id TEXT PRIMARY KEY,
    content_id TEXT NOT NULL,
    text TEXT NOT NULL,
    chars INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    synced INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS text_lines_content ON text_lines(content_id, created_at);
CREATE TABLE IF NOT EXISTS mined_sentences (
    id TEXT PRIMARY KEY,
    line_id TEXT NOT NULL UNIQUE,
    content_id TEXT NOT NULL,
    text TEXT NOT NULL,
    media_title TEXT,
    created_at TEXT NOT NULL,
    anki_note_id INTEGER,
    screenshot_path TEXT
);
CREATE INDEX IF NOT EXISTS mined_sentences_created ON mined_sentences(created_at DESC);
CREATE TABLE IF NOT EXISTS text_state (
    content_id TEXT PRIMARY KEY,
    timer_seconds INTEGER NOT NULL DEFAULT 0,
    baseline_chars INTEGER NOT NULL DEFAULT 0,
    baseline_lines INTEGER NOT NULL DEFAULT 0,
    media_type TEXT,
    media_title TEXT
);
CREATE TABLE IF NOT EXISTS text_history (
    id TEXT PRIMARY KEY,
    content_id TEXT NOT NULL,
    logged_at TEXT NOT NULL,
    chars INTEGER NOT NULL,
    lines INTEGER NOT NULL,
    seconds INTEGER NOT NULL,
    speed INTEGER NOT NULL,
    intelligence TEXT
);
CREATE INDEX IF NOT EXISTS text_history_content ON text_history(content_id, logged_at);
CREATE TABLE IF NOT EXISTS watch_history (
    key TEXT PRIMARY KEY,
    logged_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS discord_blacklist (
    key TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    label TEXT NOT NULL
);
"#;

/// Columns added after the first release. SQLite has no `ADD COLUMN IF NOT
/// EXISTS`, so a duplicate-column error simply means the database is already
/// up to date.
const MIGRATIONS: &[&str] = &[
    "ALTER TABLE vn_apps ADD COLUMN is_adult_image INTEGER",
    "ALTER TABLE vn_apps ADD COLUMN is_adult INTEGER NOT NULL DEFAULT 0",
    "ALTER TABLE vn_apps ADD COLUMN launch_path TEXT",
    "ALTER TABLE vn_apps ADD COLUMN hooker_path TEXT",
    "ALTER TABLE vn_apps ADD COLUMN native_hook_enabled INTEGER NOT NULL DEFAULT 0",
    "ALTER TABLE vn_apps ADD COLUMN native_hook_code TEXT",
    "ALTER TABLE text_history ADD COLUMN intelligence TEXT",
    "ALTER TABLE mined_sentences ADD COLUMN screenshot_path TEXT",
    "ALTER TABLE vn_apps ADD COLUMN filter_chinese INTEGER",
];

#[derive(Debug, thiserror::Error)]
pub enum RestoreError {
    #[error("not a NihongoTracker Companion backup")]
    NotABackup,
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
}

/// Bring any database, new or restored from an old backup, to the current schema.
fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA)?;
    for statement in MIGRATIONS {
        let _ = conn.execute(statement, []);
    }
    if conn.execute("ALTER TABLE vn_apps ADD COLUMN browser_texthooker INTEGER NOT NULL DEFAULT 0", []).is_ok() {
        conn.execute("UPDATE vn_apps SET browser_texthooker = 1 WHERE texthooker_url IS NOT NULL AND TRIM(texthooker_url) != ''", [])?;
    }
    if conn.execute("ALTER TABLE vn_apps ADD COLUMN reader_mode TEXT NOT NULL DEFAULT 'integrated'", []).is_ok() {
        conn.execute("UPDATE vn_apps SET reader_mode = CASE WHEN browser_texthooker = 1 THEN 'browser' ELSE 'integrated' END", [])?;
    }
    Ok(())
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    // ── backups ────────────────────────────────────────────────────────

    /// A consistent copy of the whole database, whatever is still in the WAL
    /// included. `VACUUM INTO` refuses to overwrite, so an old file goes first.
    pub fn export_to(&self, dest: &Path) -> Result<()> {
        let _ = std::fs::remove_file(dest);
        self.with(|c| {
            c.execute("VACUUM INTO ?1", params![dest.to_string_lossy()])
                .map(|_| ())
        })
    }

    /// Replace everything with `source`'s contents, in place: the connection
    /// other threads hold stays valid. A backup from an older version is
    /// migrated like any database on open.
    pub fn restore_from(&self, source: &Path) -> std::result::Result<(), RestoreError> {
        let backup = Connection::open_with_flags(source, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|_| RestoreError::NotABackup)?;
        let ours: i64 = backup
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN ('kv', 'vn_apps', 'outbox')",
                [],
                |r| r.get(0),
            )
            .map_err(|_| RestoreError::NotABackup)?;
        if ours < 3 {
            return Err(RestoreError::NotABackup);
        }

        let mut conn = self.conn.lock().expect("db mutex poisoned");
        rusqlite::backup::Backup::new(&backup, &mut conn)?
            .run_to_completion(256, std::time::Duration::ZERO, None)?;
        migrate(&conn)?;
        Ok(())
    }

    fn with<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let guard = self.conn.lock().expect("db mutex poisoned");
        f(&guard)
    }

    // ── settings ───────────────────────────────────────────────────────

    pub fn load_settings(&self) -> Settings {
        let raw: Option<String> = self
            .with(|c| {
                c.query_row("SELECT v FROM kv WHERE k = 'settings'", [], |r| r.get(0))
                    .optional()
            })
            .unwrap_or(None);
        let mut settings: Settings = raw.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        settings.discord_app_id = "1378024719672016977".into();
        settings.discord_large_image = "nihongotracker".into();
        settings
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        let json = serde_json::to_string(settings).unwrap_or_else(|_| "{}".into());
        self.with(|c| {
            c.execute(
                "INSERT INTO kv (k, v) VALUES ('settings', ?1)
                 ON CONFLICT(k) DO UPDATE SET v = excluded.v",
                params![json],
            )
            .map(|_| ())
        })
    }

    // ── generic key/value ──────────────────────────────────────────────

    pub fn get_kv(&self, key: &str) -> Option<String> {
        self.with(|c| {
            c.query_row("SELECT v FROM kv WHERE k = ?1", params![key], |r| r.get(0))
                .optional()
        })
        .ok()
        .flatten()
    }

    pub fn set_kv(&self, key: &str, value: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO kv (k, v) VALUES (?1, ?2)
                 ON CONFLICT(k) DO UPDATE SET v = excluded.v",
                params![key, value],
            )
            .map(|_| ())
        })
    }

    pub fn delete_kv(&self, key: &str) -> Result<()> {
        self.with(|c| {
            c.execute("DELETE FROM kv WHERE k = ?1", params![key])
                .map(|_| ())
        })
    }

    /// Every stored `title_map:*` entry, as `(title, json)`.
    pub fn list_title_map(&self) -> Result<Vec<(String, String)>> {
        self.with(|c| {
            let mut stmt = c.prepare("SELECT k, v FROM kv WHERE k LIKE 'title_map:%'")?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?
                        .trim_start_matches("title_map:")
                        .to_string(),
                    r.get::<_, String>(1)?,
                ))
            })?;
            rows.collect()
        })
    }

    // ── VN registry ────────────────────────────────────────────────────

    pub fn list_vn_apps(&self) -> Result<Vec<VnApp>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT exe, media_id, media_type, media_title, image, reader_mode, enabled, is_adult, launch_path, hooker_path, native_hook_enabled, native_hook_code, is_adult_image, filter_chinese
                 FROM vn_apps ORDER BY media_title COLLATE NOCASE",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(VnApp {
                    exe: r.get(0)?,
                    media_id: r.get(1)?,
                    media_type: r.get(2)?,
                    media_title: r.get(3)?,
                    image: r.get(4)?,
                    reader_mode: match r.get::<_, String>(5)?.as_str() {
                        "browser" => ReaderMode::Browser,
                        "overlay" => ReaderMode::Overlay,
                        _ => ReaderMode::Integrated,
                    },
                    enabled: r.get::<_, i64>(6)? != 0,
                    is_adult: r.get::<_, i64>(7)? != 0,
                    is_adult_image: r.get::<_, Option<i64>>(12)?.map(|value| value != 0),
                    launch_path: r.get(8)?,
                    hooker_path: r.get(9)?,
                    native_hook_enabled: r.get::<_, i64>(10)? != 0,
                    native_hook_code: r.get(11)?,
                    filter_chinese: r.get::<_, Option<i64>>(13)?.map(|value| value != 0),
                })
            })?;
            rows.collect()
        })
    }

    pub fn upsert_vn_app(&self, app: &VnApp) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO vn_apps (exe, media_id, media_type, media_title, image, reader_mode, enabled, is_adult, launch_path, hooker_path, native_hook_enabled, native_hook_code, is_adult_image, filter_chinese)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
                 ON CONFLICT(exe) DO UPDATE SET
                    media_id = excluded.media_id,
                    media_type = excluded.media_type,
                    media_title = excluded.media_title,
                    image = excluded.image,
                    reader_mode = excluded.reader_mode,
                    enabled = excluded.enabled,
                    is_adult = excluded.is_adult,
                    is_adult_image = CASE WHEN vn_apps.media_id = excluded.media_id THEN COALESCE(excluded.is_adult_image, vn_apps.is_adult_image) ELSE excluded.is_adult_image END,
                    launch_path = excluded.launch_path,
                    hooker_path = excluded.hooker_path,
                    native_hook_enabled = excluded.native_hook_enabled,
                    native_hook_code = COALESCE(excluded.native_hook_code, vn_apps.native_hook_code),
                    filter_chinese = excluded.filter_chinese",
                params![
                    app.exe.to_lowercase(),
                    app.media_id,
                    app.media_type,
                    app.media_title,
                    app.image,
                    app.reader_mode.as_str(),
                    app.enabled as i64,
                    app.is_adult as i64,
                    app.launch_path,
                    app.hooker_path,
                    app.native_hook_enabled as i64,
                    app.native_hook_code,
                    app.is_adult_image.map(i64::from),
                    app.filter_chinese.map(i64::from)
                ],
            )
            .map(|_| ())
        })
    }

    /// The per-work Chinese filter, if any executable of `content_id` sets one.
    pub fn chinese_filter_for(&self, content_id: &str) -> Option<bool> {
        self.with(|c| {
            c.query_row(
                "SELECT filter_chinese FROM vn_apps WHERE media_id = ?1 AND filter_chinese IS NOT NULL
                 ORDER BY enabled DESC LIMIT 1",
                params![content_id],
                |r| r.get::<_, i64>(0),
            )
            .optional()
        })
        .ok()
        .flatten()
        .map(|value| value != 0)
    }

    pub fn set_native_hook_code(&self, exe: &str, hook_code: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "UPDATE vn_apps SET native_hook_code = ?2 WHERE exe = ?1",
                params![exe.to_lowercase(), hook_code],
            )
            .map(|_| ())
        })
    }

    pub fn set_vn_image_rating(&self, content_id: &str, is_adult_image: bool) -> Result<()> {
        self.with(|connection| connection.execute(
            "UPDATE vn_apps SET is_adult_image = ?2 WHERE media_id = ?1 AND media_type = 'vn'",
            params![content_id, i64::from(is_adult_image)],
        ).map(|_| ()))
    }

    // ── Discord blacklist ──────────────────────────────────────────────

    pub fn discord_blacklist(&self) -> Result<Vec<BlacklistEntry>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT key, kind, label FROM discord_blacklist ORDER BY label COLLATE NOCASE",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(BlacklistEntry {
                    key: r.get(0)?,
                    kind: r.get(1)?,
                    label: r.get(2)?,
                })
            })?;
            rows.collect()
        })
    }

    pub fn add_discord_blacklist(&self, entry: &BlacklistEntry) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO discord_blacklist (key, kind, label) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET kind = excluded.kind, label = excluded.label",
                params![entry.key.trim().to_lowercase(), entry.kind, entry.label],
            )
            .map(|_| ())
        })
    }

    pub fn delete_discord_blacklist(&self, key: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "DELETE FROM discord_blacklist WHERE key = ?1",
                params![key.trim().to_lowercase()],
            )
            .map(|_| ())
        })
    }

    pub fn delete_vn_app(&self, exe: &str) -> Result<()> {
        self.with(|c| {
            c.execute("DELETE FROM vn_apps WHERE exe = ?1", params![exe.to_lowercase()])
                .map(|_| ())
        })
    }

    // ── outbox ─────────────────────────────────────────────────────────

    pub fn enqueue(&self, id: &str, kind: &str, payload: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT OR REPLACE INTO outbox (id, kind, payload, created_at, attempts, status)
                 VALUES (?1, ?2, ?3, ?4, 0, 'queued')",
                params![id, kind, payload, now_iso()],
            )
            .map(|_| ())
        })
    }

    pub fn queued(&self, limit: i64) -> Result<Vec<OutboxItem>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT id, kind, payload, created_at, attempts, last_error, status
                 FROM outbox WHERE status = 'queued' ORDER BY created_at LIMIT ?1",
            )?;
            let rows = stmt.query_map(params![limit], map_outbox)?;
            rows.collect()
        })
    }

    pub fn outbox_all(&self, limit: i64) -> Result<Vec<OutboxItem>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT id, kind, payload, created_at, attempts, last_error, status
                 FROM outbox ORDER BY created_at DESC LIMIT ?1",
            )?;
            let rows = stmt.query_map(params![limit], map_outbox)?;
            rows.collect()
        })
    }

    pub fn mark_sent(&self, id: &str) -> Result<()> {
        self.with(|c| {
            c.execute("UPDATE outbox SET status = 'sent' WHERE id = ?1", params![id])
                .map(|_| ())
        })
    }

    pub fn mark_failed(&self, id: &str, error: &str, permanent: bool) -> Result<()> {
        let status = if permanent { "failed" } else { "queued" };
        self.with(|c| {
            c.execute(
                "UPDATE outbox SET attempts = attempts + 1, last_error = ?2, status = ?3 WHERE id = ?1",
                params![id, error, status],
            )
            .map(|_| ())
        })
    }

    pub fn requeue(&self, id: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "UPDATE outbox SET status = 'queued', last_error = NULL WHERE id = ?1",
                params![id],
            )
            .map(|_| ())
        })
    }

    /// Replace a log that hasn't left yet and queue it again. `false` when it
    /// was already sent (or isn't a log), so the edit would be lost.
    pub fn update_outbox_log(&self, id: &str, payload: &str) -> Result<bool> {
        self.with(|c| {
            c.execute(
                "UPDATE outbox SET payload = ?2, status = 'queued', last_error = NULL, attempts = 0
                 WHERE id = ?1 AND kind = 'log' AND status != 'sent'",
                params![id, payload],
            )
            .map(|changed| changed > 0)
        })
    }

    pub fn delete_outbox(&self, id: &str) -> Result<()> {
        self.with(|c| {
            c.execute("DELETE FROM outbox WHERE id = ?1", params![id])
                .map(|_| ())
        })
    }

    pub fn queued_count(&self) -> i64 {
        self.with(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM outbox WHERE status = 'queued'",
                [],
                |r| r.get(0),
            )
        })
        .unwrap_or(0)
    }

    /// Drop delivered outbox rows and stale dedup keys so the file stays small.
    pub fn prune(&self, days: i64) -> Result<()> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(days)).to_rfc3339();
        self.with(|c| {
            c.execute(
                "DELETE FROM outbox WHERE status = 'sent' AND created_at < ?1",
                params![cutoff],
            )?;
            c.execute("DELETE FROM watch_history WHERE logged_at < ?1", params![cutoff])?;
            Ok(())
        })
    }

    // ── pending confirmations ──────────────────────────────────────────

    pub fn add_pending(&self, id: &str, payload: &str, source: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT OR REPLACE INTO pending_logs (id, payload, created_at, source)
                 VALUES (?1, ?2, ?3, ?4)",
                params![id, payload, now_iso(), source],
            )
            .map(|_| ())
        })
    }

    pub fn list_pending(&self) -> Result<Vec<PendingLog>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT id, payload, created_at, source FROM pending_logs ORDER BY created_at",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(PendingLog {
                    id: r.get(0)?,
                    payload: r.get(1)?,
                    created_at: r.get(2)?,
                    source: r.get(3)?,
                })
            })?;
            rows.collect()
        })
    }

    pub fn get_pending(&self, id: &str) -> Result<Option<PendingLog>> {
        self.with(|c| {
            c.query_row(
                "SELECT id, payload, created_at, source FROM pending_logs WHERE id = ?1",
                params![id],
                |r| {
                    Ok(PendingLog {
                        id: r.get(0)?,
                        payload: r.get(1)?,
                        created_at: r.get(2)?,
                        source: r.get(3)?,
                    })
                },
            )
            .optional()
        })
    }

    pub fn delete_pending(&self, id: &str) -> Result<()> {
        self.with(|c| {
            c.execute("DELETE FROM pending_logs WHERE id = ?1", params![id])
                .map(|_| ())
        })
    }

    // ── watch history (double-log guard) ───────────────────────────────

    pub fn was_logged(&self, key: &str) -> bool {
        self.with(|c| {
            c.query_row(
                "SELECT 1 FROM watch_history WHERE key = ?1",
                params![key],
                |r| r.get::<_, i64>(0),
            )
            .optional()
        })
        .ok()
        .flatten()
        .is_some()
    }

    pub fn remember_logged(&self, key: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT OR REPLACE INTO watch_history (key, logged_at) VALUES (?1, ?2)",
                params![key, now_iso()],
            )
            .map(|_| ())
        })
    }

    // ── texthooker ─────────────────────────────────────────────────────

    pub fn insert_line(&self, line: &TextLine) -> Result<bool> {
        self.with(|c| {
            let changed = c.execute(
                "INSERT OR IGNORE INTO text_lines (id, content_id, text, chars, created_at, synced)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    line.id,
                    line.content_id,
                    line.text,
                    line.chars,
                    line.created_at,
                    line.synced as i64
                ],
            )?;
            Ok(changed > 0)
        })
    }

    pub fn lines_for(&self, content_id: &str, limit: i64) -> Result<Vec<TextLine>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT id, content_id, text, chars, created_at, synced FROM text_lines
                 WHERE content_id = ?1 ORDER BY created_at LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![content_id, limit], map_line)?;
            rows.collect()
        })
    }

    pub fn line_by_id(&self, id: &str) -> Result<Option<TextLine>> {
        self.with(|c| c.query_row(
            "SELECT id, content_id, text, chars, created_at, synced FROM text_lines WHERE id = ?1",
            params![id], map_line,
        ).optional())
    }

    pub fn latest_line(&self, content_id: &str) -> Result<Option<TextLine>> {
        self.with(|c| c.query_row(
            "SELECT id, content_id, text, chars, created_at, synced FROM text_lines WHERE content_id = ?1 ORDER BY created_at DESC, rowid DESC LIMIT 1",
            params![content_id], map_line,
        ).optional())
    }

    pub fn mine_line(&self, line: &TextLine, media_title: Option<&str>) -> Result<MinedSentence> {
        let id = uuid::Uuid::new_v4().to_string();
        let created_at = now_iso();
        self.with(|c| {
            c.execute(
                "INSERT OR IGNORE INTO mined_sentences (id, line_id, content_id, text, media_title, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id, line.id, line.content_id, line.text, media_title, created_at],
            )?;
            c.query_row(
                "SELECT id, line_id, content_id, text, media_title, created_at, anki_note_id, screenshot_path FROM mined_sentences WHERE line_id = ?1",
                params![line.id], map_mined,
            )
        })
    }

    pub fn mined_sentences(&self) -> Result<Vec<MinedSentence>> {
        self.with(|c| {
            let mut stmt = c.prepare("SELECT id, line_id, content_id, text, media_title, created_at, anki_note_id, screenshot_path FROM mined_sentences ORDER BY created_at DESC LIMIT 500")?;
            let rows = stmt.query_map([], map_mined)?;
            rows.collect()
        })
    }

    pub fn mined_by_id(&self, id: &str) -> Result<Option<MinedSentence>> {
        self.with(|c| c.query_row(
            "SELECT id, line_id, content_id, text, media_title, created_at, anki_note_id, screenshot_path FROM mined_sentences WHERE id = ?1",
            params![id], map_mined,
        ).optional())
    }

    pub fn mark_mined_exported(&self, id: &str, note_id: i64) -> Result<()> {
        self.with(|c| c.execute("UPDATE mined_sentences SET anki_note_id = ?2 WHERE id = ?1", params![id, note_id]).map(|_| ()))
    }

    pub fn set_mined_screenshot(&self, id: &str, path: &str) -> Result<()> {
        self.with(|c| c.execute("UPDATE mined_sentences SET screenshot_path = ?2 WHERE id = ?1", params![id, path]).map(|_| ()))
    }

    pub fn delete_mined(&self, id: &str) -> Result<()> {
        self.with(|c| c.execute("DELETE FROM mined_sentences WHERE id = ?1", params![id]).map(|_| ()))
    }

    pub fn unsynced_lines(&self, limit: i64) -> Result<Vec<TextLine>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT id, content_id, text, chars, created_at, synced FROM text_lines
                 WHERE synced = 0 ORDER BY created_at LIMIT ?1",
            )?;
            let rows = stmt.query_map(params![limit], map_line)?;
            rows.collect()
        })
    }

    pub fn mark_lines_synced(&self, ids: &[String]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        self.with(|c| {
            let tx_sql = format!(
                "UPDATE text_lines SET synced = 1 WHERE id IN ({})",
                ids.iter().map(|_| "?").collect::<Vec<_>>().join(",")
            );
            let params: Vec<&dyn rusqlite::ToSql> =
                ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
            c.execute(&tx_sql, params.as_slice()).map(|_| ())
        })
    }

    pub fn delete_line(&self, id: &str) -> Result<()> {
        self.with(|c| {
            c.execute("DELETE FROM text_lines WHERE id = ?1", params![id])
                .map(|_| ())
        })
    }

    pub fn clear_lines(&self, content_id: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "DELETE FROM text_lines WHERE content_id = ?1",
                params![content_id],
            )
            .map(|_| ())
        })
    }

    /// `(lines, chars)` currently stored for a session.
    pub fn text_totals(&self, content_id: &str) -> (i64, i64) {
        self.with(|c| {
            c.query_row(
                "SELECT COUNT(*), COALESCE(SUM(chars), 0) FROM text_lines WHERE content_id = ?1",
                params![content_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
        })
        .unwrap_or((0, 0))
    }

    pub fn get_text_state(&self, content_id: &str) -> TextState {
        self.with(|c| {
            c.query_row(
                "SELECT content_id, timer_seconds, baseline_chars, baseline_lines, media_type, media_title
                 FROM text_state WHERE content_id = ?1",
                params![content_id],
                |r| {
                    Ok(TextState {
                        content_id: r.get(0)?,
                        timer_seconds: r.get(1)?,
                        baseline_chars: r.get(2)?,
                        baseline_lines: r.get(3)?,
                        media_type: r.get(4)?,
                        media_title: r.get(5)?,
                    })
                },
            )
            .optional()
        })
        .ok()
        .flatten()
        .unwrap_or(TextState {
            content_id: content_id.to_string(),
            ..Default::default()
        })
    }

    pub fn save_text_state(&self, state: &TextState) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO text_state (content_id, timer_seconds, baseline_chars, baseline_lines, media_type, media_title)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(content_id) DO UPDATE SET
                    timer_seconds = excluded.timer_seconds,
                    baseline_chars = excluded.baseline_chars,
                    baseline_lines = excluded.baseline_lines,
                    media_type = COALESCE(excluded.media_type, text_state.media_type),
                    media_title = COALESCE(excluded.media_title, text_state.media_title)",
                params![
                    state.content_id,
                    state.timer_seconds,
                    state.baseline_chars,
                    state.baseline_lines,
                    state.media_type,
                    state.media_title
                ],
            )
            .map(|_| ())
        })
    }

    pub fn add_text_history(&self, entry: &TextHistoryEntry) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT OR REPLACE INTO text_history (id, content_id, logged_at, chars, lines, seconds, speed, intelligence)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    entry.id,
                    entry.content_id,
                    entry.logged_at,
                    entry.chars,
                    entry.lines,
                    entry.seconds,
                    entry.speed,
                    entry.intelligence.as_ref().and_then(|value| serde_json::to_string(value).ok())
                ],
            )
            .map(|_| ())
        })
    }

    pub fn text_history(&self, content_id: &str) -> Result<Vec<TextHistoryEntry>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT id, content_id, logged_at, chars, lines, seconds, speed, intelligence
                 FROM text_history WHERE content_id = ?1 ORDER BY logged_at DESC LIMIT 100",
            )?;
            let rows = stmt.query_map(params![content_id], |r| {
                Ok(TextHistoryEntry {
                    id: r.get(0)?,
                    content_id: r.get(1)?,
                    logged_at: r.get(2)?,
                    chars: r.get(3)?,
                    lines: r.get(4)?,
                    seconds: r.get(5)?,
                    speed: r.get(6)?,
                    intelligence: r.get::<_, Option<String>>(7)?
                        .and_then(|raw| serde_json::from_str(&raw).ok()),
                })
            })?;
            rows.collect()
        })
    }

    pub fn text_line_counts(&self, content_id: &str) -> Result<Vec<(String, i64)>> {
        self.with(|c| {
            let mut stmt = c.prepare("SELECT id, chars FROM text_lines WHERE content_id = ?1")?;
            let rows = stmt.query_map(params![content_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
            rows.collect()
        })
    }

    pub fn queued_log_chars(&self, content_id: &str) -> i64 {
        self.with(|c| {
            let mut stmt = c.prepare("SELECT payload FROM outbox WHERE kind = 'log' AND status = 'queued'")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            Ok(rows.filter_map(|row| row.ok()).filter_map(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
                .filter(|payload| payload.get("mediaId").and_then(|id| id.as_str()) == Some(content_id))
                .filter_map(|payload| payload.get("chars").and_then(|chars| chars.as_i64())).sum())
        }).unwrap_or(0)
    }

    pub fn list_text_states(&self) -> Result<Vec<TextState>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT content_id, timer_seconds, baseline_chars, baseline_lines, media_type, media_title
                 FROM text_state ORDER BY media_title COLLATE NOCASE",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(TextState {
                    content_id: r.get(0)?,
                    timer_seconds: r.get(1)?,
                    baseline_chars: r.get(2)?,
                    baseline_lines: r.get(3)?,
                    media_type: r.get(4)?,
                    media_title: r.get(5)?,
                })
            })?;
            rows.collect()
        })
    }
}

fn map_outbox(r: &rusqlite::Row<'_>) -> Result<OutboxItem> {
    Ok(OutboxItem {
        id: r.get(0)?,
        kind: r.get(1)?,
        payload: r.get(2)?,
        created_at: r.get(3)?,
        attempts: r.get(4)?,
        last_error: r.get(5)?,
        status: r.get(6)?,
    })
}

fn map_line(r: &rusqlite::Row<'_>) -> Result<TextLine> {
    Ok(TextLine {
        id: r.get(0)?,
        content_id: r.get(1)?,
        text: r.get(2)?,
        chars: r.get(3)?,
        created_at: r.get(4)?,
        synced: r.get::<_, i64>(5)? != 0,
    })
}

fn map_mined(r: &rusqlite::Row<'_>) -> Result<MinedSentence> {
    Ok(MinedSentence {
        id: r.get(0)?,
        line_id: r.get(1)?,
        content_id: r.get(2)?,
        text: r.get(3)?,
        media_title: r.get(4)?,
        created_at: r.get(5)?,
        anki_note_id: r.get(6)?,
        screenshot_path: r.get(7)?,
    })
}

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[cfg(test)]
mod mining_tests {
    use std::path::Path;
    use super::{Db, ReaderMode, TextLine, VnApp};

    #[test]
    fn browser_texthooker_migrates_existing_url_once_and_preserves_disabled_choice() {
        let path = std::env::temp_dir().join(format!("ntdc-browser-{}.sqlite3", uuid::Uuid::new_v4()));
        {
            let connection = rusqlite::Connection::open(&path).unwrap();
            connection.execute_batch(super::SCHEMA).unwrap();
            connection.execute("INSERT INTO vn_apps (exe, media_id, media_type, media_title, texthooker_url) VALUES ('game.exe','v17','vn','VN','https://nihongotracker.app/texthooker/v17')", []).unwrap();
        }
        {
            let db = Db::open(&path).unwrap();
            let mut entry = db.list_vn_apps().unwrap().remove(0);
            assert_eq!(entry.reader_mode, ReaderMode::Browser);
            entry.reader_mode = ReaderMode::Integrated;
            db.upsert_vn_app(&entry).unwrap();
        }
        {
            let db = Db::open(&path).unwrap();
            assert_eq!(db.list_vn_apps().unwrap()[0].reader_mode, ReaderMode::Integrated);
        }
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn reader_mode_migrates_browser_choice_and_preserves_overlay_after_reopen() {
        let path = std::env::temp_dir().join(format!("ntdc-reader-{}.sqlite3", uuid::Uuid::new_v4()));
        {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute_batch(super::SCHEMA).unwrap();
            conn.execute("ALTER TABLE vn_apps ADD COLUMN browser_texthooker INTEGER NOT NULL DEFAULT 0", []).unwrap();
            conn.execute("INSERT INTO vn_apps (exe, media_id, media_title, browser_texthooker) VALUES ('browser.exe','v1','Browser VN',1), ('desktop.exe','v2','Desktop VN',0)", []).unwrap();
        }
        {
            let db = Db::open(&path).unwrap();
            let mut entries = db.list_vn_apps().unwrap();
            assert_eq!(entries[0].reader_mode, ReaderMode::Browser);
            assert_eq!(entries[1].reader_mode, ReaderMode::Integrated);
            entries[0].reader_mode = ReaderMode::Overlay;
            db.upsert_vn_app(&entries[0]).unwrap();
        }
        assert_eq!(Db::open(&path).unwrap().list_vn_apps().unwrap()[0].reader_mode, ReaderMode::Overlay);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn backup_round_trips_and_rejects_foreign_files() {
        let dir = std::env::temp_dir().join(format!("ntdc-backup-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let backup = dir.join("backup.sqlite3");

        let source = Db::open(&dir.join("source.sqlite3")).unwrap();
        source.enqueue("log-1", "log", r#"{"type":"anime"}"#).unwrap();
        source.export_to(&backup).unwrap();
        // A second export over the first must not fail on the existing file.
        source.export_to(&backup).unwrap();

        let target = Db::open(&dir.join("target.sqlite3")).unwrap();
        target.enqueue("other", "log", "{}").unwrap();
        target.restore_from(&backup).unwrap();
        let ids: Vec<String> = target.outbox_all(10).unwrap().into_iter().map(|i| i.id).collect();
        assert_eq!(ids, vec!["log-1".to_string()], "restore replaces, it does not merge");

        let foreign = dir.join("notes.txt");
        std::fs::write(&foreign, "not sqlite").unwrap();
        assert!(matches!(target.restore_from(&foreign), Err(super::RestoreError::NotABackup)));
        let other_db = dir.join("other.sqlite3");
        rusqlite::Connection::open(&other_db).unwrap().execute("CREATE TABLE t (x)", []).unwrap();
        assert!(matches!(target.restore_from(&other_db), Err(super::RestoreError::NotABackup)));
        assert_eq!(target.outbox_all(10).unwrap().len(), 1, "a rejected file changes nothing");

        drop((source, target));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn chinese_filter_is_per_work_and_falls_back_to_global() {
        let db = Db::open(Path::new(":memory:")).unwrap();
        let mut app = VnApp {
            exe: "air.exe".into(),
            media_id: "v36".into(),
            media_type: "vn".into(),
            media_title: "AIR".into(),
            image: None,
            launch_path: None,
            hooker_path: None,
            native_hook_enabled: false,
            native_hook_code: None,
            reader_mode: ReaderMode::Integrated,
            enabled: true,
            is_adult: false,
            is_adult_image: None,
            filter_chinese: None,
        };
        db.upsert_vn_app(&app).unwrap();
        assert_eq!(db.chinese_filter_for("v36"), None, "unset follows the global toggle");

        app.filter_chinese = Some(true);
        db.upsert_vn_app(&app).unwrap();
        assert_eq!(db.chinese_filter_for("v36"), Some(true));
        assert_eq!(db.list_vn_apps().unwrap()[0].filter_chinese, Some(true));

        app.filter_chinese = Some(false);
        db.upsert_vn_app(&app).unwrap();
        assert_eq!(db.chinese_filter_for("v36"), Some(false), "off overrides a global on");
        assert_eq!(db.chinese_filter_for("v1"), None);
    }

    #[test]
    fn selected_native_hook_is_saved_per_executable() {
        let db = Db::open(Path::new(":memory:")).unwrap();
        let app = VnApp {
            exe: "game.exe".into(),
            media_id: "v1".into(),
            media_type: "vn".into(),
            media_title: "Test VN".into(),
            image: None,
            launch_path: None,
            hooker_path: None,
            native_hook_enabled: true,
            native_hook_code: None,
            reader_mode: ReaderMode::Integrated,
            enabled: true,
            is_adult: false,
            is_adult_image: None,
            filter_chinese: None,
        };

        db.upsert_vn_app(&app).unwrap();
        db.set_native_hook_code("GAME.EXE", "/HSN-4@1234").unwrap();
        db.upsert_vn_app(&app).unwrap();

        let saved = db.list_vn_apps().unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].native_hook_code.as_deref(), Some("/HSN-4@1234"));
    }

    #[test]
    fn vn_cover_rating_is_cached_without_changing_other_library_fields() {
        let db = Db::open(Path::new(":memory:")).unwrap();
        let mut entry: VnApp = serde_json::from_value(serde_json::json!({
            "exe": "game.exe", "media_id": "v1143", "media_type": "vn",
            "media_title": "Majikoi", "enabled": true, "is_adult": true
        })).unwrap();
        assert!(entry.is_adult_image.is_none());
        db.upsert_vn_app(&entry).unwrap();
        db.set_vn_image_rating("v1143", false).unwrap();
        db.upsert_vn_app(&entry).unwrap();
        let cached = db.list_vn_apps().unwrap().remove(0);
        assert_eq!(cached.is_adult_image, Some(false));
        assert!(cached.is_adult);
        assert_eq!(cached.media_title, "Majikoi");
        entry.media_id = "v2".into();
        db.upsert_vn_app(&entry).unwrap();
        assert!(db.list_vn_apps().unwrap()[0].is_adult_image.is_none());
    }

    #[test]
    fn mined_sentence_survives_source_deletion_and_repeated_save() {
        let db = Db::open(Path::new(":memory:")).unwrap();
        let line = TextLine {
            id: "line-1".into(),
            content_id: "v1".into(),
            text: "今日は楽しい。".into(),
            chars: 7,
            created_at: super::now_iso(),
            synced: false,
        };
        db.insert_line(&line).unwrap();
        let first = db.mine_line(&line, Some("Game title")).unwrap();
        let second = db.mine_line(&line, Some("Game title")).unwrap();
        assert_eq!(first.id, second.id);
        db.delete_line(&line.id).unwrap();
        assert_eq!(db.mined_sentences().unwrap()[0].text, line.text);
        db.mark_mined_exported(&first.id, 42).unwrap();
        assert_eq!(db.mined_by_id(&first.id).unwrap().unwrap().anki_note_id, Some(42));
    }
}
