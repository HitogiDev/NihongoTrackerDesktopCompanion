//! Extensions: Tachiyomi/Mihon sources through Suwayomi-Server, with the
//! Keiyoushi store.
//!
//! Two modes (`Settings::manga_ext_mode`):
//!
//! - **managed**: the app downloads Suwayomi-Server and a Java runtime once
//!   the user agrees (see `install`; nothing is downloaded or started
//!   before), runs it headless on 127.0.0.1 behind basic auth (see
//!   `process`), only when extensions are used, and stops it after
//!   `manga_ext_idle_minutes` without use and when the app closes;
//! - **custom**: the user's own server, at a URL with optional basic auth.
//!
//! Each mode is a row of `manga_sources` (`suwayomi-managed`,
//! `suwayomi-custom`) whose `auth_json` holds the credentials: Rust only,
//! like a WebDAV server's. The UI never sees the managed server's port, its
//! credentials or any server URL: images are tokens served by `manga://`.
//!
//! Events: `manga://ext-status` (`ExtStatus`) when the server's state or the
//! installation changes.

pub mod assets;
pub mod catalog;
pub mod graphql;
pub mod install;
pub mod library;
pub mod process;

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use self::graphql::{Server, SwError};
use crate::db::Db;
use crate::manga::webdav::client::{parse_base_url, Auth};
use crate::state::AppState;

pub const STATUS_EVENT: &str = "manga://ext-status";
/// `manga_sources.kind` of both modes, and `manga_series.source`.
pub const SOURCE_KIND: &str = "suwayomi";
pub const MANAGED_SOURCE: &str = "suwayomi-managed";
pub const CUSTOM_SOURCE: &str = "suwayomi-custom";
/// Keiyoushi's store index, as its `repo.json` names it (`index_v2`), and
/// the same file straight from GitHub's raw host if that one cannot be
/// reached.
pub const KEIYOUSHI_INDEX: &str = "https://github.com/keiyoushi/extensions/raw/repo/index.pb";
const KEIYOUSHI_INDEX_RAW: &str = "https://raw.githubusercontent.com/keiyoushi/extensions/repo/index.pb";
/// How long the first start may take (it creates its database).
const START_TIMEOUT: Duration = Duration::from_secs(180);

/// Where extensions run.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub enum ExtMode {
    #[default]
    Managed,
    Custom,
}

impl ExtMode {
    pub fn source_id(self) -> &'static str {
        match self {
            ExtMode::Managed => MANAGED_SOURCE,
            ExtMode::Custom => CUSTOM_SOURCE,
        }
    }
}

/// One part of the managed server, for the consent dialog.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct ExtComponent {
    /// `server` or `java`.
    pub kind: String,
    pub version: String,
    /// Over the network.
    pub bytes: u64,
    /// On disk once installed (unpacked).
    pub size: u64,
    pub present: bool,
    pub license: String,
}

/// The installation while it runs, and how it ended.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct ExtInstall {
    /// `downloading`, `verifying`, `unpacking`, `done`, `failed`, `cancelled`.
    pub phase: String,
    pub file: String,
    pub received: u64,
    pub total: u64,
    pub error: Option<String>,
}

/// The extensions server as the UI shows it. No URL, port or credentials.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct ExtStatus {
    pub mode: ExtMode,
    /// `not-installed`, `installing`, `not-configured`, `stopped`,
    /// `starting`, `ready` or `error`.
    pub phase: String,
    pub error: Option<String>,
    /// The managed server and its runtime are on disk.
    pub installed: bool,
    /// Adoptium builds a runtime for this computer.
    pub supported: bool,
    /// A custom server has been saved.
    pub custom_configured: bool,
    /// The Keiyoushi store is on the server (once it was asked).
    pub store_ready: Option<bool>,
    /// The server's version, once it answered.
    pub version: Option<String>,
    pub install: Option<ExtInstall>,
    pub components: Vec<ExtComponent>,
    pub download_bytes: u64,
    pub disk_bytes: u64,
    pub installed_bytes: u64,
    pub free_bytes: Option<u64>,
    /// Where the managed server lives on this computer.
    pub dir: String,
}

#[derive(Debug, Clone, PartialEq)]
enum Phase {
    Stopped,
    Starting,
    Ready,
    Failed(&'static str),
}

struct Running {
    mode: ExtMode,
    server: Server,
    child: Option<Child>,
}

pub struct Service {
    pub dir: PathBuf,
    db: Arc<Db>,
    app: Option<AppHandle>,
    running: tokio::sync::Mutex<Option<Running>>,
    phase: Mutex<Phase>,
    version: Mutex<Option<String>>,
    store_ready: Mutex<Option<bool>>,
    install: Mutex<Option<ExtInstall>>,
    installing: AtomicBool,
    cancel_install: Arc<AtomicBool>,
    last_used: Mutex<Instant>,
    /// Image token → (source row, server path).
    images: Mutex<HashMap<String, (String, String)>>,
    /// The child's pid while one runs, so closing the app can stop it even
    /// while a start holds `running`.
    child_pid: Mutex<Option<u32>>,
}

/// At most this many image tokens are remembered; past it the oldest
/// listing's go (the UI asks again).
const MAX_IMAGES: usize = 20_000;

impl Service {
    pub fn new(dir: PathBuf, db: Arc<Db>, app: Option<AppHandle>) -> Self {
        Service {
            dir,
            db,
            app,
            running: tokio::sync::Mutex::new(None),
            phase: Mutex::new(Phase::Stopped),
            version: Mutex::new(None),
            store_ready: Mutex::new(None),
            install: Mutex::new(None),
            installing: AtomicBool::new(false),
            cancel_install: Arc::new(AtomicBool::new(false)),
            last_used: Mutex::new(Instant::now()),
            images: Mutex::new(HashMap::new()),
            child_pid: Mutex::new(None),
        }
    }

    fn data_dir(&self) -> PathBuf {
        self.dir.join("data")
    }

    pub fn touch(&self) {
        *self.last_used.lock().unwrap() = Instant::now();
    }

    fn set_phase(&self, phase: Phase) {
        let changed = {
            let mut current = self.phase.lock().unwrap();
            let changed = *current != phase;
            *current = phase;
            changed
        };
        if changed {
            self.emit();
        }
    }

    fn emit(&self) {
        if let Some(app) = &self.app {
            emit_status(app);
        }
    }

    // ── status ─────────────────────────────────────────────────────────

    fn components(&self) -> Vec<ExtComponent> {
        let server_present = install::server_present(&self.dir);
        let mut list = vec![ExtComponent {
            kind: "server".into(),
            version: assets::SERVER_VERSION.into(),
            bytes: if server_present { 0 } else { assets::SERVER.size },
            size: assets::SERVER.size,
            present: server_present,
            license: "MPL-2.0".into(),
        }];
        if let Some(runtime) = assets::runtime() {
            let present = install::java_path(&self.dir, &runtime).is_some();
            list.push(ExtComponent {
                kind: "java".into(),
                version: runtime.release.into(),
                bytes: if present { 0 } else { runtime.archive.size },
                size: (runtime.archive.size as f64 * assets::RUNTIME_UNPACKED_FACTOR) as u64,
                present,
                license: "GPLv2+CE".into(),
            });
        }
        list
    }

    pub fn status(&self, mode: ExtMode) -> ExtStatus {
        let installed = install::installed(&self.dir);
        let installing = self.installing.load(Ordering::SeqCst);
        let custom_configured = self.db.manga_source_row(CUSTOM_SOURCE).ok().flatten().is_some_and(|row| !row.url.is_empty());
        let phase = self.phase.lock().unwrap().clone();
        let (phase, error) = match (mode, &phase) {
            (ExtMode::Managed, _) if installing => ("installing", None),
            (ExtMode::Managed, _) if !installed => ("not-installed", None),
            (ExtMode::Custom, _) if !custom_configured => ("not-configured", None),
            (_, Phase::Starting) => ("starting", None),
            (_, Phase::Ready) => ("ready", None),
            (_, Phase::Failed(code)) => ("error", Some(code.to_string())),
            (_, Phase::Stopped) => ("stopped", None),
        };
        let components = self.components();
        ExtStatus {
            mode,
            phase: phase.into(),
            error,
            installed,
            supported: assets::runtime().is_some(),
            custom_configured,
            store_ready: *self.store_ready.lock().unwrap(),
            version: self.version.lock().unwrap().clone().filter(|_| phase == "ready"),
            install: self.install.lock().unwrap().clone(),
            download_bytes: install::missing_bytes(&self.dir),
            disk_bytes: components.iter().map(|c| c.size).sum(),
            components,
            installed_bytes: install::installed_bytes(&self.dir),
            free_bytes: crate::manga::ocr::download::free_space(&self.dir),
            dir: self.dir.to_string_lossy().into_owned(),
        }
    }

    // ── installation ───────────────────────────────────────────────────

    /// Downloads and unpacks the managed server on a thread of its own. Only
    /// called once the user agreed in the dialog. `false` if one runs.
    pub fn install(self: &Arc<Self>) -> bool {
        if assets::runtime().is_none() || self.installing.swap(true, Ordering::SeqCst) {
            return false;
        }
        self.cancel_install.store(false, Ordering::SeqCst);
        let service = self.clone();
        std::thread::spawn(move || {
            let total = install::missing_bytes(&service.dir);
            let mut done = 0u64;
            let mut current = String::new();
            let mut last_emit = Instant::now() - Duration::from_secs(1);
            let cancel = service.cancel_install.clone();
            let result = install::install(&service.dir, &cancel, &mut |step, file, received| {
                if current != file {
                    if !current.is_empty() {
                        done += if current == assets::SERVER.file { assets::SERVER.size } else { 0 };
                    }
                    current = file.to_string();
                }
                let phase = match step {
                    install::Step::Downloading => "downloading",
                    install::Step::Verifying => "verifying",
                    install::Step::Unpacking => "unpacking",
                };
                *service.install.lock().unwrap() = Some(ExtInstall {
                    phase: phase.into(),
                    file: file.into(),
                    received: (done + received).min(total),
                    total,
                    error: None,
                });
                if step != install::Step::Downloading || last_emit.elapsed() >= Duration::from_millis(250) {
                    last_emit = Instant::now();
                    service.emit();
                }
            });
            let (phase, error) = match &result {
                Ok(()) => ("done", None),
                Err(crate::manga::ocr::download::FetchError::Cancelled) => ("cancelled", None),
                Err(error) => ("failed", Some(error.code().to_string())),
            };
            if let Err(error) = &result {
                log::warn!("Suwayomi: installation stopped: {error}");
            }
            *service.install.lock().unwrap() = Some(ExtInstall { phase: phase.into(), file: String::new(), received: if result.is_ok() { total } else { done }, total, error });
            service.installing.store(false, Ordering::SeqCst);
            service.emit();
        });
        self.emit();
        true
    }

    pub fn cancel_install(&self) -> bool {
        self.cancel_install.store(true, Ordering::SeqCst);
        self.installing.load(Ordering::SeqCst)
    }

    /// Stops the server and deletes everything it put on disk.
    pub fn uninstall(&self) -> Result<(), SwError> {
        if self.installing.load(Ordering::SeqCst) {
            return Err(SwError::Busy);
        }
        self.stop_blocking();
        install::delete_all(&self.dir).map_err(|_| SwError::Disk)?;
        *self.install.lock().unwrap() = None;
        *self.store_ready.lock().unwrap() = None;
        self.set_phase(Phase::Stopped);
        self.emit();
        Ok(())
    }

    // ── the server ─────────────────────────────────────────────────────

    /// The managed server's credentials, made up the first time and kept
    /// with its source row.
    fn managed_auth(&self) -> Result<Auth, SwError> {
        let disk = |_| SwError::Disk;
        if self.db.manga_source_row(MANAGED_SOURCE).map_err(disk)?.is_none() {
            self.db.add_manga_source(MANAGED_SOURCE, SOURCE_KIND, "Suwayomi", "managed").map_err(disk)?;
        }
        let auth = Auth::from_json(self.db.manga_source_auth(MANAGED_SOURCE).map_err(disk)?.as_deref());
        if matches!(auth, Auth::Basic { .. }) {
            return Ok(auth);
        }
        let auth = random_auth();
        self.db.set_manga_source_auth(MANAGED_SOURCE, auth.to_json().as_deref()).map_err(disk)?;
        Ok(auth)
    }

    /// The custom server as saved, if it is.
    fn custom_server(&self) -> Result<Server, SwError> {
        let row = self.db.manga_source_row(CUSTOM_SOURCE).map_err(|_| SwError::Disk)?.ok_or(SwError::NotConfigured)?;
        let (url, _) = parse_base_url(&row.url).map_err(|_| SwError::NotConfigured)?;
        Ok(Server::new(url, Auth::from_json(row.auth_json.as_deref()), false))
    }

    /// A server ready to answer, in `mode`: the custom one, or the managed
    /// one, started (and waited for) if it is not running.
    pub async fn connect(&self, mode: ExtMode) -> Result<Server, SwError> {
        self.touch();
        let mut running = self.running.lock().await;
        if let Some(current) = running.as_mut() {
            let alive = current.child.as_mut().is_none_or(|child| matches!(child.try_wait(), Ok(None)));
            if current.mode == mode && alive {
                return Ok(current.server.clone());
            }
        }
        // Another mode, or a server that died: start over.
        if let Some(mut old) = running.take() {
            if let Some(child) = old.child.as_mut() {
                process::stop(&self.dir, child);
            }
            *self.child_pid.lock().unwrap() = None;
        }
        *self.store_ready.lock().unwrap() = None;
        let started = match mode {
            ExtMode::Custom => {
                let server = self.custom_server()?;
                self.set_phase(Phase::Starting);
                match server.version().await {
                    Ok(version) => Ok(Running { mode, server, child: None }).map(|r| (r, version)),
                    Err(error) => Err(error),
                }
            }
            ExtMode::Managed => self.start_managed().await,
        };
        match started {
            Ok((current, version)) => {
                let server = current.server.clone();
                *running = Some(current);
                *self.version.lock().unwrap() = Some(version);
                drop(running);
                if mode == ExtMode::Managed {
                    self.ensure_store(&server).await;
                } else {
                    let ready = server.stores().await.ok().map(|stores| stores.iter().any(|url| is_keiyoushi(url)));
                    *self.store_ready.lock().unwrap() = ready;
                }
                self.set_phase(Phase::Ready);
                Ok(server)
            }
            Err(error) => {
                self.set_phase(Phase::Failed(error.code()));
                Err(error)
            }
        }
    }

    async fn start_managed(&self) -> Result<(Running, String), SwError> {
        if !install::installed(&self.dir) {
            return Err(SwError::NotInstalled);
        }
        let runtime = assets::runtime().ok_or(SwError::Unsupported)?;
        let java = install::java_path(&self.dir, &runtime).ok_or(SwError::NotInstalled)?;
        let Auth::Basic { username, password } = self.managed_auth()? else { return Err(SwError::Disk) };
        self.set_phase(Phase::Starting);
        let dir = self.dir.clone();
        let jar = install::jar_path(&self.dir);
        let root = self.data_dir();
        let launch = tauri::async_runtime::spawn_blocking(move || -> Result<(Child, u16), SwError> {
            process::stop_leftover(&dir, assets::SERVER.file);
            std::fs::create_dir_all(&root).map_err(|_| SwError::Disk)?;
            let port = process::free_port().map_err(|_| SwError::StartFailed)?;
            let launch = process::Launch { java, jar, root, port, username: username.clone(), password: password.clone() };
            let child = process::spawn(&dir, &launch).map_err(|error| {
                log::warn!("Suwayomi: java did not start: {error}");
                SwError::StartFailed
            })?;
            Ok((child, port))
        })
        .await
        .map_err(|_| SwError::StartFailed)??;
        let (mut child, port) = launch;
        *self.child_pid.lock().unwrap() = Some(child.id());
        let url = reqwest::Url::parse(&format!("http://127.0.0.1:{port}/")).map_err(|_| SwError::StartFailed)?;
        let auth = self.managed_auth()?;
        let server = Server::new(url, auth, true);
        let started = Instant::now();
        let version = loop {
            if let Ok(Some(status)) = child.try_wait() {
                log::warn!("Suwayomi exited while starting ({status}):\n{}", process::log_tail(&self.dir, 20));
                let _ = std::fs::remove_file(process::argfile_path(&self.dir));
                *self.child_pid.lock().unwrap() = None;
                return Err(SwError::StartFailed);
            }
            if let Ok(version) = server.version().await {
                break version;
            }
            if started.elapsed() > START_TIMEOUT {
                log::warn!("Suwayomi did not answer in time:\n{}", process::log_tail(&self.dir, 20));
                process::stop(&self.dir, &mut child);
                *self.child_pid.lock().unwrap() = None;
                return Err(SwError::StartFailed);
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        };
        // Read: the password no longer needs to be on disk.
        let _ = std::fs::remove_file(process::argfile_path(&self.dir));
        log::info!("Suwayomi {version} is running");
        Ok((Running { mode: ExtMode::Managed, server, child: Some(child) }, version))
    }

    /// Adds Keiyoushi's store to the managed server the first time.
    async fn ensure_store(&self, server: &Server) {
        let ready = match server.stores().await {
            Ok(stores) if stores.iter().any(|url| is_keiyoushi(url)) => true,
            Ok(_) => self.add_store(server).await.is_ok(),
            Err(_) => false,
        };
        *self.store_ready.lock().unwrap() = Some(ready);
    }

    pub async fn add_store(&self, server: &Server) -> Result<(), SwError> {
        let result = match server.add_store(KEIYOUSHI_INDEX).await {
            Ok(()) => Ok(()),
            Err(_) => server.add_store(KEIYOUSHI_INDEX_RAW).await,
        };
        *self.store_ready.lock().unwrap() = Some(result.is_ok());
        result
    }

    /// Stops the managed server (and forgets any connection). Blocking.
    pub fn stop_blocking(&self) {
        let taken = self.running.try_lock().ok().and_then(|mut running| running.take());
        match taken {
            Some(mut running) => {
                if let Some(child) = running.child.as_mut() {
                    process::stop(&self.dir, child);
                }
            }
            // A start is holding it: stop the process by its pid.
            None => process::stop_leftover(&self.dir, assets::SERVER.file),
        }
        *self.child_pid.lock().unwrap() = None;
        *self.version.lock().unwrap() = None;
        self.set_phase(Phase::Stopped);
    }

    /// The managed server is running and nobody used it for `idle`.
    pub fn idle_for(&self, idle: Duration) -> bool {
        self.child_pid.lock().unwrap().is_some() && self.last_used.lock().unwrap().elapsed() >= idle
    }

    // ── images ─────────────────────────────────────────────────────────

    /// A token for an image the server has at `path`, or `None` for a path
    /// that is not one of its own.
    pub fn image_token(&self, source_row: &str, path: &str) -> Option<String> {
        let path = graphql::server_path(path)?;
        let token = image_token(source_row, &path);
        let mut images = self.images.lock().unwrap();
        if images.len() >= MAX_IMAGES {
            images.clear();
        }
        images.insert(token.clone(), (source_row.to_string(), path));
        Some(token)
    }

    pub fn image_path(&self, token: &str) -> Option<(String, String)> {
        self.images.lock().unwrap().get(token).cloned()
    }

    /// Bytes behind a server path of a source row, from the server of that
    /// row's mode (if it is the current one). Blocking.
    pub fn fetch_blocking(&self, source_row: &str, path: &str, max: u64) -> Result<Vec<u8>, SwError> {
        let mode = self.mode_blocking();
        if mode.source_id() != source_row {
            return Err(SwError::NotFound);
        }
        tauri::async_runtime::block_on(async {
            let server = self.connect(mode).await?;
            server.bytes(path, max).await
        })
    }

    /// The mode chosen in settings, waiting for the settings lock (callers
    /// are blocking threads, never the async runtime).
    pub fn mode_blocking(&self) -> ExtMode {
        match &self.app {
            Some(app) => app.state::<AppState>().settings.blocking_read().manga_ext_mode,
            None => ExtMode::Managed,
        }
    }
}

/// Basic-auth credentials nobody can guess: 122 random bits each half.
fn random_auth() -> Auth {
    Auth::Basic {
        username: format!("ntdc-{}", &uuid::Uuid::new_v4().simple().to_string()[..12]),
        password: format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple()),
    }
}

fn is_keiyoushi(url: &str) -> bool {
    url.contains("keiyoushi/extensions")
}

/// The same server path of the same source row is always the same token.
pub fn image_token(source_row: &str, path: &str) -> String {
    uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, format!("suwayomi-image\0{source_row}\0{path}").as_bytes()).simple().to_string()
}

pub fn emit_status(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Some(service) = state.manga.suwayomi() else { return };
    let Ok(settings) = state.settings.try_read() else { return };
    let mode = settings.manga_ext_mode;
    drop(settings);
    let _ = app.emit(STATUS_EVENT, service.status(mode));
}

/// Stops the managed server once it has gone unused for the minutes in
/// settings (0 never). Checked every half minute on a thread of its own.
pub fn start_idle_watch(app: &AppHandle) {
    let app = app.clone();
    let _ = std::thread::Builder::new().name("suwayomi-idle".into()).spawn(move || loop {
        std::thread::sleep(Duration::from_secs(30));
        let state = app.state::<AppState>();
        let Some(service) = state.manga.suwayomi() else { continue };
        let minutes = state.settings.blocking_read().manga_ext_idle_minutes;
        if minutes > 0 && service.idle_for(Duration::from_secs(minutes * 60)) {
            log::info!("Suwayomi: unused for {minutes} min, stopping it");
            service.stop_blocking();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manga::store::tests::temp_db;

    #[test]
    fn credentials_never_reach_the_ui() {
        let (db, path) = temp_db();
        let db = Arc::new(db);
        let dir = std::env::temp_dir().join(format!("ntdc-swy-status-{}", uuid::Uuid::new_v4()));
        let service = Service::new(dir.clone(), db.clone(), None);
        let Auth::Basic { username, password } = service.managed_auth().unwrap() else { panic!("basic") };
        assert!(password.len() >= 64);
        // Kept: the same ones next time.
        assert_eq!(service.managed_auth().unwrap(), Auth::Basic { username: username.clone(), password: password.clone() });

        db.add_manga_source(CUSTOM_SOURCE, SOURCE_KIND, "Mine", "https://example.org/suwayomi/").unwrap();
        let custom = Auth::Basic { username: "me".into(), password: "custom-secret".into() };
        db.set_manga_source_auth(CUSTOM_SOURCE, custom.to_json().as_deref()).unwrap();
        let token = service.image_token(MANAGED_SOURCE, "/api/v1/manga/2/thumbnail").unwrap();

        let mut shown = serde_json::to_string(&service.status(ExtMode::Managed)).unwrap();
        shown += &serde_json::to_string(&service.status(ExtMode::Custom)).unwrap();
        shown += &serde_json::to_string(&db.manga_sources().unwrap()).unwrap();
        for secret in [password.as_str(), username.as_str(), "custom-secret", "/api/v1/manga/2/thumbnail"] {
            assert!(!shown.contains(secret), "{secret} reached the UI");
        }
        assert!(shown.contains("\"auth\":\"basic\""));
        assert!(crate::manga::protocol::is_opaque_id(&token));
        assert_eq!(service.image_path(&token).unwrap().1, "/api/v1/manga/2/thumbnail");
        assert!(service.image_token(MANAGED_SOURCE, "https://elsewhere.example/x.png").is_none());
        let status = service.status(ExtMode::Managed);
        assert_eq!(status.phase, "not-installed");
        assert_eq!(service.status(ExtMode::Custom).phase, "stopped");
        assert_eq!(status.components.len(), if assets::runtime().is_some() { 2 } else { 1 });
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The real thing, run by hand: the pinned jar and JRE (already
    /// downloaded, in `NTDC_SWY_JAR` and `NTDC_SWY_JRE`) are checked and put
    /// in `NTDC_SWY_DIR`, the server starts headless, gets the Keiyoushi
    /// store, installs `NTDC_SWY_EXT` (a SAFE `ja` extension), searches
    /// `NTDC_SWY_QUERY` and reads one page of its first free chapter.
    #[test]
    #[ignore]
    fn runs_the_real_server() {
        use crate::manga::ocr::engine::sha256_file;
        let env = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("{name}"));
        let dir = PathBuf::from(env("NTDC_SWY_DIR"));
        let jar = PathBuf::from(env("NTDC_SWY_JAR"));
        let jre = PathBuf::from(env("NTDC_SWY_JRE"));
        let runtime = assets::runtime().unwrap();
        assert_eq!(sha256_file(&jar).unwrap(), assets::SERVER.sha256);
        assert_eq!(sha256_file(&jre).unwrap(), runtime.archive.sha256);
        std::fs::create_dir_all(dir.join(install::BIN_DIR)).unwrap();
        std::fs::copy(&jar, install::jar_path(&dir)).unwrap();
        if install::java_path(&dir, &runtime).is_none() {
            install::unpack_runtime(&jre, &install::runtime_dir_for_test(&dir, &runtime), runtime.zip, runtime.archive.sha256).unwrap();
        }
        assert!(install::installed(&dir));

        let (db, db_path) = temp_db();
        let db = Arc::new(db);
        let service = Service::new(dir.clone(), db.clone(), None);
        tauri::async_runtime::block_on(async {
            let started = Instant::now();
            let server = service.connect(ExtMode::Managed).await.unwrap();
            println!("started in {:?}, version {:?}, store {:?}", started.elapsed(), service.version.lock().unwrap(), service.store_ready.lock().unwrap());
            assert_eq!(*service.store_ready.lock().unwrap(), Some(true));
            let extensions = server.fetch_extensions().await.unwrap();
            let shown = catalog::extensions(extensions.clone(), false, &mut |_| None);
            println!("{} extensions, {} ja shown by default", extensions.len(), shown.iter().filter(|e| e.lang == "ja").count());
            assert!(shown.iter().all(|e| e.content == "safe"));
            let wanted = env("NTDC_SWY_EXT");
            assert!(shown.iter().any(|e| e.pkg_name == wanted));
            server.update_extension(&wanted, graphql::ExtensionAction::Install).await.unwrap();
            let sources = catalog::sources(server.sources().await.unwrap(), false, &mut |_| None);
            let source = sources.iter().find(|s| s.pkg_name.as_deref() == Some(wanted.as_str())).unwrap();
            let found = server.browse(&source.id, graphql::BrowseKind::Search, 1, Some(&env("NTDC_SWY_QUERY"))).await.unwrap();
            println!("{} results", found.mangas.len());
            let manga = &found.mangas[0];
            let id = library::add(&db, &server, MANAGED_SOURCE, manga.id, false).await.unwrap();
            let series = db.manga_series(&id).unwrap().unwrap();
            assert_eq!(series.source, SOURCE_KIND);
            let (_, chapters) = server.manga(manga.id, false).await.unwrap();
            let free = chapters.iter().find(|c| !c.name.contains('\u{1f4b4}')).expect("a free chapter");
            let pages = server.chapter_pages(free.id).await.unwrap();
            println!("{} — {}: {} pages", series.title, free.name, pages.len());
            let bytes = server.bytes(&pages[0], 64 * 1024 * 1024).await.unwrap();
            assert_eq!(crate::manga::protocol::sniff(&bytes).split('/').next(), Some("image"));
            println!("page 0: {} bytes, {}", bytes.len(), crate::manga::protocol::sniff(&bytes));
        });
        service.stop_blocking();
        assert!(!process::pid_path(&dir).exists());
        let _ = std::fs::remove_file(db_path);
    }
}
