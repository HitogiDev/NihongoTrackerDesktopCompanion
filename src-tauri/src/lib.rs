mod api;
mod commands;
mod config;
mod db;
mod discord;
mod i18n;
mod parser;
mod platform;
mod playback;
mod players;
mod state;
mod streak;
mod sync;
mod texthooker;
mod watcher;

use std::sync::Arc;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Manager, WindowEvent};
use tokio::sync::{mpsc, RwLock};

use crate::db::Db;
use crate::state::{AppState, HookerCommand, HookerState, TrackerSnapshot};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }));
        builder = builder.plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ));
    }

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let db = Arc::new(Db::open(&data_dir.join("companion.sqlite3"))?);
            let settings = db.load_settings();
            let _ = db.prune(30);

            let (hooker_tx, hooker_rx) = mpsc::unbounded_channel::<HookerCommand>();

            app.manage(AppState {
                db,
                tracker: RwLock::new(TrackerSnapshot {
                    platform_supported: platform::supported(),
                    ..Default::default()
                }),
                hooker: RwLock::new(HookerState {
                    url: settings.texthooker_ws_url.clone(),
                    ..Default::default()
                }),
                hooker_tx,
                settings: RwLock::new(settings.clone()),
                media_cache: RwLock::new(Default::default()),
                playback: RwLock::new(None),
                playback_probing: Default::default(),
                hooker_timer_active: Default::default(),
                streak: RwLock::new(Default::default()),
                streak_auth_failed: Default::default(),
                discord: RwLock::new(None),
            });

            build_main_window(app.handle(), &settings)?;
            build_tray(app.handle(), &settings.language)?;

            texthooker::spawn(app.handle().clone(), hooker_rx);
            texthooker::spawn_clipboard_watcher(app.handle().clone());
            watcher::spawn(app.handle().clone());
            sync::spawn_flusher(app.handle().clone());
            streak::spawn(app.handle().clone());

            // Opt-in: with the toggle off, no thread is started and no IPC
            // socket is opened. Discord not running is not a startup failure.
            if settings.discord_rpc {
                let handle = app.handle().clone();
                let discord_settings = settings.clone();
                tauri::async_runtime::spawn(async move {
                    discord::apply_settings(&handle, &discord_settings).await;
                });
            }

            if settings.texthooker_autoconnect {
                let state = app.state::<AppState>();
                let _ = state
                    .hooker_tx
                    .send(HookerCommand::Connect(settings.texthooker_ws_url.clone()));
            }

            if settings.start_minimized {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let keep_running = tauri::async_runtime::block_on(async {
                    window
                        .app_handle()
                        .state::<AppState>()
                        .settings_snapshot()
                        .await
                        .minimize_to_tray
                });
                if keep_running {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::verify_api_key,
            commands::connection_status,
            commands::get_tracker_state,
            commands::list_processes,
            commands::log_current_anime,
            commands::list_pending,
            commands::confirm_pending,
            commands::discard_pending,
            commands::list_outbox,
            commands::retry_outbox,
            commands::delete_outbox,
            commands::flush_outbox,
            commands::create_log,
            commands::preview_xp,
            commands::recent_logs,
            commands::user_stats,
            commands::ranking,
            commands::streak_status,
            commands::discord_preview,
            commands::discord_blacklist,
            commands::add_discord_blacklist,
            commands::remove_discord_blacklist,
            commands::search_media,
            commands::multi_search_media,
            commands::set_title_mapping,
            commands::list_title_mappings,
            commands::delete_title_mapping,
            commands::list_vn_apps,
            commands::upsert_vn_app,
            commands::delete_vn_app,
            commands::hooker_state,
            commands::hooker_connect,
            commands::hooker_disconnect,
            commands::set_hooker_session,
            commands::hooker_lines,
            commands::hooker_totals,
            commands::hooker_ingest,
            commands::hooker_set_timer_active,
            commands::hooker_attach_media,
            commands::hooker_delete_line,
            commands::hooker_restore_line,
            commands::hooker_history,
            commands::hooker_clear,
            commands::hooker_set_timer,
            commands::list_text_sessions,
            commands::log_text_session,
            commands::media_detail,
            commands::remote_text_session,
            commands::list_extensions,
            commands::extensions_folder,
            commands::open_extensions_folder,
            commands::open_extension_options,
            commands::platform_supported,
            commands::parse_release_name,
        ])
        .build(tauri::generate_context!())
        .expect("error while building NihongoTracker Companion")
        .run(|app, event| {
            // Never leave a presence hanging with whatever was being read when
            // the app closed.
            if matches!(event, tauri::RunEvent::Exit) {
                discord::shutdown(app);
            }
        });
}

/// Where unpacked browser extensions are loaded from.
pub fn extensions_dir(app: &tauri::AppHandle) -> std::path::PathBuf {
    let dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("extensions");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// The main window is built here rather than declared in `tauri.conf.json`
/// because loading browser extensions needs `extensions_path`, which the config
/// file does not expose.
///
/// Popup dictionaries (Yomitan, JitenReader) are ordinary Chrome extensions, so
/// with this the reader gets the same hover-a-word experience as a browser —
/// on Windows, where WebView2 supports extensions. Elsewhere the flags are
/// ignored and the window is otherwise identical.
fn build_main_window(app: &tauri::AppHandle, settings: &config::Settings) -> tauri::Result<()> {
    let mut builder =
        tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::default())
            .title("NihongoTracker Companion")
            .inner_size(1100.0, 720.0)
            .min_inner_size(900.0, 600.0)
            .resizable(true)
            .center();

    if settings.extensions_enabled {
        builder = builder
            .browser_extensions_enabled(true)
            .extensions_path(extensions_dir(app));
    }

    builder.build()?;
    Ok(())
}

/// The tray menu in the user's language. Rebuilt when the language changes, so
/// it never needs a restart.
pub fn tray_menu(app: &tauri::AppHandle, lang: &str) -> tauri::Result<Menu<tauri::Wry>> {
    let show = MenuItem::with_id(app, "show", i18n::tray_open(lang), true, None::<&str>)?;
    let log_now = MenuItem::with_id(app, "log", i18n::streak_action(lang), true, None::<&str>)?;
    let sync_now = MenuItem::with_id(app, "sync", i18n::tray_sync(lang), true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", i18n::tray_quit(lang), true, None::<&str>)?;
    Menu::with_items(app, &[&show, &log_now, &sync_now, &quit])
}

fn build_tray(app: &tauri::AppHandle, lang: &str) -> tauri::Result<()> {
    let menu = tray_menu(app, lang)?;

    let mut tray = TrayIconBuilder::with_id("main-tray")
        .tooltip("NihongoTracker Companion")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
            }
            "log" => streak::open_log_form(app),
            "sync" => {
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    sync::flush(&handle).await;
                    sync::sync_text_lines(&handle).await;
                });
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::DoubleClick { .. } = event {
                if let Some(window) = tray.app_handle().get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        });

    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}
