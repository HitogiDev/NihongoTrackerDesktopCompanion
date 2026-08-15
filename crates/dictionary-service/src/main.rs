//! Dictionary lookup over HTTP, for the NihongoTracker texthooker.
//!
//! The shape of this service is decided by the spike measurements, not by
//! taste:
//!
//! - **One offset per request.** A single offset costs 2.62 ms on average
//!   (p99 7.7 ms). Resolving every offset of a line costs 127 ms on average and
//!   555 ms at p99, so the client asks for the character under the cursor and
//!   nothing else.
//! - **No result cache.** Keyed on the input text it measured a 7.4 % hit rate,
//!   because every offset of a line is a different suffix.
//!
//! It never talks to a browser directly: the Express backend fronts it, and that
//! is where authentication and rate limiting live.

mod importer;
mod licenses;
mod pool;

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use pool::DictionaryPool;

struct AppState {
    pool: DictionaryPool,
    data_dir: PathBuf,
    started: std::time::Instant,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=info".into()),
        )
        .init();

    let data_dir = env_path("DICTIONARY_DATA_DIR", "/data/dictionaries");
    let import_dir = env_path("DICTIONARY_IMPORT_DIR", "/data/import");
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(8081);
    let pool_size: usize = std::env::var("DICTIONARY_POOL_SIZE")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get().min(4)).unwrap_or(2));

    // Importing is slow and blocking, and it must finish before anything is
    // served — so it happens here, off the async runtime's back.
    let dictionaries = tokio::task::spawn_blocking({
        let import_dir = import_dir.clone();
        let data_dir = data_dir.clone();
        move || importer::sync_dictionaries(&import_dir, &data_dir)
    })
    .await
    .expect("import task");

    if dictionaries.is_empty() {
        tracing::warn!(
            data = %data_dir.display(),
            import = %import_dir.display(),
            "no dictionaries found — the service will answer, but every lookup will be empty"
        );
    }

    // Opening warms every engine, so the first request does not pay for the
    // ~490 ms of page faults a cold memory map costs. Traffic is only accepted
    // after this returns.
    let pool = tokio::task::spawn_blocking({
        let dictionaries = dictionaries.clone();
        move || DictionaryPool::open(&dictionaries, pool_size)
    })
    .await
    .expect("warm-up task");

    tracing::info!(
        dictionaries = pool.loaded().len(),
        engines = pool.size(),
        "warmed up"
    );

    let state = Arc::new(AppState {
        pool,
        data_dir,
        started: std::time::Instant::now(),
    });

    let app = Router::new()
        .route("/health", get(health))
        .route("/lookup", post(lookup))
        .route("/licenses", get(license_list))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .unwrap_or_else(|error| panic!("cannot bind port {port}: {error}"));
    tracing::info!(port, "listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server");
}

// ── health ─────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct Health {
    status: &'static str,
    /// False when no dictionary is loaded: the service answers, but emptily.
    ready: bool,
    engines: usize,
    #[serde(rename = "uptimeSeconds")]
    uptime_seconds: u64,
    dictionaries: Vec<pool::LoadedDictionary>,
}

async fn health(State(state): State<Arc<AppState>>) -> Json<Health> {
    Json(Health {
        status: "ok",
        ready: !state.pool.is_empty(),
        engines: state.pool.size(),
        uptime_seconds: state.started.elapsed().as_secs(),
        dictionaries: state.pool.loaded().to_vec(),
    })
}

// ── lookup ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LookupBody {
    text: String,
    /// Offset of the character under the cursor, in **UTF-16 code units** —
    /// what `String.prototype.length` and the DOM's caret APIs count in.
    #[serde(default)]
    offset: usize,
    #[serde(default)]
    max_results: Option<i32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LookupResponse {
    /// The substring actually handed to the engine, so a client can tell what
    /// its offset resolved to.
    query: String,
    offset: usize,
    matches: Vec<Match>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Match {
    /// The part of `query` that matched.
    matched: String,
    /// The dictionary form it deinflected to.
    deinflected: String,
    /// The deinflection chain, in order.
    process: Vec<String>,
    expression: String,
    reading: String,
    /// Part-of-speech rules, as the dictionary states them.
    rules: String,
    frequencies: Vec<Frequency>,
    glossaries: Vec<GlossaryOut>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Frequency {
    dict_name: String,
    display: String,
    value: i32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GlossaryOut {
    dict_name: String,
    /// Yomitan structured content, passed through untouched. Rendering it is the
    /// client's job — the shared TypeScript renderer does it for both clients.
    content: Box<RawValue>,
}

const MAX_TEXT_LENGTH: usize = 4096;

async fn lookup(
    State(state): State<Arc<AppState>>,
    Json(body): Json<LookupBody>,
) -> Result<Json<LookupResponse>, (StatusCode, Json<ErrorBody>)> {
    if body.text.is_empty() {
        return Err(bad_request("text is required"));
    }
    if body.text.len() > MAX_TEXT_LENGTH {
        return Err(bad_request("text is too long"));
    }

    let Some(start) = byte_offset_for_utf16(&body.text, body.offset) else {
        return Err(bad_request("offset is past the end of text"));
    };
    let query = body.text[start..].to_string();
    let max_results = body.max_results.unwrap_or(8).clamp(1, 32);

    // The engine is blocking and holds a mutex for the duration; keeping it off
    // the async workers is what stops one slow lookup from stalling the runtime.
    let rows = {
        let state = state.clone();
        let query = query.clone();
        tokio::task::spawn_blocking(move || state.pool.lookup(&query, max_results))
            .await
            .map_err(|error| {
                tracing::error!(?error, "lookup task failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorBody { error: "lookup failed".into() }),
                )
            })?
    };

    let matches = rows.into_iter().map(into_match).collect();
    Ok(Json(LookupResponse {
        query,
        offset: body.offset,
        matches,
    }))
}

fn into_match(row: nihongo_dict::LookupRow) -> Match {
    Match {
        matched: row.matched,
        deinflected: row.deinflected,
        process: row.process,
        expression: row.expression,
        reading: row.reading,
        rules: row.rules,
        frequencies: row
            .frequencies
            .into_iter()
            .map(|hit| Frequency {
                dict_name: hit.dict_name,
                display: hit.display,
                value: hit.value,
            })
            .collect(),
        glossaries: row
            .glossaries
            .into_iter()
            .filter_map(|gloss| {
                // The glossary arrives as a JSON string. `RawValue` validates it
                // and passes the bytes through without building a tree, so what
                // the client receives is what the dictionary stored.
                match RawValue::from_string(gloss.text) {
                    Ok(content) => Some(GlossaryOut {
                        dict_name: gloss.dict_name,
                        content,
                    }),
                    Err(error) => {
                        tracing::warn!(?error, dictionary = %gloss.dict_name, "dropping a glossary that is not valid JSON");
                        None
                    }
                }
            })
            .collect(),
    }
}

/// JavaScript counts string offsets in UTF-16 code units and Rust indexes bytes,
/// so the two have to be reconciled explicitly. They agree for ordinary Japanese
/// and disagree the moment an emoji shows up in a line.
fn byte_offset_for_utf16(text: &str, offset: usize) -> Option<usize> {
    if offset == 0 {
        return Some(0);
    }
    let mut units = 0usize;
    for (byte_index, character) in text.char_indices() {
        if units == offset {
            return Some(byte_index);
        }
        units += character.len_utf16();
    }
    (units == offset).then_some(text.len())
}

// ── licenses ───────────────────────────────────────────────────────────

async fn license_list(State(state): State<Arc<AppState>>) -> Json<Vec<licenses::DictionaryLicense>> {
    let names: Vec<String> = state
        .pool
        .loaded()
        .iter()
        .map(|entry| entry.name.clone())
        .collect();
    Json(licenses::resolve(&names, &state.data_dir))
}

// ── plumbing ───────────────────────────────────────────────────────────

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

fn bad_request(message: &str) -> (StatusCode, Json<ErrorBody>) {
    (
        StatusCode::BAD_REQUEST,
        Json(ErrorBody {
            error: message.to_string(),
        }),
    )
}

fn env_path(name: &str, fallback: &str) -> PathBuf {
    PathBuf::from(std::env::var(name).unwrap_or_else(|_| fallback.to_string()))
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutting down");
}

#[cfg(test)]
mod tests {
    use super::byte_offset_for_utf16;

    #[test]
    fn maps_utf16_offsets_onto_bytes() {
        let text = "食べさせられなかった";
        assert_eq!(byte_offset_for_utf16(text, 0), Some(0));
        // Every character here is three bytes and one UTF-16 unit.
        assert_eq!(byte_offset_for_utf16(text, 1), Some(3));
        assert_eq!(byte_offset_for_utf16(text, 10), Some(text.len()));
        assert_eq!(byte_offset_for_utf16(text, 11), None);
    }

    #[test]
    fn counts_a_surrogate_pair_as_two_units() {
        // 𠮟 is outside the BMP: two UTF-16 units, four bytes.
        let text = "𠮟る";
        assert_eq!(byte_offset_for_utf16(text, 0), Some(0));
        assert_eq!(byte_offset_for_utf16(text, 2), Some(4));
        assert_eq!(byte_offset_for_utf16(text, 3), Some(text.len()));
        // An offset landing inside the pair is not a character boundary.
        assert_eq!(byte_offset_for_utf16(text, 1), None);
    }

    #[test]
    fn an_empty_string_only_has_offset_zero() {
        assert_eq!(byte_offset_for_utf16("", 0), Some(0));
        assert_eq!(byte_offset_for_utf16("", 1), None);
    }
}
