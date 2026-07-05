# Changelog

## v1.0.0

### New features
- OGG 208kbps output format (libvorbis) — selectable via radio buttons
- Progress bar with percentage during download
- Spinner indicator next to Download button
- Sample rate enforcement (auto-downscale to 48kHz if exceeded)
- DRM fallback: retry with `--extractor-args soundcloud:formats=*`
- Dynamic conversion log (shows actual format and bitrate)
- No upscaling: conversion uses `min(source_bitrate, target_bitrate)` for both MP3 and OGG
- Displays actual average bitrate of downloaded audio in log
- Green progress bar at 100%

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

### Platform
- Windows: hidden console windows for yt-dlp, ffmpeg, ffprobe, powershell (`CREATE_NO_WINDOW`)
- accesskit disabled to fix Wine/Proton crashes

---

## v0.1.0

- Initial release
- Download audio via yt-dlp
- Convert to MP3 192 kbps
- Add silence at start
- Paste from clipboard
- RU/EN language toggle
