#!/usr/bin/env sh
# Builds VMPC on Linux and installs it for the current user (~/.local).
#   Debian/Ubuntu deps: sudo apt install build-essential pkg-config libasound2-dev ffmpeg
#   Fedora deps:        sudo dnf install gcc pkgconf alsa-lib-devel ffmpeg
#   Arch deps:          sudo pacman -S base-devel alsa-lib ffmpeg
set -eu
cd "$(dirname "$0")/.."

cargo build --release
target="${CARGO_TARGET_DIR:-target}"

mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications" "$HOME/.local/share/icons/hicolor/256x256/apps" "$HOME/.local/share/icons/hicolor/512x512/apps"
install -m 755 "$target/release/vmpc" "$HOME/.local/bin/vmpc"
install -m 644 assets/vmpc-256.png "$HOME/.local/share/icons/hicolor/256x256/apps/vmpc.png"
install -m 644 assets/vmpc-512.png "$HOME/.local/share/icons/hicolor/512x512/apps/vmpc.png"
install -m 644 assets/vmpc.desktop "$HOME/.local/share/applications/vmpc.desktop"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$HOME/.local/share/applications" || true

command -v ffmpeg >/dev/null 2>&1 || echo "NOTE: ffmpeg is not installed - VMPC needs ffmpeg + ffprobe at runtime."
echo "Installed: ~/.local/bin/vmpc (also in your app menu as VMPC)"
