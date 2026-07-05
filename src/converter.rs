use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(windows)]
use std::os::windows::process::CommandExt;

fn ffprobe_cmd(ffmpeg: &Path) -> PathBuf {
    let parent = ffmpeg.parent().unwrap_or(Path::new(""));
    let probe_name = if cfg!(target_os = "windows") { "ffprobe.exe" } else { "ffprobe" };
    parent.join(probe_name)
}

fn run_ffprobe(path: &Path, ffmpeg: &Path, entries: &str) -> anyhow::Result<String> {
    let mut cmd = Command::new(ffprobe_cmd(ffmpeg));
    cmd.args([
        "-v", "error",
        "-show_entries", entries,
        "-of", "default=noprint_wrappers=1:nokey=1",
        &path.to_string_lossy(),
    ]);
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);
    let output = cmd.output()?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn get_sample_rate(path: &Path, ffmpeg: &Path) -> anyhow::Result<u32> {
    let s = run_ffprobe(path, ffmpeg, "stream=sample_rate")?;
    s.parse().map_err(|e| anyhow::anyhow!("Не удалось прочитать sample rate: {}", e))
}

pub fn get_actual_bitrate(path: &Path, ffmpeg: &Path) -> anyhow::Result<u64> {
    let s = run_ffprobe(path, ffmpeg, "format=bit_rate")?;
    s.parse().map_err(|e| anyhow::anyhow!("Не удалось прочитать bitrate: {}", e))
}

pub fn convert_audio(ffmpeg: &Path, input: &Path, format: &str, bitrate: &str) -> anyhow::Result<PathBuf> {
    let ext = match format {
        "mp3" => "mp3",
        "ogg" => "ogg",
        _ => return Err(anyhow::anyhow!("Unknown format: {}", format)),
    };

    let output = input.with_extension(ext);
    if output == input {
        return Ok(input.to_path_buf());
    }

    let mut args: Vec<String> = vec![
        "-y".to_owned(),
        "-i".to_owned(),
        input.to_string_lossy().to_string(),
        "-b:a".to_owned(),
        bitrate.to_owned(),
    ];

    if format == "ogg" {
        args.push("-c:a".to_owned());
        args.push("libvorbis".to_owned());
    }

    if get_sample_rate(input, ffmpeg).unwrap_or(0) > 48000 {
        args.push("-ar".to_owned());
        args.push("48000".to_owned());
    }

    args.push(output.to_string_lossy().to_string());

    let mut cmd = Command::new(ffmpeg);
    cmd.args(&args)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);
    let status = cmd.status()
        .map_err(|e| anyhow::anyhow!("Не удалось запустить ffmpeg: {}", e))?;

    if !status.success() {
        return Err(anyhow::anyhow!("ffmpeg: ошибка конвертации в {} kbps {}", bitrate.trim_end_matches('k'), ext));
    }

    std::fs::remove_file(input)?;
    Ok(output)
}

pub fn add_silence(ffmpeg: &Path, input: &Path, ms: u64, format: &str, bitrate: &str) -> anyhow::Result<PathBuf> {
    let ext = match format {
        "mp3" => "mp3",
        "ogg" => "ogg",
        _ => return Err(anyhow::anyhow!("Unknown format: {}", format)),
    };
    let tmp_ext = format!("silence.{ext}");
    let tmp = input.with_extension(&tmp_ext);
    let adelays = format!("adelay={}|{}", ms, ms);

    let mut args: Vec<String> = vec![
        "-y".to_owned(),
        "-i".to_owned(),
        input.to_string_lossy().to_string(),
        "-b:a".to_owned(),
        bitrate.to_owned(),
    ];
    if format == "ogg" {
        args.push("-c:a".to_owned());
        args.push("libvorbis".to_owned());
    }

    if get_sample_rate(input, ffmpeg).unwrap_or(0) > 48000 {
        args.push("-ar".to_owned());
        args.push("48000".to_owned());
    }

    args.push("-af".to_owned());
    args.push(adelays.clone());
    args.push(tmp.to_string_lossy().to_string());

    let mut cmd = Command::new(ffmpeg);
    cmd.args(&args)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);
    let status = cmd.status()
        .map_err(|e| anyhow::anyhow!("Не удалось запустить ffmpeg: {}", e))?;

    if !status.success() {
        return Err(anyhow::anyhow!("ffmpeg: ошибка добавления тишины"));
    }

    std::fs::remove_file(input)?;
    let output = input.with_extension(ext);
    std::fs::rename(&tmp, &output)?;
    Ok(output)
}


const CLEANUP_EXTS: &[&str] = &[
    "m4a", "webm", "opus", "mka", "ogg", "aac", "wav", "flac",
];

pub fn cleanup(dir: &Path, keep: &Path, format: &str) {
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
                    .is_some_and(|n| n.ends_with(&format!(".silence.{}", format)));
            if is_temp || is_silence_tmp {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}
