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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VnApp {
    /// Executable file name, lowercased. Primary key.
    pub exe: String,
    pub media_id: String,
    pub media_type: String,
    pub media_title: String,
    pub image: Option<String>,
    /// Optional extra URL to open alongside the built-in texthooker.
    pub texthooker_url: Option<String>,
    pub enabled: bool,
    /// `isAdult` as NihongoTracker reports it. Kept here so Discord Rich
    /// Presence can strip the title without a round trip.
    #[serde(default)]
    pub is_adult: bool,
}

/// One entry of the Discord presence blacklist: a media contentId or an
/// executable the user never wants published.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
pub struct TextLine {
    pub id: String,
    pub content_id: String,
    pub text: String,
    pub chars: i64,
    pub created_at: String,
    pub synced: bool,
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
    speed INTEGER NOT NULL
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
const MIGRATIONS: &[&str] = &["ALTER TABLE vn_apps ADD COLUMN is_adult INTEGER NOT NULL DEFAULT 0"];

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(SCHEMA)?;
        for statement in MIGRATIONS {
            let _ = conn.execute(statement, []);
        }
        Ok(Self {
            conn: Mutex::new(conn),
        })
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
        raw.and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
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
                "SELECT exe, media_id, media_type, media_title, image, texthooker_url, enabled, is_adult
                 FROM vn_apps ORDER BY media_title COLLATE NOCASE",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(VnApp {
                    exe: r.get(0)?,
                    media_id: r.get(1)?,
                    media_type: r.get(2)?,
                    media_title: r.get(3)?,
                    image: r.get(4)?,
                    texthooker_url: r.get(5)?,
                    enabled: r.get::<_, i64>(6)? != 0,
                    is_adult: r.get::<_, i64>(7)? != 0,
                })
            })?;
            rows.collect()
        })
    }

    pub fn upsert_vn_app(&self, app: &VnApp) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO vn_apps (exe, media_id, media_type, media_title, image, texthooker_url, enabled, is_adult)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(exe) DO UPDATE SET
                    media_id = excluded.media_id,
                    media_type = excluded.media_type,
                    media_title = excluded.media_title,
                    image = excluded.image,
                    texthooker_url = excluded.texthooker_url,
                    enabled = excluded.enabled,
                    is_adult = excluded.is_adult",
                params![
                    app.exe.to_lowercase(),
                    app.media_id,
                    app.media_type,
                    app.media_title,
                    app.image,
                    app.texthooker_url,
                    app.enabled as i64,
                    app.is_adult as i64
                ],
            )
            .map(|_| ())
        })
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

    pub fn line_text_exists(&self, content_id: &str, text: &str) -> bool {
        self.with(|c| {
            c.query_row(
                "SELECT 1 FROM text_lines WHERE content_id = ?1 AND text = ?2 LIMIT 1",
                params![content_id, text],
                |r| r.get::<_, i64>(0),
            )
            .optional()
        })
        .ok()
        .flatten()
        .is_some()
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

    /// Move a local/blank session's lines under a real media id once the user
    /// picks the work they were reading.
    pub fn reassign_lines(&self, from: &str, to: &str) -> Result<usize> {
        self.with(|c| {
            c.execute(
                "UPDATE text_lines SET content_id = ?2, synced = 0 WHERE content_id = ?1",
                params![from, to],
            )
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
                "INSERT OR REPLACE INTO text_history (id, content_id, logged_at, chars, lines, seconds, speed)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    entry.id,
                    entry.content_id,
                    entry.logged_at,
                    entry.chars,
                    entry.lines,
                    entry.seconds,
                    entry.speed
                ],
            )
            .map(|_| ())
        })
    }

    pub fn text_history(&self, content_id: &str) -> Result<Vec<TextHistoryEntry>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT id, content_id, logged_at, chars, lines, seconds, speed
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
                })
            })?;
            rows.collect()
        })
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

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}
