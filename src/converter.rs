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
    let dur = run_ffprobe(path, ffmpeg, "format=duration")?;
    let secs: f64 = dur.trim().parse()
        .map_err(|_| anyhow::anyhow!("Не удалось прочитать длительность"))?;
    let size = std::fs::metadata(path)
        .map_err(|_| anyhow::anyhow!("Не удалось прочитать размер файла"))?
        .len();
    let bps = (size as f64 * 8.0) / secs;
    Ok(bps as u64)
}

pub fn convert_audio(ffmpeg: &Path, input: &Path, format: &str) -> anyhow::Result<PathBuf> {
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
    ];

    if format == "ogg" {
        args.push("-b:a".to_owned());
        args.push("208k".to_owned());
        args.push("-maxrate".to_owned());
        args.push("208k".to_owned());
        args.push("-bufsize".to_owned());
        args.push("208k".to_owned());
        args.push("-c:a".to_owned());
        args.push("libvorbis".to_owned());
    } else {
        args.push("-b:a".to_owned());
        args.push("192k".to_owned());
        args.push("-maxrate".to_owned());
        args.push("192k".to_owned());
        args.push("-bufsize".to_owned());
        args.push("192k".to_owned());
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
        return Err(anyhow::anyhow!("ffmpeg: ошибка конвертации"));
    }

    std::fs::remove_file(input)?;
    Ok(output)
}

pub fn add_silence(ffmpeg: &Path, input: &Path, ms: u64, format: &str) -> anyhow::Result<PathBuf> {
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
    ];

    if format == "ogg" {
        args.push("-b:a".to_owned());
        args.push("208k".to_owned());
        args.push("-maxrate".to_owned());
        args.push("208k".to_owned());
        args.push("-bufsize".to_owned());
        args.push("208k".to_owned());
        args.push("-c:a".to_owned());
        args.push("libvorbis".to_owned());
    } else {
        args.push("-b:a".to_owned());
        args.push("192k".to_owned());
        args.push("-maxrate".to_owned());
        args.push("192k".to_owned());
        args.push("-bufsize".to_owned());
        args.push("192k".to_owned());
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

pub fn debloat(ffmpeg: &Path, input: &Path, format: &str, max_kbps: u64) -> anyhow::Result<PathBuf> {
    if max_kbps == 0 {
        return Ok(input.to_path_buf());
    }

    let ext = match format {
        "mp3" => "mp3",
        "ogg" => "ogg",
        _ => return Err(anyhow::anyhow!("Unknown format: {}", format)),
    };

    let bitrate_str = format!("{}k", max_kbps);
    let tmp = input.with_extension(format!("debloat.{ext}"));

    let mut args: Vec<String> = vec![
        "-y".to_owned(),
        "-i".to_owned(),
        input.to_string_lossy().to_string(),
        "-b:a".to_owned(),
        bitrate_str,
    ];
    if format == "ogg" {
        args.push("-c:a".to_owned());
        args.push("libvorbis".to_owned());
    }
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
        return Err(anyhow::anyhow!("ffmpeg: ошибка деблоатинга"));
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
    let keep_stem = keep.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path == keep {
                continue;
            }
            let name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            if !name.starts_with(keep_stem) {
                continue;
            }
            let ext = path.extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            let is_temp = CLEANUP_EXTS.contains(&ext);
            let is_silence_tmp = name.ends_with(&format!(".silence.{}", format));
            let is_debloat_tmp = name.ends_with(&format!(".debloat.{}", format));
            if is_temp || is_silence_tmp || is_debloat_tmp {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}
