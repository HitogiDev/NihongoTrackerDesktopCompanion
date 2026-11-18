//! The built-in OCR: text for manga pages that have none, read on this
//! computer by ONNX models (see `engine`), on a thread of its own with a
//! queue (see `worker`).
//!
//! What it reads is stored as `.mokuro` pages in `manga_page_text` (engine
//! `builtin`), which `mokuro::volume_text` uses for the pages a `.mokuro`
//! does not cover. The reader asks for the pages it shows when they have no
//! text; "process volume" queues every page without text.
//!
//! Events: `manga://ocr-status` (`OcrStatus`) whenever the engine or a
//! volume run changes, `manga://ocr-page` (`OcrPage`) when a page gets text.

pub mod assets;
pub mod beam;
pub mod download;
pub mod engine;
pub mod export;
pub mod layout;
pub mod owocr;
pub mod text;
pub mod worker;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use self::assets::Variant;
use self::worker::{OcrVolumeRun, Priority, Queue, Shared, Work};
use super::mokuro::MokuroPage;
use super::store::ENGINE_BUILTIN;
use crate::state::AppState;

pub const STATUS_EVENT: &str = "manga://ocr-status";
/// `OcrDownload`: the runtime and models coming down.
pub const PROGRESS_EVENT: &str = "manga://ocr-progress";
pub const PAGE_EVENT: &str = "manga://ocr-page";

/// The OCR as the UI shows it.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct OcrStatus {
    /// ONNX Runtime has a build for this computer.
    pub supported: bool,
    /// The runtime and the chosen variant's models are on disk.
    pub installed: bool,
    pub variant: Variant,
    /// What the accelerator would be (`directml`, `coreml`), if any.
    pub accelerator: Option<String>,
    /// Where the loaded engine runs now; `None` while it is not loaded.
    pub provider: Option<String>,
    pub loading: bool,
    /// A page is being read.
    pub busy: bool,
    pub error: Option<String>,
    /// Volumes being processed whole.
    pub runs: Vec<OcrVolumeRun>,
    /// For the consent dialog: what would be downloaded, and where.
    pub components: Vec<OcrComponent>,
    /// Bytes still to download for the chosen variant.
    pub download_bytes: u64,
    /// Bytes the OCR's files take on disk now.
    pub installed_bytes: u64,
    /// Free space on the disk they go to.
    pub free_bytes: Option<u64>,
    pub dir: String,
    pub runtime_version: String,
    /// A download in progress (or how the last one ended).
    pub download: Option<download::Progress>,
}

/// One part of the OCR, as the consent dialog lists it.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct OcrComponent {
    /// `runtime`, `detector` or `recognizer`.
    pub kind: String,
    /// Over the network.
    pub bytes: u64,
    /// On disk.
    pub size: u64,
    pub present: bool,
    pub license: String,
}

/// A page that just got text.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct OcrPage {
    pub volume_id: String,
    pub page: i64,
    /// `false` when it could not be read.
    pub ok: bool,
}

pub struct Service {
    pub dir: PathBuf,
    shared: Shared,
    started: AtomicBool,
    download: Mutex<Option<download::Progress>>,
    downloading: AtomicBool,
    cancel_download: Arc<AtomicBool>,
}

/// Left in `ocr/` when "delete" could not remove a file that was in use (a
/// loaded library on Windows): the next start finishes the job.
const DELETE_PENDING: &str = ".delete-pending";

impl Service {
    pub fn new(dir: PathBuf) -> Self {
        if dir.join(DELETE_PENDING).exists() && download::delete_all(&dir).is_empty() {
            let _ = std::fs::remove_file(dir.join(DELETE_PENDING));
        }
        Service {
            dir,
            shared: Arc::new((Mutex::new(Queue::default()), Condvar::new())),
            started: AtomicBool::new(false),
            download: Mutex::new(None),
            downloading: AtomicBool::new(false),
            cancel_download: Arc::new(AtomicBool::new(false)),
        }
    }

    fn components(&self, variant: Variant) -> Vec<OcrComponent> {
        let group = |kind: &str, assets: &[assets::Asset], license: &str| OcrComponent {
            kind: kind.to_string(),
            bytes: assets.iter().filter(|asset| !asset.present(&self.dir)).map(|asset| asset.transfer_size()).sum(),
            size: assets.iter().map(|asset| asset.size).sum(),
            present: assets.iter().all(|asset| asset.present(&self.dir)),
            license: license.to_string(),
        };
        let mut list = Vec::new();
        if let Some(runtime) = assets::runtime() {
            list.push(group("runtime", runtime, "MIT"));
        }
        list.push(group("detector", &[assets::DETECTOR], "Apache-2.0"));
        let [encoder, decoder] = assets::recognizer(variant);
        list.push(group("recognizer", &[encoder, decoder, assets::VOCAB], "Apache-2.0"));
        list
    }

    fn installed_bytes(&self) -> u64 {
        assets::known_files().iter().filter_map(|file| std::fs::metadata(self.dir.join(file)).ok()).map(|meta| meta.len()).sum()
    }

    /// Downloads what the variant still needs, on a thread of its own, with
    /// progress on `manga://ocr-progress`. The UI asks first: this is only
    /// called once the user has agreed. `false` if one is already running.
    pub fn install(&self, app: &AppHandle, variant: Variant) -> bool {
        let Some(assets) = assets::all(variant) else { return false };
        if self.downloading.swap(true, Ordering::SeqCst) {
            return false;
        }
        self.cancel_download.store(false, Ordering::SeqCst);
        let missing: Vec<assets::Asset> = assets.into_iter().filter(|asset| !asset.present(&self.dir)).collect();
        let total: u64 = missing.iter().map(|asset| asset.transfer_size()).sum();
        let app = app.clone();
        let dir = self.dir.clone();
        let cancel = self.cancel_download.clone();
        std::thread::spawn(move || {
            let state = app.state::<AppState>();
            let Some(service) = state.manga.ocr() else { return };
            let client = download::client();
            let mut done = 0u64;
            let mut last_emit = std::time::Instant::now() - std::time::Duration::from_secs(1);
            let mut outcome: Result<(), download::FetchError> = Ok(());
            for asset in &missing {
                let mut report = |received: u64, phase: &str, force: bool| {
                    let progress = download::Progress {
                        phase: phase.to_string(),
                        file: asset.file.to_string(),
                        received: (done + received).min(total),
                        total,
                        error: None,
                    };
                    *service.download.lock().unwrap() = Some(progress.clone());
                    if force || last_emit.elapsed() >= std::time::Duration::from_millis(200) {
                        last_emit = std::time::Instant::now();
                        let _ = app.emit(PROGRESS_EVENT, progress);
                    }
                };
                report(0, "downloading", true);
                let result = download::fetch(&client, asset, &dir, &cancel, &mut |received| {
                    let phase = if received >= asset.transfer_size() { "verifying" } else { "downloading" };
                    report(received, phase, phase == "verifying");
                });
                if let Err(error) = result {
                    log::warn!("OCR download of {} failed: {error}", asset.file);
                    outcome = Err(error);
                    break;
                }
                done += asset.transfer_size();
            }
            let progress = download::Progress {
                phase: match &outcome {
                    Ok(()) => "done",
                    Err(download::FetchError::Cancelled) => "cancelled",
                    Err(_) => "failed",
                }
                .to_string(),
                file: String::new(),
                received: done,
                total,
                error: outcome.as_ref().err().map(|error| error.code().to_string()),
            };
            *service.download.lock().unwrap() = Some(progress.clone());
            service.downloading.store(false, Ordering::SeqCst);
            if outcome.is_ok() {
                service.reload();
            }
            let _ = app.emit(PROGRESS_EVENT, progress);
            emit_status(&app);
        });
        true
    }

    pub fn cancel_install(&self) -> bool {
        let running = self.downloading.load(Ordering::SeqCst);
        self.cancel_download.store(true, Ordering::SeqCst);
        running
    }

    /// Deletes the runtime and every model (and unfinished downloads). The
    /// engine goes first; what is still in use goes at the next start.
    pub fn delete(&self) -> Result<(), String> {
        if self.downloading.load(Ordering::SeqCst) {
            return Err("downloading".into());
        }
        self.with_queue(|queue| {
            queue.reload = true;
            queue.runs.clear();
        });
        let left = download::delete_all(&self.dir);
        *self.download.lock().unwrap() = None;
        if !left.is_empty() {
            log::info!("OCR files in use, deleted at the next start: {left:?}");
            std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
            std::fs::write(self.dir.join(DELETE_PENDING), left.join("\n")).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Everything the chosen variant needs is here.
    pub fn installed(&self, variant: Variant) -> bool {
        assets::all(variant).is_some_and(|assets| assets.iter().all(|asset| asset.present(&self.dir)))
    }

    pub fn status(&self, variant: Variant) -> OcrStatus {
        let queue = self.shared.0.lock().unwrap();
        let mut runs: Vec<OcrVolumeRun> = queue.runs.values().cloned().collect();
        runs.sort_by(|a, b| a.title.cmp(&b.title));
        OcrStatus {
            supported: assets::runtime().is_some(),
            installed: self.installed(variant),
            variant,
            accelerator: crate::platform::ocr_accelerator().map(str::to_string),
            provider: queue.provider.map(str::to_string),
            loading: queue.loading,
            busy: queue.current.is_some(),
            error: queue.error.clone(),
            runs,
            components: self.components(variant),
            download_bytes: assets::all(variant)
                .map(|assets| assets.iter().filter(|asset| !asset.present(&self.dir)).map(|asset| asset.transfer_size()).sum())
                .unwrap_or(0),
            installed_bytes: self.installed_bytes(),
            free_bytes: download::free_space(&self.dir),
            dir: self.dir.to_string_lossy().into_owned(),
            runtime_version: assets::RUNTIME_VERSION.to_string(),
            download: self.download.lock().unwrap().clone(),
        }
    }

    fn with_queue<T>(&self, f: impl FnOnce(&mut Queue) -> T) -> T {
        let (lock, ready) = &*self.shared;
        let result = f(&mut lock.lock().unwrap());
        ready.notify_all();
        result
    }

    /// The engine is dropped before the next page: its files changed.
    pub fn reload(&self) {
        self.with_queue(|queue| {
            queue.reload = true;
            queue.error = None;
        });
    }

    fn ensure_thread(&self, app: &AppHandle) {
        if self.started.swap(true, Ordering::SeqCst) {
            return;
        }
        let shared = self.shared.clone();
        let app = app.clone();
        let dir = self.dir.clone();
        let spawned = std::thread::Builder::new().name("manga-ocr".into()).spawn(move || run(&app, &shared, &dir));
        if let Err(error) = spawned {
            log::error!("OCR thread did not start: {error}");
            self.started.store(false, Ordering::SeqCst);
        }
    }

    /// The reader shows `visible` and has no text for them: read them now,
    /// and the next pages after. Read-ahead for other volumes is dropped.
    pub fn request_pages(&self, app: &AppHandle, volume_id: &str, visible: &[i64], ahead: &[i64]) {
        self.ensure_thread(app);
        self.with_queue(|queue| {
            queue.drop_ahead_except(volume_id);
            for &page in visible {
                queue.push(volume_id, page, Priority::Visible, Work::Page);
            }
            for &page in ahead {
                queue.push(volume_id, page, Priority::Ahead, Work::Page);
            }
        });
    }

    /// Queues `pages` of a volume as a run the UI follows. Pages already
    /// queued join it.
    pub fn process_volume(&self, app: &AppHandle, volume_id: &str, title: &str, pages: &[i64]) {
        self.ensure_thread(app);
        self.with_queue(|queue| {
            for &page in pages {
                queue.push(volume_id, page, Priority::Volume, Work::Page);
            }
            queue.runs.insert(
                volume_id.to_string(),
                OcrVolumeRun { volume_id: volume_id.to_string(), title: title.to_string(), done: 0, total: pages.len() as i64, failed: 0 },
            );
        });
        emit_status(app);
    }

    /// Stops a run: its pages still waiting are dropped (the one being read
    /// finishes).
    pub fn cancel_volume(&self, app: &AppHandle, volume_id: &str) -> bool {
        let cancelled = self.with_queue(|queue| {
            queue.drop_volume(volume_id, Priority::Volume);
            queue.runs.remove(volume_id).is_some()
        });
        emit_status(app);
        cancelled
    }

    /// Reads the text inside one box of a page, before anything else queued.
    /// Blocks until it is read: call it off the async runtime.
    pub fn recognize_region(&self, app: &AppHandle, volume_id: &str, page: i64, bbox: [f64; 4]) -> Result<String, String> {
        self.ensure_thread(app);
        let (reply, answer) = mpsc::channel();
        self.with_queue(|queue| queue.push(volume_id, page, Priority::Region, Work::Region { bbox, reply }));
        answer.recv().map_err(|_| "error".to_string())?
    }

    /// Pages of a volume waiting in the queue, for the reader.
    pub fn queued_pages(&self, volume_id: &str) -> Vec<i64> {
        let queue = self.shared.0.lock().unwrap();
        let mut pages: Vec<i64> = queue.queued_pages(volume_id).into_iter().collect();
        if let Some((volume, page)) = &queue.current {
            if volume == volume_id {
                pages.push(*page);
            }
        }
        pages.sort_unstable();
        pages.dedup();
        pages
    }

    /// Stops the thread (the app is closing).
    pub fn shutdown(&self) {
        self.with_queue(|queue| queue.stopping = true);
    }
}

fn variant(app: &AppHandle) -> Variant {
    app.state::<AppState>().settings.try_read().map(|settings| settings.manga_ocr_variant).unwrap_or_default()
}

pub fn emit_status(app: &AppHandle) {
    let state = app.state::<AppState>();
    if let Some(service) = state.manga.ocr.get() {
        let _ = app.emit(STATUS_EVENT, service.status(variant(app)));
    }
}

fn set(shared: &Shared, f: impl FnOnce(&mut Queue)) {
    f(&mut shared.0.lock().unwrap());
}

/// The OCR thread: takes jobs until the app closes.
fn run(app: &AppHandle, shared: &Shared, dir: &Path) {
    let mut engine: Option<engine::Engine> = None;
    loop {
        let job = match worker::next(shared, engine.is_none()) {
            Ok(job) => job,
            Err(true) => return,
            Err(false) => {
                // Nothing to do for a while: give the memory back.
                engine = None;
                set(shared, |queue| queue.provider = None);
                emit_status(app);
                continue;
            }
        };
        let reload = std::mem::take(&mut shared.0.lock().unwrap().reload);
        if reload {
            engine = None;
        }
        if engine.is_none() {
            set(shared, |queue| {
                queue.loading = true;
                queue.provider = None;
            });
            emit_status(app);
            let loaded = engine::Engine::load(dir, variant(app));
            match loaded {
                Ok(loaded) => {
                    let provider = loaded.provider.name();
                    log::info!("OCR ready on {provider}");
                    set(shared, |queue| {
                        queue.loading = false;
                        queue.provider = Some(provider);
                        queue.error = None;
                    });
                    engine = Some(loaded);
                }
                Err(error) => {
                    log::warn!("OCR did not load: {error}");
                    // Nothing queued can be read: say so and empty the queue.
                    let mut failed = vec![job];
                    set(shared, |queue| {
                        queue.loading = false;
                        queue.error = Some(error.clone());
                        queue.current = None;
                        queue.runs.clear();
                        failed.extend(std::iter::from_fn(|| queue.pop()));
                    });
                    for job in failed {
                        finish(app, &job, Err(error.clone()));
                    }
                    emit_status(app);
                    continue;
                }
            }
            emit_status(app);
        }
        let engine_ref = engine.as_mut().expect("loaded above");
        let result = read(app, engine_ref, &job);
        set(shared, |queue| {
            queue.current = None;
            // The accelerator may have given way to the CPU.
            queue.provider = Some(engine_ref.provider.name());
            if let Some(run) = queue.runs.get_mut(&job.volume_id).filter(|_| job.priority == Priority::Volume) {
                match &result {
                    Ok(_) => run.done += 1,
                    Err(_) => run.failed += 1,
                }
            }
        });
        finish(app, &job, result);
        let ended = {
            let mut queue = shared.0.lock().unwrap();
            let ended: Vec<OcrVolumeRun> = queue.runs.values().filter(|run| run.done + run.failed >= run.total).cloned().collect();
            for run in &ended {
                queue.runs.remove(&run.volume_id);
            }
            ended
        };
        for run in ended {
            volume_done(app, &run);
        }
        emit_status(app);
    }
}

/// What a job produced: a page's text, or a region's.
enum Read {
    Page(Option<MokuroPage>),
    Region(String),
}

fn read(app: &AppHandle, engine: &mut engine::Engine, job: &worker::Job) -> Result<Read, String> {
    let state = app.state::<AppState>();
    if matches!(job.work, Work::Page) && state.db.manga_page_text_engine(&job.volume_id, job.page).map_err(|e| e.to_string())?.is_some() {
        // Read already (or corrected by hand): nothing to redo.
        return Ok(Read::Page(None));
    }
    let location = state.db.manga_volume_location(&job.volume_id).map_err(|e| e.to_string())?.ok_or("not-found")?;
    let index = usize::try_from(job.page).map_err(|_| "not-found")?;
    let name = location.pages.get(index).ok_or("not-found")?.clone();
    let remote = state.manga.remote();
    let bytes = super::protocol::page_bytes(remote.as_deref(), &location, index).ok_or("unreadable")?;
    let image = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
    match &job.work {
        Work::Page => {
            let page = engine.recognize_page(&image, &name)?;
            let json = serde_json::to_string(&page).map_err(|e| e.to_string())?;
            let had_text = state.db.manga_volume(&job.volume_id).ok().flatten().is_some_and(|volume| volume.ocr_state != "none");
            state.db.set_manga_page_text(&job.volume_id, job.page, &json, ENGINE_BUILTIN).map_err(|e| e.to_string())?;
            if !had_text {
                // The volume has text now: the library shows it.
                let _ = app.emit(super::library::LIBRARY_CHANGED, ());
            }
            Ok(Read::Page(Some(page)))
        }
        Work::Region { bbox, .. } => engine.recognize_box(&image, *bbox).map(Read::Region),
    }
}

fn finish(app: &AppHandle, job: &worker::Job, result: Result<Read, String>) {
    match (&job.work, result) {
        (Work::Region { reply, .. }, Ok(Read::Region(text))) => {
            let _ = reply.send(Ok(text));
        }
        (Work::Region { reply, .. }, Err(error)) => {
            let _ = reply.send(Err(error));
        }
        (Work::Page, Ok(Read::Page(page))) => {
            if let Some(page) = page {
                super::session::page_text_changed(app, &job.volume_id, job.page, Some(page));
            }
            let _ = app.emit(PAGE_EVENT, OcrPage { volume_id: job.volume_id.clone(), page: job.page, ok: true });
        }
        (Work::Page, Err(error)) => {
            log::warn!("OCR of {}/{} failed: {error}", job.volume_id, job.page);
            let _ = app.emit(PAGE_EVENT, OcrPage { volume_id: job.volume_id.clone(), page: job.page, ok: false });
        }
        _ => {}
    }
}

/// A whole volume is read: the library shows it, and a notification says so.
fn volume_done(app: &AppHandle, run: &OcrVolumeRun) {
    let _ = app.emit(super::library::LIBRARY_CHANGED, ());
    let state = app.state::<AppState>();
    let Ok(settings) = state.settings.try_read() else { return };
    if settings.notifications {
        let lang = settings.language.as_str();
        crate::notifications::show(app, crate::i18n::ocr_done_title(lang), &crate::i18n::ocr_done_body(lang, &run.title, run.done, run.failed));
    }
}

/// The pages of a volume with no text at all (no `.mokuro` page, nothing
/// stored), and its title for the UI, as "series — volume".
pub fn pages_without_text(
    db: &crate::db::Db,
    remote: Option<&super::webdav::remote::Remote>,
    volume_id: &str,
) -> Result<(String, Vec<i64>), String> {
    // A remote archive opened for the first time: its pages first.
    if let (Some(remote), Some(location)) = (remote, db.manga_volume_location(volume_id).map_err(|e| e.to_string())?) {
        if super::store::is_remote_format(&location.format) && location.pages.is_empty() {
            remote.ensure_index(&location).map_err(|e| e.code().to_string())?;
        }
    }
    let volume = db.manga_volume(volume_id).map_err(|e| e.to_string())?.ok_or("not-found")?;
    let series = db.manga_series(&volume.series_id).map_err(|e| e.to_string())?.ok_or("not-found")?;
    let text = super::mokuro::volume_text(db, remote, volume_id)?;
    let pages = text.iter().enumerate().filter(|(_, page)| page.is_none()).map(|(index, _)| index as i64).collect();
    Ok((format!("{} — {}", series.title, volume.title), pages))
}

/// Stores a page as the user corrected it (engine `manual`: it wins over a
/// `.mokuro` and the OCR never redoes it), and lets the session count it.
pub fn save_edited_page(app: &AppHandle, volume_id: &str, page: i64, image: (f64, f64), blocks: &[layout::EditedBlock]) -> Result<MokuroPage, String> {
    let state = app.state::<AppState>();
    let location = state.db.manga_volume_location(volume_id).map_err(|e| e.to_string())?.ok_or("not-found")?;
    let name = usize::try_from(page).ok().and_then(|index| location.pages.get(index)).ok_or("not-found")?;
    let edited = layout::edited_page(name, image, blocks)?;
    let json = serde_json::to_string(&edited).map_err(|e| e.to_string())?;
    state.db.set_manga_page_text(volume_id, page, &json, super::store::ENGINE_MANUAL).map_err(|e| e.to_string())?;
    super::session::page_text_changed(app, volume_id, page, Some(edited.clone()));
    let _ = app.emit(PAGE_EVENT, OcrPage { volume_id: volume_id.to_string(), page, ok: true });
    let _ = app.emit(super::library::LIBRARY_CHANGED, ());
    Ok(edited)
}
