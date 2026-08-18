mod app_state;
mod commands;
mod database;
mod domain;
mod error;
mod models;
mod platform;
mod windowing;

use std::time::{Duration, Instant};

use app_state::{AppState, RuntimeState};
use database::Database;
use tauri::{menu::MenuBuilder, tray::TrayIconBuilder, AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcutExt as _, ShortcutState};

pub fn run() {
    let shortcut_plugin = tauri_plugin_global_shortcut::Builder::new()
        .with_shortcut(platform::DEFAULT_SHORTCUT)
        .expect("default shortcut should be valid")
        .with_handler(|app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                let _ = windowing::toggle_clipboard(app);
            }
        })
        .build();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            let _ = windowing::show_clipboard(app);
        }))
        .plugin(shortcut_plugin)
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .app_name("EasyClipboard")
                .build(),
        )
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            commands::list_items,
            commands::get_item,
            commands::paste_item,
            commands::delete_item,
            commands::clear_recent,
            commands::delete_all_data,
            commands::set_pinned,
            commands::list_groups,
            commands::create_group,
            commands::rename_group,
            commands::delete_group,
            commands::move_item,
            commands::get_settings,
            commands::update_settings,
            commands::set_global_shortcut,
            commands::get_desktop_capabilities,
            commands::start_recording,
            commands::request_paste_automation_access,
            commands::open_paste_automation_settings,
            commands::pick_excluded_app,
            commands::hide_panel,
            commands::close_settings,
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            {
                app.handle()
                    .set_activation_policy(tauri::ActivationPolicy::Accessory)?;
                app.handle().set_dock_visibility(false)?;
            }
            let app_data_dir = app.path().app_data_dir()?;
            let database = tauri::async_runtime::block_on(Database::open(&app_data_dir))
                .map_err(|error| error.to_string())?;
            if let Ok(settings) = tauri::async_runtime::block_on(database.get_settings()) {
                if settings.shortcut != platform::DEFAULT_SHORTCUT {
                    let manager = app.global_shortcut();
                    if manager.unregister_all().is_err()
                        || manager.register(settings.shortcut.as_str()).is_err()
                    {
                        let _ = manager.unregister_all();
                        let _ = manager.register(platform::DEFAULT_SHORTCUT);
                        log::warn!("saved shortcut could not be restored");
                    }
                }
            }
            let runtime = RuntimeState::new(platform::change_token());
            app.manage(AppState {
                database,
                runtime: std::sync::Mutex::new(runtime),
            });
            setup_tray(app)?;
            let (change_sender, change_receiver) = tokio::sync::mpsc::unbounded_channel();
            platform::install_clipboard_listener(change_sender)?;
            spawn_clipboard_monitor(app.handle().clone(), change_receiver);
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "clipboard" && matches!(event, WindowEvent::Focused(false)) {
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("failed to run EasyClipboard");
}

fn setup_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let menu = MenuBuilder::new(app)
        .text("open", "打开 EasyClipboard")
        .text("pause", "暂停 / 继续记录")
        .separator()
        .text("settings", "设置…")
        .separator()
        .text("quit", "退出 EasyClipboard")
        .build()?;
    let icon = app
        .default_window_icon()
        .cloned()
        .expect("Tauri app icon should be configured");
    TrayIconBuilder::with_id("main")
        .icon(icon)
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip("EasyClipboard")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => {
                let _ = windowing::show_clipboard(app);
            }
            "pause" => toggle_recording(app.clone()),
            "settings" => {
                let _ = windowing::open_settings(app);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

fn toggle_recording(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let database = app.state::<AppState>().database.clone();
        if let Ok(mut settings) = database.get_settings().await {
            settings.recording_paused = !settings.recording_paused;
            if let Ok(settings) = database.save_settings(settings).await {
                let _ = app.emit("settings://changed", settings);
            }
        }
    });
}

fn spawn_clipboard_monitor(
    app: AppHandle,
    change_receiver: tokio::sync::mpsc::UnboundedReceiver<()>,
) {
    #[cfg(target_os = "windows")]
    let mut change_receiver = change_receiver;
    #[cfg(target_os = "macos")]
    let _change_receiver = change_receiver;
    tauri::async_runtime::spawn(async move {
        let mut last_cleanup = Instant::now() - Duration::from_secs(86_400);
        loop {
            let database = app.state::<AppState>().database.clone();
            let settings = match database.get_settings().await {
                Ok(settings) => settings,
                Err(error) => {
                    log::warn!("settings read failed: {}", error.code());
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
            };
            if last_cleanup.elapsed() >= Duration::from_secs(86_400) {
                if let Err(error) = database
                    .cleanup(settings.max_items, settings.retention_days)
                    .await
                {
                    log::warn!("scheduled cleanup failed: {}", error.code());
                }
                last_cleanup = Instant::now();
            }

            let (started, _idle_for) = app
                .state::<AppState>()
                .runtime
                .lock()
                .map(|runtime| {
                    (
                        runtime.clipboard_started,
                        runtime.last_clipboard_change.elapsed(),
                    )
                })
                .unwrap_or((false, Duration::from_secs(60)));
            if !started || settings.recording_paused {
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
            if !platform::paste_automation_ready() {
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }

            let count = platform::change_token();
            let now = Instant::now();
            let should_capture = {
                let app_state = app.state::<AppState>();
                let Ok(mut runtime) = app_state.runtime.lock() else {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                };
                runtime.should_capture_change(count, now)
            };

            if should_capture {
                let source =
                    platform::frontmost_application().unwrap_or(platform::TargetApplication {
                        pid: 0,
                        name: "未知应用".into(),
                        identifier: None,
                        #[cfg(target_os = "windows")]
                        window_handle: 0,
                    });
                let excluded =
                    platform::source_is_excluded(&settings, source.identifier.as_deref());
                if !excluded {
                    match platform::read_capture(source) {
                        Ok(Some(captured)) => {
                            let suppress_by_hash = app
                                .state::<AppState>()
                                .runtime
                                .lock()
                                .map(|runtime| {
                                    now <= runtime.suppress_until
                                        && runtime.expected_hash.as_ref() == Some(&captured.hash)
                                })
                                .unwrap_or(false);
                            if !suppress_by_hash {
                                match database.insert_capture(captured).await {
                                    Ok(_) => {
                                        let _ = app.emit("clipboard://changed", ());
                                    }
                                    Err(error) => log::warn!("capture skipped: {}", error.code()),
                                }
                            }
                        }
                        Ok(None) => {}
                        Err(error) => log::warn!("capture skipped: {}", error.code()),
                    }
                }
            }
            #[cfg(target_os = "macos")]
            let interval = if _idle_for >= Duration::from_secs(60) {
                Duration::from_secs(1)
            } else {
                Duration::from_millis(500)
            };
            #[cfg(target_os = "macos")]
            tokio::time::sleep(interval).await;
            #[cfg(target_os = "windows")]
            {
                let _ = change_receiver.recv().await;
            }
        }
    });
}
