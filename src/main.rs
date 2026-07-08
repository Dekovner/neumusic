use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use std::process::Command;

use rust_embed::RustEmbed;

#[cfg(target_os = "windows")]
fn die(msg: &str) -> ! {
    let wide: Vec<u16> = msg.encode_utf16().chain([0]).collect();
    let title: Vec<u16> = "NeuMusic\0".encode_utf16().collect();
    unsafe {
        extern "system" {
            fn MessageBoxW(
                hWnd: *const u8,
                lpText: *const u16,
                lpCaption: *const u16,
                uType: u32,
            ) -> i32;
        }
        MessageBoxW(std::ptr::null(), wide.as_ptr(), title.as_ptr(), 0x00000010);
    }
    std::process::exit(1);
}

#[cfg(not(target_os = "windows"))]
fn die(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(1);
}

fn load_icon() -> egui::IconData {
    let png_bytes = include_bytes!("../neumusic.png");
    let img = image::load_from_memory(png_bytes)
        .expect("Failed to load icon")
        .to_rgba8();
    let (w, h) = img.dimensions();
    egui::IconData {
        rgba: img.into_raw(),
        width: w,
        height: h,
    }
}

#[derive(RustEmbed)]
#[folder = "assets/"]
struct Assets;

fn extract_binary(name: &str) -> Option<PathBuf> {
    let embedded = Assets::get(name)?;
    let tmp = std::env::temp_dir().join(format!("neumusic-{name}"));
    std::fs::write(&tmp, embedded.data).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)).ok()?;
    }
    Some(tmp)
}

fn extract_yt_dlp() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        extract_binary("yt-dlp.exe")
    }
    #[cfg(not(target_os = "windows"))]
    {
        extract_binary("yt-dlp")
    }
}

fn yt_dlp_cache_dir() -> PathBuf {
    let base = if cfg!(target_os = "linux") {
        std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()))
                    .join(".local/share")
            })
    } else if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()))
            .join("Library/Application Support")
    } else if cfg!(target_os = "windows") {
        PathBuf::from(std::env::var("APPDATA").unwrap_or_else(|_| "C:\\temp".into()))
    } else {
        std::env::temp_dir()
    };
    base.join("neumusic")
}

fn yt_dlp_version(path: &Path) -> Option<String> {
    Command::new(path)
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_owned();
            if s.is_empty() { None } else { Some(s) }
        })
}

fn download_ytdlp(url: &str, dest: &Path) -> Result<(), String> {
    let resp = ureq::get(url)
        .call()
        .map_err(|e| format!("HTTP: {}", e))?;
    let mut data = Vec::new();
    resp.into_reader()
        .read_to_end(&mut data)
        .map_err(|e| format!("Read: {}", e))?;
    let tmp = dest.with_extension(".tmp");
    std::fs::write(&tmp, &data).map_err(|e| format!("Write: {}", e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("Chmod: {}", e))?;
    }
    std::fs::rename(&tmp, dest).map_err(|e| format!("Rename: {}", e))?;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn resolve_ffmpeg() -> Option<PathBuf> {
    let output = Command::new("which").arg("ffmpeg").output().ok()?;
    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path.is_empty() {
            return Some(PathBuf::from(path));
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn extract_ffmpeg() -> Option<PathBuf> {
    use std::io::Write;

    let dir = std::env::temp_dir().join("neumusic-ffmpeg");
    std::fs::create_dir_all(&dir).ok()?;

    let ffmpeg_path = dir.join("ffmpeg.exe");
    let embedded = WindowsAssets::get("ffmpeg.exe")?;
    let mut f = std::fs::File::create(&ffmpeg_path).ok()?;
    f.write_all(&embedded.data).ok()?;
    drop(f);

    if let Some(ffprobe) = WindowsAssets::get("ffprobe.exe") {
        let ffprobe_path = dir.join("ffprobe.exe");
        if let Ok(mut f) = std::fs::File::create(&ffprobe_path) {
            let _ = f.write_all(&ffprobe.data);
        }
    }

    Some(ffmpeg_path)
}

fn main() -> eframe::Result<()> {
    // 1. Extract embedded yt-dlp (fallback)
    let embedded = extract_yt_dlp().unwrap_or_else(|| {
        die("Error: yt-dlp not found in embedded resources. Rebuild the application.");
    });

    // 2. Set up cache dir
    let cache_dir = yt_dlp_cache_dir();
    let _ = std::fs::create_dir_all(&cache_dir);

    #[cfg(target_os = "windows")]
    const BINARY_NAME: &str = "yt-dlp.exe";
    #[cfg(not(target_os = "windows"))]
    const BINARY_NAME: &str = "yt-dlp";

    let cached = cache_dir.join(BINARY_NAME);

    // 3. Seed cache from embedded if needed
    if !cached.exists() || yt_dlp_version(&cached).is_none() {
        let _ = std::fs::copy(&embedded, &cached);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&cached, std::fs::Permissions::from_mode(0o755));
        }
    }

    // 4. Use cached if valid, else fallback to embedded
    let yt_dlp = if yt_dlp_version(&cached).is_some() {
        cached
    } else {
        embedded
    };

    // 5. Background updater channel
    let (update_tx, update_rx) = mpsc::channel::<neumusic::app::YtdlpUpdateEvent>();

    // 6. Spawn background updater
    let yt_dlp_for_updater = yt_dlp.clone();
    let cache_dir_for_updater = cache_dir.clone();
    std::thread::spawn(move || {
        let emit = |ev: neumusic::app::YtdlpUpdateEvent| { let _ = update_tx.send(ev); };

        emit(neumusic::app::YtdlpUpdateEvent::Checking);

        let current_ver = yt_dlp_version(&yt_dlp_for_updater).unwrap_or_default();
        emit(neumusic::app::YtdlpUpdateEvent::Version(current_ver.clone()));

        let result = (|| -> Result<String, String> {
            let resp = ureq::get("https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest")
                .set("User-Agent", "neumusic/1.0")
                .call()
                .map_err(|e| format!("HTTP: {}", e))?;
            let body = resp.into_string()
                .map_err(|e| format!("Read: {}", e))?;
            let tag = body.split("\"tag_name\":\"")
                .nth(1)
                .and_then(|s| s.split('"').next())
                .ok_or_else(|| "Parse error".to_owned())?;
            Ok(tag.to_owned())
        })();

        match result {
            Ok(latest) => {
                if latest == current_ver {
                    emit(neumusic::app::YtdlpUpdateEvent::Current);
                } else {
                    emit(neumusic::app::YtdlpUpdateEvent::Available(latest.clone()));

                    let url = format!(
                        "https://github.com/yt-dlp/yt-dlp/releases/download/{}/yt-dlp{}",
                        latest,
                        if cfg!(target_os = "windows") { ".exe" } else { "" }
                    );
                    let dest = cache_dir_for_updater.join(BINARY_NAME);
                    match download_ytdlp(&url, &dest) {
                        Ok(()) => emit(neumusic::app::YtdlpUpdateEvent::Downloaded),
                        Err(e) => emit(neumusic::app::YtdlpUpdateEvent::Error(e)),
                    }
                }
            }
            Err(e) => emit(neumusic::app::YtdlpUpdateEvent::Error(e)),
        }
        emit(neumusic::app::YtdlpUpdateEvent::Done);
    });

    #[cfg(not(target_os = "windows"))]
    let ffmpeg = resolve_ffmpeg().unwrap_or_else(|| {
        die("Error: ffmpeg not found. Install it and try again.");
    });

    #[cfg(target_os = "windows")]
    let ffmpeg = extract_ffmpeg().unwrap_or_else(|| {
        die("Error: ffmpeg.exe not found in embedded resources.");
    });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([560.0, 380.0])
            .with_resizable(true)
            .with_icon(load_icon()),
        ..Default::default()
    };

    eframe::run_native(
        "NeuMusic",
        options,
        Box::new(move |cc: &eframe::CreationContext| {
            let mut fonts = egui::FontDefinitions::default();

            {
                let jp: std::sync::Arc<egui::FontData> = egui::FontData::from_static(
                    include_bytes!("../assets/NotoSansJP-Regular.otf"),
                ).into();
                let kr: std::sync::Arc<egui::FontData> = egui::FontData::from_static(
                    include_bytes!("../assets/NotoSansKR-Regular.otf"),
                ).into();
                let sc: std::sync::Arc<egui::FontData> = egui::FontData::from_static(
                    include_bytes!("../assets/NotoSansSC-Regular.otf"),
                ).into();

                fonts.font_data.insert("noto-jp".into(), jp);
                fonts.font_data.insert("noto-kr".into(), kr);
                fonts.font_data.insert("noto-sc".into(), sc);

                fonts.families.entry(egui::FontFamily::Proportional).or_default()
                    .push("noto-jp".into());
                fonts.families.entry(egui::FontFamily::Proportional).or_default()
                    .push("noto-sc".into());
                fonts.families.entry(egui::FontFamily::Proportional).or_default()
                    .push("noto-kr".into());
            }

            cc.egui_ctx.set_fonts(fonts);

            let saved_dir = cc.storage.and_then(|s| s.get_string("output_dir"));
            Ok(Box::new(neumusic::NeuMusicApp::new(yt_dlp, ffmpeg, saved_dir, Some(update_rx))))
        }),
    )
}

#[cfg(target_os = "windows")]
#[derive(RustEmbed)]
#[folder = "assets-windows/"]
struct WindowsAssets;
