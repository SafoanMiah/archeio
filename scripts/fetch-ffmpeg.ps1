#!/usr/bin/env pwsh
# Downloads the gyan.dev FFmpeg "essentials" build, extracts ffmpeg.exe and
# its LICENSE file, and drops them into src-tauri/binaries/. The Tauri build
# bundles those into the MSI/NSIS installer so end users don't need to install
# FFmpeg separately.
#
# Run this once before the first `npm run tauri:build`. Re-run to refresh.

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$repoRoot = Split-Path -Parent $PSScriptRoot
$dest = Join-Path $repoRoot 'src-tauri\binaries'
$exePath = Join-Path $dest 'ffmpeg.exe'
$licensePath = Join-Path $dest 'FFMPEG_LICENSE.txt'

if (-not (Test-Path $dest)) {
    New-Item -ItemType Directory -Path $dest | Out-Null
}

$url = 'https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip'
$zip = Join-Path $env:TEMP 'archeio-ffmpeg-essentials.zip'
$extract = Join-Path $env:TEMP 'archeio-ffmpeg-extract'

Write-Host "Downloading FFmpeg essentials (~100 MB) from gyan.dev..."
Invoke-WebRequest -Uri $url -OutFile $zip -UseBasicParsing

Write-Host "Extracting..."
if (Test-Path $extract) { Remove-Item -Recurse -Force $extract }
Expand-Archive -Path $zip -DestinationPath $extract -Force

$ffmpeg = Get-ChildItem -Path $extract -Recurse -Filter ffmpeg.exe | Select-Object -First 1
$license = Get-ChildItem -Path $extract -Recurse -Filter LICENSE | Select-Object -First 1

if (-not $ffmpeg) { throw "ffmpeg.exe not found in extracted archive" }

Copy-Item $ffmpeg.FullName $exePath -Force
if ($license) { Copy-Item $license.FullName $licensePath -Force }

Remove-Item -Recurse -Force $extract
Remove-Item -Force $zip

$size = [math]::Round((Get-Item $exePath).Length / 1MB, 1)
Write-Host "Done. ffmpeg.exe ($size MB) -> $exePath"
