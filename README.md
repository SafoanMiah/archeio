# Archeio

Press a hotkey. Your screen streams to YouTube as a private live broadcast that auto-archives into a video on your channel. Your channel becomes the storage, your hard drive stays empty.

Windows only.

## Install

Grab the latest installer from [Releases](https://github.com/SafoanMiah/Archeio/releases):

- `Archeio_0.0.1_x64-setup.exe` (NSIS, recommended)
- `Archeio_0.0.1_x64_en-US.msi` (MSI, if you prefer Windows Installer)

FFmpeg is bundled, so there's nothing else to install. Run the installer, finish the in-app YouTube wizard (about 2 minutes), and you're done. Default hotkey is `Alt+X` to start and stop. Past broadcasts show up in the library with inline playback and one-click rename.

> Heads up: Windows SmartScreen will warn on first launch because the binary isn't code-signed. Click "More info" then "Run anyway". Not worth paying for a cert this early in.

## How it works

- Tauri 2 (Rust) + React frontend.
- Hotkey toggles a YouTube Live broadcast via the Data + Live Streaming APIs.
- FFmpeg captures the screen with `ddagrab`, encodes with NVENC / AMF / QSV / x264 (autodetected), pushes RTMP straight to YouTube ingest.
- OAuth tokens live in Windows Credential Manager. Library is JSON in `%LOCALAPPDATA%\Archeio\`.
- Bring your own Google Cloud OAuth client. The in-app wizard walks the steps. Per-user OAuth means per-user 10k-unit/day YouTube quota.

## Build from source

```powershell
git clone https://github.com/SafoanMiah/Archeio.git
cd Archeio
npm install
pwsh scripts/fetch-ffmpeg.ps1   # downloads ~100 MB FFmpeg into src-tauri/binaries/
npm run tauri:build             # outputs to src-tauri/target/release/bundle/
```

For development (`npm run tauri:dev`) you can skip the fetch script and install FFmpeg system-wide instead:

```powershell
winget install Gyan.FFmpeg
```

Archeio falls back to whatever `ffmpeg` is on PATH when no bundled copy is found.

## License

Archeio is [GPL-3.0](LICENSE). Use it, fork it, modify it, share it freely. The one rule: any distributed version has to stay GPL-3.0 with full source available, which keeps the project open and stops it from being quietly forked into a closed-source resell.

The bundled `ffmpeg.exe` is also GPL, so the installer ships under consistent terms. Full FFmpeg license ships as `FFMPEG_LICENSE.txt`.
