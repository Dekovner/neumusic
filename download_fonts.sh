#!/bin/bash
set -e
FONTS_DIR="$(dirname "$0")/assets"
mkdir -p "$FONTS_DIR"

BASE="https://github.com/googlefonts/noto-cjk/releases/download/Sans2.004"

download_and_extract() {
    local zip_name="$1"
    local font_file="$2"

    if [ -f "$FONTS_DIR/$font_file" ]; then
        return
    fi

    local tmpzip=$(mktemp)
    echo "Downloading $zip_name..."
    curl -sL "$BASE/$zip_name" -o "$tmpzip"
    echo "Extracting $font_file..."
    unzip -j -o "$tmpzip" "$font_file" -d "$FONTS_DIR/" >/dev/null 2>&1
    rm -f "$tmpzip"
}

download_and_extract "16_NotoSansJP.zip"   "NotoSansJP-Regular.otf"
download_and_extract "17_NotoSansKR.zip"   "NotoSansKR-Regular.otf"
download_and_extract "18_NotoSansSC.zip"   "NotoSansSC-Regular.otf"
