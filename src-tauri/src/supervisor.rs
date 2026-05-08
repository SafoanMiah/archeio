//! Broadcast lifecycle: provision via YouTube API, push ffmpeg to the returned
//! ingest URL, watch it, clean up.
//!
//! Invariant: `inner` holds `Some(_)` iff a broadcast is running. The monitor
//! task is the *only* writer that flips it back to `None`, regardless of
//! whether the exit was user-requested (stop()) or ffmpeg dying on its own.
//! That single rule kills the "already streaming" stuck-state bug.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use parking_lot::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncWriteExt;
use tokio::process::Child;
use tokio::sync::Mutex as AsyncMutex;
use tokio::task::JoinHandle;
use tokio::time::timeout;

use crate::error::{Error, Result};
use crate::types::LiveState;

/// Below this many seconds, we treat the broadcast as "never reached YouTube"
/// and best-effort delete the orphaned broadcast resource so the user's
/// channel doesn't accumulate empty `created`-state broadcasts.
const MIN_LIVE_SECS: i64 = 5;

#[derive(Default)]
pub struct State {
    /// Holds the running broadcast handle, or None when idle.
    inner: Mutex<Option<Running>>,
    /// Serialises start/stop so two button mashes can't race into a half-set
    /// state. The monitor task does *not* take this lock - it only touches
    /// `inner` when clearing.
    op_lock: AsyncMutex<()>,
    /// Mirrors the success of the most recent global-shortcut registration.
    /// Frontend can `invoke("hotkey_status")` on mount so it doesn't miss the
    /// initial event emitted before listeners attach.
    pub hotkey_active: Mutex<bool>,
}

struct Running {
    cancel: Arc<AtomicBool>,
    started_at: chrono::DateTime<chrono::Utc>,
    /// Owned by the slot so `stop()` can take it out and `await` it, ensuring
    /// the monitor's cleanup happens before `stop()` resolves.
    monitor: JoinHandle<()>,
}

impl State {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> LiveState {
        match self.inner.lock().as_ref() {
            Some(r) => LiveState {
                is_running: true,
                started_at: Some(r.started_at),
            },
            None => LiveState {
                is_running: false,
                started_at: None,
            },
        }
    }
}

pub async fn start(app: AppHandle, custom_title: Option<String>) -> Result<LiveState> {
    let state = app.state::<Arc<State>>().inner().clone();
    let _g = state.op_lock.lock().await;

    if state.inner.lock().is_some() {
        return Err(Error::AlreadyBroadcasting);
    }

    // Custom title from the UI input wins; if blank/missing, generate the
    // default `Archeio · May 8, 14:32` form so the channel page row is at
    // least timestamped.
    let title = custom_title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(generate_title);
    let test_output = std::env::var("ARCHEIO_TEST_OUTPUT").ok();

    // Provision a YouTube broadcast - unless we're in test mode targeting a
    // local file, in which case we skip every API call.
    let (broadcast_id, video_id, rtmp_url, broadcast_title) = if let Some(target) = test_output {
        tracing::info!("[debug] using test output target: {}", target);
        (None, None, target, title.clone())
    } else {
        let p = crate::youtube::provision_broadcast(&title).await?;
        tracing::info!(
            "provisioned broadcast: video_id={} broadcast_id={}",
            p.video_id,
            p.broadcast_id
        );
        let url = format!("{}/{}", p.rtmp_ingest, p.stream_key);
        (Some(p.broadcast_id), Some(p.video_id), url, p.title)
    };

    let child = match crate::ffmpeg::spawn(&app, &rtmp_url).await {
        Ok(c) => c,
        Err(e) => {
            // ffmpeg failed before pushing - the broadcast we just created is
            // orphaned. Clean it up so it doesn't sit in `created` forever.
            if let Some(bid) = &broadcast_id {
                let _ = crate::youtube::delete_broadcast(bid).await;
            }
            return Err(e);
        }
    };

    let started_at = Utc::now();

    // Library row is created upfront with the video_id we already know - no
    // post-broadcast paste/auto-link round-trip needed.
    let library_row_id = if let Some(vid) = &video_id {
        if let Some(lib) = app.try_state::<Arc<crate::library::Library>>() {
            let row = lib.add_at_start(started_at, vid, &broadcast_title);
            let _ = app.emit("library-changed", ());
            Some(row.id)
        } else {
            None
        }
    } else {
        None
    };

    let cancel = Arc::new(AtomicBool::new(false));

    let monitor = spawn_monitor(
        app.clone(),
        state.clone(),
        child,
        cancel.clone(),
        started_at,
        broadcast_id,
        library_row_id,
    );

    {
        let mut slot = state.inner.lock();
        *slot = Some(Running {
            cancel,
            started_at,
            monitor,
        });
    }

    crate::overlay::show(&app);

    let live = state.snapshot();
    let _ = app.emit("live-state-changed", &live);
    Ok(live)
}

pub async fn stop(app: AppHandle) -> Result<()> {
    let state = app.state::<Arc<State>>().inner().clone();
    let _g = state.op_lock.lock().await;

    let monitor = {
        let mut slot = state.inner.lock();
        let running = slot.take().ok_or(Error::NotBroadcasting)?;
        running.cancel.store(true, Ordering::SeqCst);
        running.monitor
    };

    // The monitor's internal ceiling is 5s (graceful 'q' wait + kill).
    // 7s gives slack for tokio scheduling and overlay events.
    let _ = tokio::time::timeout(Duration::from_secs(7), monitor).await;
    Ok(())
}

/// Crash-recovery sweep. Runs at app startup; finds library rows that were
/// left mid-broadcast (ended_at = None) by a previous run that died without
/// reaching graceful_exit_handler (Task Manager kill, OS crash, panic) and
/// transitions the YouTube broadcast to `complete` so it stops showing as
/// live on the user's channel. Library rows are marked ended only if the
/// API call succeeds, so OAuth-disconnected runs leave the work for a
/// future launch.
pub async fn recover_orphans(app: AppHandle) {
    let lib = match app.try_state::<Arc<crate::library::Library>>() {
        Some(l) => l.inner().clone(),
        None => return,
    };

    let orphans: Vec<_> = lib
        .list()
        .into_iter()
        .filter(|r| r.ended_at.is_none() && r.youtube_video_id.is_some())
        .collect();

    if orphans.is_empty() {
        return;
    }

    tracing::info!(
        "recovery: {} unfinished broadcast(s) from a previous run",
        orphans.len()
    );

    let now = Utc::now();
    let mut changed = false;
    for row in orphans {
        let Some(vid) = &row.youtube_video_id else {
            continue;
        };
        match crate::youtube::complete_broadcast(vid).await {
            Ok(()) => {
                tracing::info!("recovery: completed orphan broadcast {vid}");
                if lib.set_ended(&row.id, now).is_ok() {
                    changed = true;
                }
            }
            Err(e) => {
                tracing::warn!("recovery: complete_broadcast({vid}) failed: {e}");
                // Leave the row open; next launch will retry. Most common
                // cause is OAuth not connected yet (e.g. user disconnected).
            }
        }
    }

    if changed {
        let _ = app.emit("library-changed", ());
    }
}

pub async fn toggle(app: AppHandle) {
    // Hotkey + tray paths don't carry a custom title; the supervisor falls
    // back to the auto-generated `Archeio · <date>` form.
    let state = app.state::<Arc<State>>().inner().clone();
    let running = state.inner.lock().is_some();
    let result = if running {
        stop(app.clone()).await
    } else {
        start(app.clone(), None).await.map(|_| ())
    };
    if let Err(e) = result {
        tracing::warn!("toggle failed: {e}");
        let _ = app.emit("error-toast", e.to_string());
    }
}

fn generate_title() -> String {
    // YouTube broadcast title; user can rename in-app afterwards. Local
    // timezone for human-friendliness on the channel page.
    let now = chrono::Local::now();
    format!("Archeio · {}", now.format("%b %-d, %H:%M"))
}

fn spawn_monitor(
    app: AppHandle,
    state: Arc<State>,
    mut child: Child,
    cancel: Arc<AtomicBool>,
    started_at: chrono::DateTime<chrono::Utc>,
    broadcast_id: Option<String>,
    library_row_id: Option<String>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut stdin = child.stdin.take();

        // Drain stderr so the pipe doesn't fill and stall ffmpeg.
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                use tokio::io::{AsyncBufReadExt, BufReader};
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::info!("ffmpeg: {}", line);
                }
            });
        }

        loop {
            if cancel.load(Ordering::SeqCst) {
                if let Some(mut sin) = stdin.take() {
                    let _ = sin.write_all(b"q\n").await;
                    let _ = sin.flush().await;
                    let _ = sin.shutdown().await;
                }
                match timeout(Duration::from_secs(5), child.wait()).await {
                    Ok(Ok(status)) => {
                        tracing::info!("ffmpeg exited cleanly: {:?}", status);
                    }
                    Ok(Err(e)) => {
                        tracing::warn!("ffmpeg wait error: {e}; killing");
                        let _ = child.kill().await;
                    }
                    Err(_) => {
                        tracing::warn!("ffmpeg ignored 'q' for 5s; killing");
                        let _ = child.kill().await;
                        let _ = child.wait().await;
                    }
                }
                break;
            }

            match child.try_wait() {
                Ok(Some(status)) => {
                    tracing::info!("ffmpeg exited on its own: {:?}", status);
                    if !cancel.load(Ordering::SeqCst) {
                        let _ = app.emit(
                            "error-toast",
                            "Stream ended unexpectedly. Check your network \
                             and try again."
                                .to_string(),
                        );
                    }
                    break;
                }
                Ok(None) => {
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
                Err(e) => {
                    tracing::warn!("ffmpeg try_wait error: {e}");
                    break;
                }
            }
        }

        // The single place that clears the slot.
        {
            let mut slot = state.inner.lock();
            *slot = None;
        }

        let now = Utc::now();
        let elapsed = (now - started_at).num_seconds();

        if let Some(lib) = app.try_state::<Arc<crate::library::Library>>() {
            if elapsed < MIN_LIVE_SECS {
                // Sub-5s: broadcast never reached YouTube in any meaningful
                // way. Best-effort clean up the orphaned broadcast resource
                // and drop the empty library row.
                if let Some(bid) = &broadcast_id {
                    if let Err(e) = crate::youtube::delete_broadcast(bid).await {
                        tracing::warn!("delete_broadcast on cleanup failed: {e}");
                    }
                }
                if let Some(rid) = &library_row_id {
                    let _ = lib.remove(rid);
                }
            } else if let Some(rid) = &library_row_id {
                let _ = lib.set_ended(rid, now);
            }
            let _ = app.emit("library-changed", ());
        }

        let _ = app.emit("live-state-changed", state.snapshot());
        crate::overlay::hide(&app);
    })
}
