use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
#[cfg(windows)]
use std::os::windows::process::CommandExt;

pub const AUDIO_EXTS: &[&str] = &[
    "mp3", "m4a", "webm", "opus", "mka", "ogg", "aac", "wav", "flac",
];

pub fn download_audio(
    yt_dlp: &Path,
    url: &str,
    dir: &Path,
    playlist: bool,
    ffmpeg_dir: Option<&Path>,
    cancel_flag: &AtomicBool,
    child_lock: &Mutex<Option<Child>>,
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

    fn run_ytdlp_cancelable(
        args: &[String],
        yt_dlp: &Path,
        cancel: &AtomicBool,
        child_lock: &Mutex<Option<Child>>,
    ) -> anyhow::Result<bool> {
        let mut cmd = Command::new(yt_dlp);
        cmd.args(args)
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit());
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);
        let child = cmd.spawn()
            .map_err(|e| anyhow::anyhow!("Не удалось запустить yt-dlp: {}", e))?;

        *child_lock.lock().unwrap() = Some(child);

        loop {
            if cancel.load(Ordering::Relaxed) {
                let _ = child_lock.lock().unwrap().take()
                    .map(|mut c| { let _ = c.kill(); let _ = c.wait(); });
                return Err(anyhow::anyhow!("Отменено пользователем"));
            }

            let mut guard = child_lock.lock().unwrap();
            if let Some(ref mut c) = *guard {
                match c.try_wait() {
                    Ok(Some(status)) => {
                        drop(guard);
                        *child_lock.lock().unwrap() = None;
                        return Ok(status.success());
                    }
                    Ok(None) => {}
                    Err(e) => {
                        drop(guard);
                        *child_lock.lock().unwrap() = None;
                        return Err(anyhow::anyhow!("Ошибка yt-dlp: {}", e));
                    }
                }
            } else {
                return Err(anyhow::anyhow!("Процесс yt-dlp был завершён"));
            }

            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    if run_ytdlp_cancelable(&args, yt_dlp, cancel_flag, child_lock)? {
        return find_newest_audio(dir)
            .ok_or_else(|| anyhow::anyhow!("Не найден скачанный аудиофайл"));
    }

    if cancel_flag.load(Ordering::Relaxed) {
        return Err(anyhow::anyhow!("Отменено"));
    }

    let mut fallback_args = vec!["--extractor-args".to_owned(), "soundcloud:formats=*".to_owned()];
    fallback_args.extend(args);

    if run_ytdlp_cancelable(&fallback_args, yt_dlp, cancel_flag, child_lock)? {
        return find_newest_audio(dir)
            .ok_or_else(|| anyhow::anyhow!("Не найден скачанный аудиофайл"));
    }

    Err(anyhow::anyhow!("yt-dlp: ошибка загрузки. Возможно, контент защищён DRM."))
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

pub fn find_all_audio(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir).ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| AUDIO_EXTS.contains(&ext))
                .unwrap_or(false)
        })
        .map(|e| e.path())
        .collect();
    files.sort_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
    files
}
