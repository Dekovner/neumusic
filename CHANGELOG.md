# Changelog

## v1.0.1-1

### Changes
- Bloat detection now compares source vs actual bitrate — warning only shows when
  conversion actually increased file size (`actual > src_kbps`)
- Warning message updated: shows source → actual kbps with spectrogram check
  recommendation
- `src_kbps` now always measured on raw downloaded file (regardless of convert toggle)

---

## v1.0.1

### Bug fixes
- Accurate bitrate detection — `get_actual_bitrate()` now uses `filesize * 8 / duration` instead of unreliable `format.bit_rate` for ABR/VBR
- Bitrate warning now compares source bitrate (not final ABR-upconverted file) with target cap — fixes bloat detection on SoundCloud
- Cleanup no longer deletes unrelated files in output dir — scoped to same filename stem only
- Warning not showing when convert=ON fixed (was comparing final upscaled bitrate)
- Final bitrate measurement moved to end of pipeline (after all processing)

### UI improvements
- Log entries are now colored: normal=white, warning=yellow, errors=red
- Warning moved from separate yellow label into log (maintains yellow color)
- Log auto-scrolls to bottom on new entries
- URL + Load file merged into one row with vertical separator
- Separate row for loaded filename + ✕ (always visible, no overflow)
- URL field disabled (greyed) when local file is loaded
- URL field width capped at 200px
- Filename truncated with `…` when too long

### Pipeline order
- Silence applied BEFORE debloat: convert → silence → debloat → cleanup

### Platform
- Windows EXE and Linux AppImage rebuilt with all fixes

---

## v1.0.0

### New features
- OGG 208kbps output format (libvorbis) — selectable via radio buttons
- Progress bar with percentage during download
- Spinner indicator next to Download button
- Sample rate enforcement (auto-downscale to 48kHz if exceeded)
- DRM fallback: retry with `--extractor-args soundcloud:formats=*`
- Dynamic conversion log (shows actual format and bitrate)
- ABR bitrate capping: no floor, never exceeds 192k (MP3) / 208k (OGG)
- No upscaling: conversion uses ABR with `-maxrate` instead of CBR/VBR floors
- Displays actual average bitrate of downloaded audio in log
- Green progress bar at 100%
- Low-bitrate warning (yellow) when actual < 192kbps and debloat is off

### Bug fixes
- Fixed double lossy encoding — removed `--audio-format mp3` from yt-dlp args
- Fixed `add_silence` filter syntax (missing `adelay=` prefix)
- Fixed ffmpeg path resolution on Linux for `--ffmpeg-location`
- Fixed cleanup to not delete unrelated mp3 files
- Fixed conversion log always showing "192 kbps" regardless of format
- Fixed progress bar never rendering (processed all messages in one frame)

### UI improvements
- English is default language
- Non-resizable window
- Removed spinner animation from progress bar
- Clearer error message on DRM-protected content
- Settings moved to CollapsingHeader (no separate window)
- Debloat is off by default (must be explicitly enabled)
- debloat_bitrate changed to u32 Slider (8..=320 kbps) instead of TextEdit

### Platform
- Windows: hidden console windows for yt-dlp, ffmpeg, ffprobe, powershell (`CREATE_NO_WINDOW`)
- accesskit disabled to fix Wine/Proton crashes
- Wayland support: `WINIT_UNIX_BACKEND=wayland` in AppRun, egui/eframe 0.35
- Linux paste via `xclip`/`xsel`/`wl-paste` instead of `arboard`

---

## v0.1.0

- Initial release
- Download audio via yt-dlp
- Convert to MP3 192 kbps
- Add silence at start
- Paste from clipboard
- RU/EN language toggle
