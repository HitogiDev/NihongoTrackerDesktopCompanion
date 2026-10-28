mod api;
mod commands;
mod connection;
mod config;
mod db;
mod dictionary;
mod discord;
mod i18n;
mod mining;
mod native_hook;
mod native_bridge;
mod notifications;
mod parser;
mod platform;
mod playback;
mod players;
mod state;
mod streak;
mod sync;
mod texthooker;
mod text_cache;
mod text_filters;
mod updater;
mod watcher;
mod window_chrome;
mod reading_overlay;

use std::sync::Arc;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager, WindowEvent};
use tokio::sync::{mpsc, RwLock};

use crate::db::Db;
use crate::dictionary::DictionaryStore;
use crate::state::{AppState, HookerCommand, HookerState, TrackerSnapshot};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main_window(app);
        }));
        // Size, position and maximized state, restored when a window is built
        // again. Never visibility: the main window shows itself when ready, and
        // the reading overlay places its own windows.
        builder = builder.plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::all() - tauri_plugin_window_state::StateFlags::VISIBLE,
                )
                .with_denylist(&[reading_overlay::LABEL, reading_overlay::POPUP_LABEL])
                .build(),
        );
        builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
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
            let dictionaries = Arc::new(DictionaryStore::open(data_dir.join("dictionaries")));
            let settings = db.load_settings();
            #[cfg(windows)]
            if let Err(error) = notifications::register(app.handle()) {
                log::warn!("Could not register notification identity: {error}");
            }
            let _ = db.prune(30);

            let (hooker_tx, hooker_rx) = mpsc::unbounded_channel::<HookerCommand>();

            app.manage(AppState {
                reading_overlay: Default::default(),
                db,
                dictionaries,
                tracker: RwLock::new(TrackerSnapshot {
                    platform_supported: platform::supported(),
                    capabilities: platform::capabilities(),
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
                session_paused: Default::default(),
                vn_timer_edit: Default::default(),
                streak: RwLock::new(Default::default()),
                streak_auth_failed: Default::default(),
                discord: RwLock::new(None),
                game_hook: native_hook::NativeGameHookManager::default(),
                text_cache_tasks: Default::default(),
                connection_online: Default::default(),
                browser_bridge: Default::default(),
                outbox_flush: Default::default(),
                main_intent: Default::default(),
                connection_last: Default::default(),
            });

            // Started minimized, the main window is not built at all until the
            // tray or another instance asks for it: a webview the user never
            // looks at still costs a renderer process.
            if !settings.start_minimized {
                build_main_window(app.handle())?;
            }
            if settings.overlay_enabled {
                build_overlay_window(app.handle())?;
            }
            #[cfg(desktop)]
            {
                use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
                // Ctrl+Shift+M, or ⌘⇧M on macOS where Control shortcuts belong to
                // the terminal.
                #[cfg(target_os = "macos")]
                let primary = Modifiers::SUPER;
                #[cfg(not(target_os = "macos"))]
                let primary = Modifiers::CONTROL;
                let shortcut = Shortcut::new(Some(primary | Modifiers::SHIFT), Code::KeyM);
                app.handle().plugin(
                    tauri_plugin_global_shortcut::Builder::new().with_handler(move |app, _, event| {
                        if event.state() == ShortcutState::Pressed {
                            let app = app.clone();
                            tauri::async_runtime::spawn(async move {
                                let _ = mining::mine_latest(&app).await;
                            });
                        }
                    }).build(),
                )?;
                if let Err(error) = app.global_shortcut().register(shortcut) {
                    log::warn!("Could not register sentence shortcut: {error}");
                }
            }
            build_tray(app.handle(), &settings.language)?;
            reading_overlay::apply_shortcuts(app.handle(), &settings);
            reading_overlay::spawn(app.handle().clone());

            texthooker::spawn(app.handle().clone(), hooker_rx);
            texthooker::spawn_clipboard_watcher(app.handle().clone());
            watcher::spawn(app.handle().clone());
            sync::spawn_flusher(app.handle().clone());
            connection::spawn(app.handle().clone());
            sync::spawn_presence_sync(app.handle().clone());
            streak::spawn(app.handle().clone());
            #[cfg(desktop)]
            {
                app.manage(updater::Pending::default());
                updater::spawn(app.handle().clone());
            }

            // Opt-in: with the toggle off, no thread is started and no IPC
            // socket is opened. Discord not running is not a startup failure.
            if settings.discord_rpc {
                let handle = app.handle().clone();
                let discord_settings = settings.clone();
                tauri::async_runtime::spawn(async move {
                    discord::apply_settings(&handle, &discord_settings).await;
                });
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(window.label(), reading_overlay::LABEL | reading_overlay::POPUP_LABEL) {
                match event {
                    WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        if window.label() == reading_overlay::LABEL { reading_overlay::hide(window.app_handle()); }
                        else { reading_overlay::close_popup(window.app_handle()); }
                    }
                    WindowEvent::Moved(_) | WindowEvent::Resized(_) => reading_overlay::save_geometry(window.app_handle(), window.label()),
                    _ => {}
                }
                return;
            }
            if let WindowEvent::CloseRequested { api, .. } = event {
                if matches!(window.label(), "settings" | "texthooker" | reading_overlay::SETTINGS_LABEL) {
                    api.prevent_close();
                    let _ = window.hide();
                    if window.label() == "texthooker" {
                        let app = window.app_handle().clone();
                        tauri::async_runtime::spawn(async move {
                            let state = app.state::<AppState>();
                            if state.tracker.read().await.vn.is_none() {
                                let _ = state.hooker_tx.send(HookerCommand::Disconnect);
                            }
                        });
                    }
                    if window.label() == reading_overlay::SETTINGS_LABEL {
                        reading_overlay::settings_closed(window.app_handle());
                    }
                    return;
                }
                // Closing the session overlay switches it off; the webview goes
                // with it (see `set_overlay_enabled`).
                if window.label() == "overlay" {
                    let app = window.app_handle().clone();
                    tauri::async_runtime::spawn(async move {
                        let state = app.state::<AppState>();
                        let updated = {
                            let mut settings = state.settings.write().await;
                            if !settings.overlay_enabled {
                                return;
                            }
                            settings.overlay_enabled = false;
                            settings.clone()
                        };
                        if let Err(error) = state.db.save_settings(&updated) {
                            eprintln!("Failed to save overlay visibility: {error}");
                        }
                        let _ = app.emit("settings://changed", updated);
                    });
                    return;
                }
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
            commands::preview_native_hook_filters,
            commands::reading_overlay_state,
            commands::reading_overlay_action,
            commands::reading_overlay_popup,
            commands::reading_overlay_select_hook,
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
            commands::activity_feed,
            commands::friends_immersion_presence,
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
            commands::select_native_hook,
            commands::launch_program,
            commands::hooker_state,
            commands::hooker_connect,
            commands::set_hooker_input_source,
            commands::hooker_disconnect,
            commands::set_hooker_session,
            commands::hooker_lines,
            commands::hooker_latest_line,
            commands::overlay_open_main,
            commands::take_main_intent,
            commands::open_aux_window,
            commands::open_work_texthooker,
            commands::set_session_paused,
            commands::mine_line,
            commands::mine_latest_line,
            commands::mined_sentences,
            commands::delete_mined_sentence,
            commands::mined_screenshot,
            commands::export_mined_to_anki,
            commands::anki_models,
            commands::anki_model_fields,
            commands::mine_dictionary_entry,
            commands::hooker_totals,
            commands::hooker_context,
            commands::refresh_hooker_context,
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
            commands::dictionary_list,
            commands::dictionary_import,
            commands::dictionary_remove,
            commands::dictionary_lookup,
            commands::platform_supported,
            commands::platform_capabilities,
            commands::pending_update,
            commands::check_for_update,
            commands::install_update,
            commands::request_window_titles_permission,
            commands::parse_release_name,
        ])
        .build(tauri::generate_context!())
        .expect("error while building NihongoTracker Companion")
        .run(|app, event| {
            // Never leave a presence hanging with whatever was being read when
            // the app closed.
            if matches!(event, tauri::RunEvent::Exit) {
                discord::shutdown(app);
                app.state::<AppState>().game_hook.shutdown();
            }
            // Clicking the Dock icon. With "minimize to tray" the window is only
            // hidden (or never built), and macOS would otherwise do nothing.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                show_main_window(app);
            }
        });
}

/// The main window is built here rather than declared in `tauri.conf.json`, and
/// only when it is about to be shown.
///
/// It starts hidden and the frontend shows it after its first render, so the
/// user never sees a blank webview flash before the theme loads. If the page
/// fails to load, Rust shows it anyway a few seconds later.
fn build_main_window(app: &tauri::AppHandle) -> tauri::Result<()> {
    let theme = app.state::<AppState>().settings.try_read()
        .map(|settings| settings.theme.clone())
        .unwrap_or_else(|_| "dark".into());
    let builder = tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::default())
        .title("NihongoTracker Companion")
        .inner_size(1100.0, 720.0)
        .min_inner_size(900.0, 600.0)
        .resizable(true)
        .center()
        .visible(false);
    window_chrome::main_window(builder, &theme).build()?;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(4)).await;
        if let Some(window) = app.get_webview_window("main") {
            if window.is_visible().ok() == Some(false) {
                let _ = window.show();
            }
        }
    });
    Ok(())
}

/// Brings the main window up, building it if it does not exist yet. Returns
/// `true` when it was just built: its page is still loading, so events emitted
/// now would reach no listener (see `open_main_with`).
pub fn show_main_window(app: &tauri::AppHandle) -> bool {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        return false;
    }
    if let Err(error) = build_main_window(app) {
        log::error!("Could not create the main window: {error}");
        return false;
    }
    true
}

/// Shows the main window and hands it something to do: emitted as `event` if
/// the page is already listening, or kept for `take_main_intent` if the window
/// had to be built.
pub fn open_main_with(app: &tauri::AppHandle, event: &str, intent: &str) {
    if show_main_window(app) {
        *app.state::<AppState>().main_intent.lock().unwrap() = Some(intent.to_string());
    } else {
        let _ = app.emit(event, intent);
    }
}

fn build_overlay_window(app: &tauri::AppHandle) -> tauri::Result<()> {
    tauri::WebviewWindowBuilder::new(app, "overlay", tauri::WebviewUrl::default())
        .title("NihongoTracker Overlay")
        .inner_size(350.0, 190.0)
        .resizable(true)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .position(80.0, 80.0)
        .build()?;
    Ok(())
}

/// The session overlay exists only while it is switched on. Switching it off
/// destroys the webview instead of hiding it.
pub fn set_overlay_enabled(app: &tauri::AppHandle, enabled: bool) {
    match (app.get_webview_window("overlay"), enabled) {
        (Some(window), true) => { let _ = window.show(); }
        (Some(window), false) => { let _ = window.destroy(); }
        (None, true) => {
            if let Err(error) = build_overlay_window(app) {
                log::error!("Could not create the session overlay: {error}");
            }
        }
        (None, false) => {}
    }
}

/// The tray menu in the user's language. Rebuilt when the language changes, so
/// it never needs a restart.
pub fn tray_menu(app: &tauri::AppHandle, lang: &str) -> tauri::Result<Menu<tauri::Wry>> {
    let show = MenuItem::with_id(app, "show", i18n::tray_open(lang), true, None::<&str>)?;
    let log_now = MenuItem::with_id(app, "log", i18n::streak_action(lang), true, None::<&str>)?;
    let sync_now = MenuItem::with_id(app, "sync", i18n::tray_sync(lang), true, None::<&str>)?;
    let overlay_interaction = MenuItem::with_id(app, "overlay-interaction", i18n::tray_overlay_interaction(lang), true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", i18n::tray_quit(lang), true, None::<&str>)?;
    Menu::with_items(app, &[&show, &log_now, &sync_now, &overlay_interaction, &quit])
}

fn build_tray(app: &tauri::AppHandle, lang: &str) -> tauri::Result<()> {
    let menu = tray_menu(app, lang)?;

    let mut tray = TrayIconBuilder::with_id("main-tray")
        .tooltip("NihongoTracker Companion")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => { show_main_window(app); }
            "log" => streak::open_log_form(app),
            "sync" => {
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    sync::flush(&handle).await;
                    sync::sync_text_lines(&handle).await;
                });
            }
            "overlay-interaction" => { let _ = reading_overlay::restore_interaction(app); }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::DoubleClick { .. } = event {
                show_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}
