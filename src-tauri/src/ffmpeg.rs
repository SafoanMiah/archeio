use std::process::Stdio;

use tauri::{AppHandle, Manager};
use tokio::process::{Child, Command};

use crate::error::{Error, Result};

/// Locate ffmpeg.exe. Search order:
/// 1. `<resource_dir>/binaries/ffmpeg.exe` - the copy bundled by tauri.conf.json
///    `bundle.resources`. This is where the installer drops it on end-user
///    machines.
/// 2. `<exe_dir>/ffmpeg.exe` - manual placement next to Archeio.exe (portable
///    installs, advanced users).
/// 3. Bare `"ffmpeg"` - rely on PATH. Last-resort fallback for dev sessions
///    where the resource hasn't been copied alongside the dev binary yet.
fn resolve_program(app: &AppHandle) -> String {
    if let Ok(resource_dir) = app.path().resource_dir() {
        let candidate = resource_dir.join("binaries").join("ffmpeg.exe");
        if candidate.exists() {
            return candidate.to_string_lossy().into_owned();
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let candidate = parent.join("ffmpeg.exe");
            if candidate.exists() {
                return candidate.to_string_lossy().into_owned();
            }
        }
    }
    "ffmpeg".to_string()
}

pub async fn check(app: &AppHandle) -> Result<String> {
    let output = Command::new(resolve_program(app))
        .args(["-hide_banner", "-version"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|_| Error::FfmpegNotFound)?;

    if !output.status.success() {
        return Err(Error::FfmpegNotFound);
    }
    let line = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    if line.is_empty() {
        return Err(Error::FfmpegNotFound);
    }
    Ok(line)
}

async fn detect_video_encoder(app: &AppHandle) -> &'static str {
    let output = Command::new(resolve_program(app))
        .args(["-hide_banner", "-encoders"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await;

    let combined = match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout).into_owned(),
        Err(_) => return "libx264",
    };

    if combined.contains("h264_nvenc") {
        "h264_nvenc"
    } else if combined.contains("h264_amf") {
        "h264_amf"
    } else if combined.contains("h264_qsv") {
        "h264_qsv"
    } else {
        "libx264"
    }
}

fn encoder_tuning(encoder: &str) -> Vec<&'static str> {
    match encoder {
        "h264_nvenc" => vec!["-preset", "p4", "-tune", "ll", "-rc", "cbr"],
        "h264_amf" => vec!["-quality", "speed", "-usage", "transcoding", "-rc", "cbr"],
        "h264_qsv" => vec!["-preset", "veryfast"],
        _ => vec!["-preset", "veryfast", "-tune", "zerolatency"],
    }
}

/// Spawn an ffmpeg child piping desktop capture (scaled to 1080p60) + silent
/// audio to YouTube's RTMP ingest.
///
/// Filter chain notes (these are the bits that bite):
/// - `ddagrab` outputs D3D11 BGRA frames. `hwdownload` MUST be paired with
///   `format=bgra`; specifying `nv12` is rejected because that's not what the
///   GPU produced.
/// - The trailing `format=yuv420p` converts to the pixel format the encoder
///   wants. Setting `-pix_fmt yuv420p` alone isn't reliable across encoders.
/// - `scale=1920:1080` normalises any monitor resolution to 1080p so the
///   bitrate makes sense; ddagrab's own `video_size` parameter is the source
///   crop, not the output size, so we don't use it.
/// - Audio is `anullsrc` - YouTube wants both tracks present, but device
///   enumeration is a whole separate failure surface we're sidestepping in V0.1.
pub async fn spawn(app: &AppHandle, rtmp_url: &str) -> Result<Child> {
    let encoder = detect_video_encoder(app).await;
    tracing::info!("ffmpeg encoder: {}", encoder);

    // ARCHEIO_FFMPEG_LOGLEVEL is undocumented escape hatch for debugging - the
    // default (warning) keeps logs quiet; set "info" to see frame-rate stats
    // and stream health lines.
    let log_level = std::env::var("ARCHEIO_FFMPEG_LOGLEVEL").unwrap_or_else(|_| "warning".into());
    let mut args: Vec<String> = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        log_level,
        "-f".into(),
        "lavfi".into(),
        "-i".into(),
        "anullsrc=channel_layout=stereo:sample_rate=44100".into(),
        "-filter_complex".into(),
        "ddagrab=output_idx=0:framerate=60,hwdownload,format=bgra,scale=1920:1080,format=yuv420p[v]"
            .into(),
        "-map".into(),
        "[v]".into(),
        "-map".into(),
        "0:a".into(),
        "-c:v".into(),
        encoder.to_string(),
    ];
    args.extend(encoder_tuning(encoder).iter().map(|s| s.to_string()));
    args.extend([
        "-b:v".into(),
        "6000k".into(),
        "-maxrate".into(),
        "6000k".into(),
        "-bufsize".into(),
        "12000k".into(),
        // 2-second keyframe interval - YouTube ingest requirement.
        "-g".into(),
        "120".into(),
        "-keyint_min".into(),
        "120".into(),
        "-c:a".into(),
        "aac".into(),
        "-b:a".into(),
        "128k".into(),
        "-ar".into(),
        "44100".into(),
        "-f".into(),
        "flv".into(),
        rtmp_url.to_string(),
    ]);

    tracing::info!("spawning ffmpeg: {}", args.join(" "));

    let mut cmd = Command::new(resolve_program(app));
    cmd.args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    #[cfg(windows)]
    {
        // CREATE_NO_WINDOW so we don't pop a console next to the GUI.
        cmd.creation_flags(0x0800_0000);
    }

    cmd.spawn().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => Error::FfmpegNotFound,
        _ => Error::FfmpegFailed(e.to_string()),
    })
}
