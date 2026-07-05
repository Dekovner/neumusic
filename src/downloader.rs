use std::path::{Path, PathBuf};
use std::process::Command;

pub const AUDIO_EXTS: &[&str] = &[
    "mp3", "m4a", "webm", "opus", "mka", "ogg", "aac", "wav", "flac",
];

pub fn download_audio(
    yt_dlp: &Path,
    url: &str,
    dir: &Path,
    playlist: bool,
    ffmpeg_dir: Option<&Path>,
) -> anyhow::Result<PathBuf> {
    let template = dir.join("%(title)s.%(ext)s");
    let template_str = template.to_string_lossy().to_string();

    let mut args: Vec<String> = vec![
        "-f".to_owned(),
        "bestaudio/best".to_owned(),
        "-x".to_owned(),
        "-o".to_owned(),
        template_str,
    ];

    if let Some(fdir) = ffmpeg_dir {
        args.push("--ffmpeg-location".to_owned());
        args.push(fdir.to_string_lossy().to_string());
    }

    if !playlist {
        args.push("--no-playlist".to_owned());
    }
    args.push(url.to_owned());

    let status = Command::new(yt_dlp)
        .args(&args)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .map_err(|e| anyhow::anyhow!("Не удалось запустить yt-dlp: {}", e))?;

    if !status.success() {
        return Err(anyhow::anyhow!("yt-dlp: ошибка загрузки"));
    }

    find_newest_audio(dir)
        .ok_or_else(|| anyhow::anyhow!("Не найден скачанный аудиофайл"))
}

pub fn find_newest_audio(dir: &Path) -> Option<PathBuf> {
    let entries: Vec<_> = std::fs::read_dir(dir).ok()?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| AUDIO_EXTS.contains(&ext))
                .unwrap_or(false)
        })
        .collect();

    entries
        .into_iter()
        .max_by_key(|e| {
            std::fs::metadata(e.path())
                .and_then(|m| m.modified())
                .ok()
        })
        .map(|e| e.path())
}
