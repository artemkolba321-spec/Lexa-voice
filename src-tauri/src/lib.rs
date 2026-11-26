mod commands;
mod devices;
mod platform;
mod snapshot;
mod tools;
mod utils;
mod voice;

use std::sync::{Arc, Mutex};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, WindowEvent};

/// Обёртка для VoiceEngine в Tauri State.
pub struct VoiceStateWrapper(pub Arc<Mutex<Option<voice::VoiceEngine>>>);

pub fn run() {
    tauri::Builder::default()
        .manage(Mutex::new(devices::AppState::default()))
        .manage(VoiceStateWrapper(Arc::new(Mutex::new(None))))
        .setup(|app| {
            let open_item =
                MenuItem::with_id(app, "open", "Открыть Lexa", true, None::<&str>)?;

            let separator =
                tauri::menu::PredefinedMenuItem::separator(app)?;

            let quit_item =
                MenuItem::with_id(app, "quit", "Завершить Lexa", true, None::<&str>)?;

            let menu = Menu::with_items(
                app,
                &[&open_item, &separator, &quit_item],
            )?;

            let mut tray = TrayIconBuilder::new()
                .tooltip("Lexa — локальный ассистент")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| {
                    match event.id.as_ref() {
                        "open" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.unminimize();
                                let _ = window.set_focus();
                            }
                        }
                        "quit" => app.exit(0),
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();

                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                    }
                });

            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }

            tray.build(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::execute_tool,
            commands::run_demo_command,
            commands::set_profile,
            commands::list_audio_devices,
            commands::list_audio_hosts,
            commands::start_listen,
            commands::stop_listen,
            commands::text_command,
            commands::get_voice_status,
        ])
        .build(tauri::generate_context!())
        .expect("не удалось инициализировать Lexa")
        .run(|app_handle, event| {
            if let tauri::RunEvent::WindowEvent {
                label,
                event: WindowEvent::CloseRequested { api, .. },
                ..
            } = event
            {
                if label == "main" {
                    api.prevent_close();

                    if let Some(window) = app_handle.get_webview_window(&label) {
                        let _ = window.hide();
                    }
                }
            }
        });
}
