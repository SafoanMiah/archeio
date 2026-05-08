//! Tauri command surface. Every name + signature is a contract with the React
//! app; renaming requires updating `src/lib/api.ts`.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, State};

use crate::error::{Error, Result};
use crate::library::{Broadcast, Library};
use crate::oauth::{self, OAuthStatus};
use crate::supervisor;
use crate::types::LiveState;
use crate::youtube;

#[tauri::command]
pub async fn start_broadcast(app: AppHandle, title: Option<String>) -> Result<LiveState> {
    supervisor::start(app, title).await
}

#[tauri::command]
pub async fn stop_broadcast(app: AppHandle) -> Result<()> {
    supervisor::stop(app).await
}

#[tauri::command]
pub async fn live_state(state: State<'_, Arc<supervisor::State>>) -> Result<LiveState> {
    Ok(state.snapshot())
}

#[tauri::command]
pub async fn check_ffmpeg(app: AppHandle) -> Result<String> {
    crate::ffmpeg::check(&app).await
}

#[tauri::command]
pub async fn open_external(app: AppHandle, url: String) -> Result<()> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| Error::Other(e.to_string()))
}

#[tauri::command]
pub async fn retry_hotkey(app: AppHandle) -> Result<bool> {
    let ok = crate::register_hotkey(&app).is_ok();
    *app.state::<Arc<supervisor::State>>().hotkey_active.lock() = ok;
    let _ = app.emit("hotkey-status", ok);
    Ok(ok)
}

#[tauri::command]
pub async fn hotkey_status(state: State<'_, Arc<supervisor::State>>) -> Result<bool> {
    Ok(*state.hotkey_active.lock())
}

#[tauri::command]
pub async fn hotkey_label() -> Result<String> {
    Ok(crate::HOTKEY.to_string())
}

#[tauri::command]
pub async fn library_list(library: State<'_, Arc<Library>>) -> Result<Vec<Broadcast>> {
    Ok(library.list())
}

#[tauri::command]
pub async fn library_remove(
    library: State<'_, Arc<Library>>,
    app: AppHandle,
    id: String,
) -> Result<()> {
    library.remove(&id)?;
    let _ = app.emit("library-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn oauth_status() -> Result<OAuthStatus> {
    Ok(oauth::status())
}

#[tauri::command]
pub async fn oauth_connect(app: AppHandle) -> Result<OAuthStatus> {
    oauth::connect().await?;
    let s = oauth::status();
    let _ = app.emit("oauth-status-changed", &s);
    Ok(s)
}

#[tauri::command]
pub async fn oauth_disconnect(app: AppHandle) -> Result<OAuthStatus> {
    oauth::disconnect()?;
    let s = oauth::status();
    let _ = app.emit("oauth-status-changed", &s);
    Ok(s)
}

#[tauri::command]
pub async fn oauth_save_client(
    app: AppHandle,
    client_id: String,
    client_secret: String,
) -> Result<OAuthStatus> {
    oauth::save_client(&client_id, &client_secret)?;
    let s = oauth::status();
    let _ = app.emit("oauth-status-changed", &s);
    Ok(s)
}

#[tauri::command]
pub async fn youtube_channel_title() -> Result<String> {
    youtube::channel_title().await
}

#[tauri::command]
pub async fn youtube_update_title(
    library: State<'_, Arc<Library>>,
    app: AppHandle,
    id: String,
    new_title: String,
) -> Result<Broadcast> {
    let video_id = library
        .list()
        .into_iter()
        .find(|r| r.id == id)
        .and_then(|r| r.youtube_video_id)
        .ok_or_else(|| Error::Other("broadcast has no linked YouTube video".into()))?;

    youtube::update_video_title(&video_id, &new_title).await?;
    let row = library.set_title(&id, &new_title)?;
    let _ = app.emit("library-changed", ());
    Ok(row)
}
