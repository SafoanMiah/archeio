mod commands;
mod error;
mod ffmpeg;
mod library;
mod oauth;
mod overlay;
mod supervisor;
mod tray;
mod types;
mod youtube;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tauri::{Emitter, Manager, RunEvent};
use tauri_plugin_global_shortcut::ShortcutState;

/// Latch flipped on the first ExitRequested so the second pass (after we
/// re-trigger exit post-cleanup) doesn't loop infinitely.
static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);

pub use error::{Error, Result};

pub const HOTKEY: &str = "Alt+X";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "archeio_lib=info,warn".into()),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            // Global handler attached at plugin-build time. The official
            // pattern - per-shortcut on_shortcut() handlers were not firing
            // reliably; this single sink is the documented approach and gets
            // every shortcut event the OS routes to us.
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    tracing::info!("hotkey fired: {HOTKEY}");
                    let h = app.clone();
                    tauri::async_runtime::spawn(async move {
                        supervisor::toggle(h).await;
                    });
                })
                .build(),
        )
        .setup(|app| {
            let handle = app.handle().clone();
            handle.manage(Arc::new(supervisor::State::new()));
            handle.manage(Arc::new(library::Library::load()));

            tray::install(&handle)?;
            overlay::create(&handle);

            // Crash-recovery sweep. Task Manager kills (and any other
            // unblockable terminate) bypass graceful_exit_handler, so any
            // broadcasts in progress at the time can hang in 'live' state
            // on YouTube. Walk the library on startup and transition them
            // to complete via the API.
            let h = handle.clone();
            tauri::async_runtime::spawn(async move {
                supervisor::recover_orphans(h).await;
            });

            let ok = register_hotkey(&handle).is_ok();
            *handle
                .state::<Arc<supervisor::State>>()
                .hotkey_active
                .lock() = ok;
            let _ = handle.emit("hotkey-status", ok);

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    let _ = window.hide();
                    api.prevent_close();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::start_broadcast,
            commands::stop_broadcast,
            commands::live_state,
            commands::check_ffmpeg,
            commands::open_external,
            commands::retry_hotkey,
            commands::hotkey_label,
            commands::hotkey_status,
            commands::library_list,
            commands::library_remove,
            commands::oauth_status,
            commands::oauth_connect,
            commands::oauth_disconnect,
            commands::oauth_save_client,
            commands::youtube_channel_title,
            commands::youtube_update_title,
            commands::youtube_update_privacy,
        ])
        .build(tauri::generate_context!())
        .expect("error while building archeio")
        .run(graceful_exit_handler);
}

/// Catch every exit path (tray Quit, OS logoff/shutdown, external close) and
/// flush a running broadcast through `supervisor::stop` before letting the
/// process die. Without this, ffmpeg's child gets `kill_on_drop`'d on runtime
/// teardown and YouTube sees an abrupt RTMP disconnect.
fn graceful_exit_handler(app: &tauri::AppHandle, event: RunEvent) {
    let RunEvent::ExitRequested { api, .. } = event else {
        return;
    };
    if SHUTTING_DOWN.swap(true, Ordering::SeqCst) {
        // Second pass - our own re-triggered exit. Let it through.
        return;
    }
    api.prevent_exit();
    let h = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(8),
            supervisor::stop(h.clone()),
        )
        .await;
        h.exit(0);
    });
}

pub fn register_hotkey(app: &tauri::AppHandle) -> Result<()> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;

    let shortcuts = app.global_shortcut();
    let _ = shortcuts.unregister_all();
    if let Err(e) = shortcuts.register(HOTKEY) {
        // Almost always a zombie Archeio from a previous dev run holding the
        // chord at the kernel level. Windows doesn't release a RegisterHotKey
        // claim until the owning thread is gone, and crashed dev iterations
        // can linger. Kill any stale Archeio.exe other than us and try once
        // more before giving up.
        tracing::warn!("hotkey '{HOTKEY}' first attempt failed: {e}; killing stale archeio processes and retrying");
        kill_stale_archeio();
        std::thread::sleep(std::time::Duration::from_millis(300));
        let _ = shortcuts.unregister_all();
        shortcuts.register(HOTKEY).map_err(|e2| {
            tracing::warn!("hotkey '{HOTKEY}' retry failed: {e2}");
            Error::Other(format!("register hotkey '{HOTKEY}': {e2}"))
        })?;
    }
    tracing::info!("hotkey '{HOTKEY}' registered");
    Ok(())
}

/// Force-kill any other Archeio.exe processes still alive on this machine.
/// Used as a self-heal step when the global hotkey can't be registered -
/// the most common cause is our own zombie from an earlier crashed dev run.
fn kill_stale_archeio() {
    let our_pid = std::process::id();
    let filter = format!("PID ne {}", our_pid);
    let _ = std::process::Command::new("taskkill")
        .args(["/F", "/IM", "Archeio.exe", "/FI", &filter])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    // Lowercase too - older builds shipped with the lowercase bin name and
    // a leftover build from before the rename could still be alive.
    let _ = std::process::Command::new("taskkill")
        .args(["/F", "/IM", "archeio.exe", "/FI", &filter])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

