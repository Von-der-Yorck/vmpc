#!/usr/bin/env sh
# VMPC installer for Linux (x86_64). Installs for the current user, no root needed.
#
#   Web install (downloads the latest release):
#     curl -fsSL https://github.com/__REPO__/releases/latest/download/install-linux.sh | sh
#   From an extracted release archive:
#     ./install-linux.sh
#   Uninstall:
#     curl -fsSL https://github.com/__REPO__/releases/latest/download/install-linux.sh | sh -s -- --uninstall
set -eu

REPO="${VMPC_REPO:-__REPO__}"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
PREFIX="$DATA/vmpc"
BIN="$HOME/.local/bin"
APPS="$DATA/applications"
ICONS="$DATA/icons/hicolor"

refresh_menus() {
    command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$APPS" >/dev/null 2>&1 || true
    command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q "$ICONS" >/dev/null 2>&1 || true
}

if [ "${1:-}" = "--uninstall" ]; then
    rm -rf "$PREFIX" "$BIN/vmpc" "$APPS/vmpc.desktop" "$ICONS/256x256/apps/vmpc.png" "$ICONS/512x512/apps/vmpc.png"
    refresh_menus
    echo "VMPC has been removed. (Settings remain in ~/.config/vmpc.)"
    exit 0
fi

arch=$(uname -m)
if [ "$arch" != "x86_64" ]; then
    echo "Sorry, release builds are x86_64 only (this machine: $arch). Build from source instead." >&2
    exit 1
fi

here=$(cd "$(dirname "$0")" 2>/dev/null && pwd || pwd)
if [ -x "$here/vmpc" ] && [ -f "$here/assets/vmpc.desktop" ]; then
    src="$here"
else
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    url="https://github.com/$REPO/releases/latest/download/vmpc-linux-x86_64.tar.gz"
    echo "Downloading VMPC from $url"
    if command -v curl >/dev/null 2>&1; then
        curl -fL --progress-bar -o "$tmp/vmpc.tar.gz" "$url"
    elif command -v wget >/dev/null 2>&1; then
        wget -q --show-progress -O "$tmp/vmpc.tar.gz" "$url"
    else
        echo "Please install curl or wget." >&2
        exit 1
    fi
    tar -xzf "$tmp/vmpc.tar.gz" -C "$tmp"
    src="$tmp/vmpc"
fi

mkdir -p "$PREFIX" "$BIN" "$APPS" "$ICONS/256x256/apps" "$ICONS/512x512/apps"
rm -rf "$PREFIX"/*
cp -R "$src"/. "$PREFIX"/
chmod 755 "$PREFIX/vmpc"
[ -f "$PREFIX/ffmpeg/ffmpeg" ] && chmod 755 "$PREFIX/ffmpeg/ffmpeg" "$PREFIX/ffmpeg/ffprobe"
ln -sf "$PREFIX/vmpc" "$BIN/vmpc"
cp "$PREFIX/assets/vmpc-256.png" "$ICONS/256x256/apps/vmpc.png"
cp "$PREFIX/assets/vmpc-512.png" "$ICONS/512x512/apps/vmpc.png"
sed "s|^Exec=.*|Exec=\"$PREFIX/vmpc\" %F|" "$PREFIX/assets/vmpc.desktop" > "$APPS/vmpc.desktop"
refresh_menus

echo "VMPC installed to $PREFIX"
echo "Start it from your app menu or run: vmpc"
case ":$PATH:" in
    *":$BIN:"*) ;;
    *) echo "Note: $BIN is not on your PATH; the app menu entry works regardless." ;;
esac
