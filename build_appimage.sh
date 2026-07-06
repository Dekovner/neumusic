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
if ! command -v appimagetool &>/dev/null; then
    echo "Installing appimagetool..."
    if command -v cargo-appimage &>/dev/null; then
        # extract appimagetool from cargo-appimage install
        APPIMAGETOOL=$(which appimagetool 2>/dev/null || echo "")
        if [ -z "$APPIMAGETOOL" ]; then
            echo "appimagetool not found, installing via cargo..."
            cargo install appimagetool 2>/dev/null || true
        fi
    fi
fi

APPDIR="target/neumusic.AppDir"
rm -rf "$APPDIR"

mkdir -p "$APPDIR/usr/bin"
cp "target/release/neumusic" "$APPDIR/usr/bin/"

cat > "$APPDIR/AppRun" << 'EOF'
#!/bin/bash
HERE="$(dirname "$(readlink -f "$0")")"
export WINIT_UNIX_BACKEND=wayland
exec "$HERE/usr/bin/neumusic" "$@"
EOF
chmod +x "$APPDIR/AppRun"

cat > "$APPDIR/neumusic.desktop" << EOF
[Desktop Entry]
Name=NeuMusic
Exec=neumusic
Icon=neumusic
Type=Application
Categories=AudioVideo;
EOF

cp neumusic.png "$APPDIR/"

mkdir -p target/appimage
appimagetool "$APPDIR" "target/appimage/neumusic.AppImage"

rm -rf "$APPDIR"

echo ""
echo "Done! AppImage created at: target/appimage/neumusic.AppImage"
