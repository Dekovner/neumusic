#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

# Use rustup cargo if available
if [ -f "$HOME/.cargo/env" ]; then
    . "$HOME/.cargo/env"
fi

echo "=== Downloading yt-dlp standalone binary ==="
mkdir -p assets
if [ ! -f assets/yt-dlp ]; then
    curl -L -o assets/yt-dlp \
        https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp
fi
chmod +x assets/yt-dlp

echo "=== Building release binary ==="
cargo build --release

echo "=== Building AppImage ==="
if ! command -v cargo-appimage &>/dev/null; then
    echo "Installing cargo-appimage..."
    cargo install cargo-appimage
fi

cargo appimage

echo ""
echo "Done! AppImage created at: target/appimage/neumusic.AppImage"
