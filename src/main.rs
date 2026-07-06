use std::path::PathBuf;

#[cfg(not(target_os = "windows"))]
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
    let yt_dlp = extract_yt_dlp().unwrap_or_else(|| {
        die("Error: yt-dlp not found in embedded resources. Rebuild the application.");
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
            Ok(Box::new(neumusic::NeuMusicApp::new(yt_dlp, ffmpeg, saved_dir)))
        }),
    )
}

#[cfg(target_os = "windows")]
#[derive(RustEmbed)]
#[folder = "assets-windows/"]
struct WindowsAssets;
