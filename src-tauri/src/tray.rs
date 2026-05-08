use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use crate::error::Result;

const ID_SHOW: &str = "show";
const ID_TOGGLE: &str = "toggle";
const ID_QUIT: &str = "quit";

pub fn install(app: &AppHandle) -> Result<()> {
    if let Err(e) = install_inner(app) {
        tracing::warn!("tray install failed: {e}; continuing without a tray");
    }
    Ok(())
}

fn install_inner(app: &AppHandle) -> Result<()> {
    let show = MenuItem::with_id(app, ID_SHOW, "Show Archeio", true, None::<&str>)?;
    let toggle = MenuItem::with_id(app, ID_TOGGLE, "Toggle stream", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, ID_QUIT, "Quit", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&show, &toggle, &separator, &quit])?;

    let mut builder = TrayIconBuilder::with_id("archeio-tray")
        .tooltip("Archeio")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            ID_SHOW => show_main_window(app),
            ID_TOGGLE => {
                let h = app.clone();
                tauri::async_runtime::spawn(async move {
                    crate::supervisor::toggle(h).await;
                });
            }
            // The graceful-exit handler in lib.rs intercepts ExitRequested
            // and flushes any running broadcast before the process dies.
            ID_QUIT => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder
        .build(app)
        .map_err(|e| crate::error::Error::Other(format!("tray build: {e}")))?;
    Ok(())
}

fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}
