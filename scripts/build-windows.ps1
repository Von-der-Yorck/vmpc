# Builds VMPC for Windows and puts vmpc.exe into .\dist
#   -BundleFFmpeg  also copies ffmpeg.exe + ffprobe.exe into dist\ffmpeg\ (fully portable folder)
param([switch]$BundleFFmpeg)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

cargo build --release
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $root "target" }
New-Item -ItemType Directory -Force (Join-Path $root "dist") | Out-Null
Copy-Item (Join-Path $target "release\vmpc.exe") (Join-Path $root "dist\vmpc.exe") -Force

if ($BundleFFmpeg) {
    $ff = (Get-Command ffmpeg -ErrorAction SilentlyContinue).Source
    $fp = (Get-Command ffprobe -ErrorAction SilentlyContinue).Source
    if (-not $ff -or -not $fp) { throw "ffmpeg/ffprobe not on PATH - install with: winget install Gyan.FFmpeg" }
    New-Item -ItemType Directory -Force (Join-Path $root "dist\ffmpeg") | Out-Null
    Copy-Item $ff, $fp (Join-Path $root "dist\ffmpeg") -Force
}
Write-Host "Done: dist\vmpc.exe"
