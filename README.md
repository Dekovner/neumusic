# NeuMusic

GUI audio downloader and converter. Downloads best available audio (Opus), converts to **MP3 (≤192kbps)** or **OGG (≤208kbps)** with strict bitrate caps. Supports local files, playlist downloads, silence insertion, and debloat (CBR re-encode).

## Download

Get the latest release from [Releases](https://github.com/Dekovner/neumusic/releases):

- **Linux (AppImage)**: `neumusic.AppImage` — needs system `ffmpeg` + `ffprobe`
- **Windows**: `neumusic.exe` — fully self-contained (includes yt-dlp, ffmpeg, ffprobe)

## Usage

1. Paste a URL or click **📁 Load file** to select a local audio file
2. Select output folder
3. Configure settings (optional):
   - **Convert audio** — enable MP3 (192kbps cap) or OGG (208kbps cap) conversion
   - **Add silence at start** — insert silence in milliseconds
   - **Download entire playlist** — download all tracks from a playlist URL
   - **Debloat audio** — force CBR re-encode at a chosen bitrate (8–320 kbps)
4. Click **Download**

If the source bitrate is lower than the target cap and debloat is off, a warning is shown in the log suggesting to enable debloat.

## Build from source

### Linux (AppImage)
```sh
./build_appimage.sh
```

Output: `target/appimage/neumusic.AppImage`

### Windows (cross-compile)
```sh
rustup target add x86_64-pc-windows-gnu
sudo apt install mingw-w64
cargo build --release --target x86_64-pc-windows-gnu
```

Output: `target/x86_64-pc-windows-gnu/release/neumusic.exe`

### Debug
```sh
cargo run
```

## Dependencies

- **Linux**: `ffmpeg` + `ffprobe` (system), optionally `xclip`/`xsel`/`wl-clipboard` for paste
- **Windows**: none — all embedded (yt-dlp, ffmpeg, ffprobe)
