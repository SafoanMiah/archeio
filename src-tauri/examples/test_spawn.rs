//! Standalone reproducer for the ffmpeg spawn path used by the supervisor.
//!
//! Run with: cargo run --example test_spawn --manifest-path src-tauri/Cargo.toml
//!
//! This duplicates the exact arg list and Command::new wiring from
//! src/ffmpeg.rs but writes to NUL instead of RTMP, so we can isolate
//! whether the spawn itself works without involving Tauri or YouTube.

use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::time::timeout;

#[tokio::main]
async fn main() {
    let args: Vec<String> = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "info".into(),
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
        "h264_nvenc".into(),
        "-preset".into(),
        "p4".into(),
        "-tune".into(),
        "ll".into(),
        "-rc".into(),
        "cbr".into(),
        "-b:v".into(),
        "6000k".into(),
        "-maxrate".into(),
        "6000k".into(),
        "-bufsize".into(),
        "12000k".into(),
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
        "-t".into(),
        "5".into(),
        "-f".into(),
        "flv".into(),
        "NUL".into(),
    ];

    println!("[test] argv count: {}", args.len());

    let mut cmd = Command::new("ffmpeg");
    cmd.args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }

    println!("[test] spawning...");
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[test] spawn failed: {e}");
            std::process::exit(1);
        }
    };
    println!("[test] spawned, pid={:?}", child.id());

    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                println!("[ffmpeg] {line}");
            }
            println!("[test] stderr drained");
        });
    }

    match timeout(Duration::from_secs(15), child.wait()).await {
        Ok(Ok(status)) => {
            println!("[test] exit: {:?} success={}", status, status.success());
        }
        Ok(Err(e)) => println!("[test] wait err: {e}"),
        Err(_) => {
            println!("[test] timeout, killing");
            let _ = child.kill().await;
        }
    }
}
