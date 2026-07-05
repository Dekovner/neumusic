# NeuMusic

GUI audio downloader. Downloads best-quality audio, converts to 192kbps MP3 with optional silence at the start.

## Download

Get the latest release from [Releases](https://github.com/Dekovner/neumusic/releases):

- **Linux (AppImage)**: `neumusic.AppImage` — needs system `ffmpeg` + `ffprobe`
- **Windows**: `neumusic.exe` — fully self-contained (includes yt-dlp, ffmpeg, ffprobe)

## Usage

1. Paste a URL
2. Select output folder
3. (Optional) enable 192kbps conversion and/or silence at start
4. Click Download

## Build from source

```sh
cargo run              # debug
./build_appimage.sh    # AppImage
```

Windows cross-compile:
```sh
cargo build --release --target x86_64-pc-windows-gnu
```

## Dependencies

- **Linux**: `ffmpeg`, `ffprobe` (system), optionally `xclip`/`xsel`/`wl-clipboard` for paste
- **Windows**: none — all embedded
