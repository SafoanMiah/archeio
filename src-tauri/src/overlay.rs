//! The always-on-top "REC" / "Processing…" pill in the top-right corner.
//!
//! Owns its own Webview window. Build once at startup, hidden; show/hide on
//! broadcast and cooldown transitions. The window's inner size matches the
//! pill content exactly so there's no visible window background around it.

use tauri::utils::config::Color;
use tauri::{AppHandle, LogicalPosition, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

/// Logical pixels. The frontend `<OverlayPill>` is built to fill exactly this
/// rect; bumping these values without bumping the CSS will reintroduce the
/// "wider gray box" the user reported. Width chosen to comfortably fit the
/// "REC HH:MM:SS" content (~140px at the current font sizes); below that the
/// timer's right edge clips inside the pill.
pub const WIDTH: f64 = 160.0;
pub const HEIGHT: f64 = 32.0;

/// Padding from the screen edges, applied uniformly to top and right so the
/// pill looks visually balanced.
const MARGIN: f64 = 20.0;

pub fn create(app: &AppHandle) {
    let builder = WebviewWindowBuilder::new(
        app,
        "overlay",
        WebviewUrl::App("index.html?overlay=1".into()),
    )
    .title("Archeio Overlay")
    .inner_size(WIDTH, HEIGHT)
    .always_on_top(true)
    .decorations(false)
    .transparent(true)
    .skip_taskbar(true)
    .resizable(false)
    .shadow(false)
    .focused(false)
    .visible(false)
    // Force the WebView2 fill to fully-transparent. Without this, the area
    // *outside* the rounded pill (but inside the window rectangle) shows the
    // WebView2 default (gray) - the "different colored corners" bug.
    .background_color(Color(0, 0, 0, 0));

    match builder.build() {
        Ok(w) => {
            let _ = w.set_ignore_cursor_events(true);
            position(&w);
        }
        Err(e) => tracing::warn!("overlay window build failed: {e}"),
    }
}

pub fn show(app: &AppHandle) {
    // The overlay window is built lazily during app setup; if the user toggles
    // a broadcast in the first ~100-300ms after launch, the webview may not
    // be ready yet and `get_webview_window` returns `None` silently. Poll
    // briefly so a fast-trigger broadcast still gets its overlay.
    let h = app.clone();
    tauri::async_runtime::spawn(async move {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if let Some(w) = h.get_webview_window("overlay") {
                position(&w);
                let _ = w.show();
                let _ = w.set_always_on_top(true);
                return;
            }
            if std::time::Instant::now() >= deadline {
                tracing::warn!("overlay window never became available; skipping show");
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    });
}

pub fn hide(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.hide();
    }
}

/// Pin the window to the top-right corner with `MARGIN` padding from both
/// screen edges. Re-runs on every show in case the user changed displays or
/// scaling since startup.
fn position(w: &WebviewWindow) {
    let Ok(Some(mon)) = w.primary_monitor() else {
        return;
    };
    let scale = mon.scale_factor();
    let logical_w = mon.size().width as f64 / scale;
    let x = (logical_w - WIDTH - MARGIN).max(0.0);
    let y = MARGIN;
    let _ = w.set_position(LogicalPosition::new(x, y));
}
