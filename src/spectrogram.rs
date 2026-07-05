use std::path::Path;
use std::process::Command;

use egui::ColorImage;

pub fn load_spectrogram(path: &Path) -> anyhow::Result<ColorImage> {
    let img = image::open(path).map_err(|e| anyhow::anyhow!("Ошибка загрузки PNG: {}", e))?;
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let pixels = rgba.into_raw();

    Ok(ColorImage::from_rgba_unmultiplied(size, &pixels))
}

pub fn get_audio_duration_secs(path: &Path) -> anyhow::Result<f64> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            &path.to_string_lossy(),
        ])
        .output()
        .map_err(|e| anyhow::anyhow!("ffprobe: {}", e))?;

    let s = String::from_utf8_lossy(&output.stdout);
    s.trim()
        .parse::<f64>()
        .map_err(|e| anyhow::anyhow!("ffprobe: неверная длительность: {}", e))
}
