use std::path::{Path, PathBuf};
use std::process::Command;

pub fn convert_to_192(ffmpeg: &Path, input: &Path) -> anyhow::Result<PathBuf> {
    let output = input.with_extension("mp3");
    if output == input {
        return Ok(input.to_path_buf());
    }

    let status = Command::new(ffmpeg)
        .args([
            "-y",
            "-i",
            &input.to_string_lossy(),
            "-b:a",
            "192k",
            &output.to_string_lossy(),
        ])
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .map_err(|e| anyhow::anyhow!("Не удалось запустить ffmpeg: {}", e))?;

    if !status.success() {
        return Err(anyhow::anyhow!("ffmpeg: ошибка конвертации в 192 kbps"));
    }

    std::fs::remove_file(input)?;
    Ok(output)
}

pub fn add_silence(ffmpeg: &Path, input: &Path, ms: u64) -> anyhow::Result<PathBuf> {
    let tmp = input.with_extension("silence.mp3");
    let adelays = format!("adelay={}|{}", ms, ms);

    let status = Command::new(ffmpeg)
        .args([
            "-y",
            "-i",
            &input.to_string_lossy(),
            "-b:a",
            "192k",
            "-af",
            &adelays,
            &tmp.to_string_lossy(),
        ])
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .map_err(|e| anyhow::anyhow!("Не удалось запустить ffmpeg: {}", e))?;

    if !status.success() {
        return Err(anyhow::anyhow!("ffmpeg: ошибка добавления тишины"));
    }

    std::fs::remove_file(input)?;
    let output = input.with_extension("mp3");
    std::fs::rename(&tmp, &output)?;
    Ok(output)
}


const CLEANUP_EXTS: &[&str] = &[
    "m4a", "webm", "opus", "mka", "ogg", "aac", "wav", "flac",
];

pub fn cleanup(dir: &Path, keep: &Path) {
    let keep_stem = keep.file_stem();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path == keep {
                continue;
            }
            let is_temp = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| CLEANUP_EXTS.contains(&ext))
                .unwrap_or(false);
            let is_silence_tmp = keep_stem.is_some()
                && path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.ends_with(".silence.mp3"));
            if is_temp || is_silence_tmp {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}
