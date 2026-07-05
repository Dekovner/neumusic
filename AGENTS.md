# neumusic — AGENTS.md

## Build & run

```sh
cargo run              # debug
cargo build --release  # release
./build_appimage.sh    # full pipeline: download yt-dlp → cargo build --release → cargo appimage
```

AppImage output: `target/appimage/neumusic.AppImage`

## External deps

- **yt-dlp**: embedded via `rust-embed` from `assets/yt-dlp`. That file must exist before building. `build_appimage.sh` downloads it automatically.
- **ffmpeg**: system dependency on Linux/macOS (checked at startup). Install via package manager. On Windows, `ffmpeg.exe` is embedded in the binary (in `assets/ffmpeg.exe`).

## Windows EXE (cross-compile)

```sh
rustup target add x86_64-pc-windows-gnu
sudo apt install mingw-w64
cargo build --release --target x86_64-pc-windows-gnu
```

Output: `target/x86_64-pc-windows-gnu/release/neumusic.exe`

The EXE is fully self-contained — both `yt-dlp` and `ffmpeg` are embedded in the binary.

## Architecture

Single crate (`eframe`/`egui` GUI). Entrypoint: `src/main.rs` → extracts yt-dlp, checks ffmpeg → launches `NeuMusicApp` (`src/app.rs`). Pipeline: `downloader.rs` → `converter.rs`. No web framework, no server.

## Notable

- `src/spectrogram.rs` — defined but **unused** (not imported in `lib.rs`)
- UI bilingual (RU/EN, Russian default), toggle in top-right
- No tests, no CI, no lint config — `cargo check` is the only verification
- Single crate, no workspace
- Icon: `neumusic.png` at project root (also `icon.png` for `cargo-appimage` compatibility)
