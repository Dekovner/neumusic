use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;

use rfd::FileDialog;

use crate::{converter, downloader};
use std::process::Command;
#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[derive(PartialEq)]
enum Lang {
    Ru,
    En,
}

#[derive(PartialEq, Clone)]
enum OutputFormat {
    Mp3,
    Ogg,
}

pub struct NeuMusicApp {
    yt_dlp: PathBuf,
    ffmpeg: PathBuf,
    url: String,
    output_dir: PathBuf,

    convert_192: bool,
    add_silence: bool,
    silence_ms: String,
    playlist: bool,

    output_format: OutputFormat,
    lang: Lang,
    status: String,
    progress: f32,
    busy: bool,
    bg_task: Option<mpsc::Receiver<BgMsg>>,

    log: Vec<String>,
}

enum BgMsg {
    Status(String),
    Progress(f32),
    Error(String),
    Done,
}

fn t(lang: &Lang, ru: &str, en: &str) -> String {
    match lang {
        Lang::Ru => ru.to_owned(),
        Lang::En => en.to_owned(),
    }
}

fn paste_from_clipboard() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        let mut cmd = Command::new("powershell");
        cmd.args(["-command", "Get-Clipboard"]);
        cmd.creation_flags(0x08000000);
        cmd.output().ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Command::new("xclip")
            .args(["-o", "-selection", "clipboard"])
            .output().ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .or_else(|| {
                Command::new("xsel")
                    .args(["-b", "-o"])
                    .output().ok()
                    .filter(|o| o.status.success())
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            })
            .or_else(|| {
                Command::new("wl-paste")
                    .output().ok()
                    .filter(|o| o.status.success())
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            })
    }
}

impl NeuMusicApp {
    pub fn new(yt_dlp: PathBuf, ffmpeg: PathBuf) -> Self {
        let music_dir = dirs_audio_dir().unwrap_or_default();

        Self {
            yt_dlp,
            ffmpeg,
            output_dir: music_dir,
            convert_192: true,
            silence_ms: "2000".to_owned(),
            output_format: OutputFormat::Mp3,
            lang: Lang::En,
            status: "Ready".to_owned(),
            progress: 0.0,
            log: vec!["Ready".to_owned()],
            ..Default::default()
        }
    }

    fn toggle_lang(&mut self) {
        self.lang = match self.lang {
            Lang::En => {
                self.status = "Готов".to_owned();
                self.log = vec!["Готов".to_owned()];
                Lang::Ru
            }
            Lang::Ru => {
                self.status = "Ready".to_owned();
                self.log = vec!["Ready".to_owned()];
                Lang::En
            }
        };
    }

    fn pick_dir(&mut self) {
        if let Some(dir) = FileDialog::new().pick_folder() {
            self.output_dir = dir;
        }
    }

    fn start_download(&mut self, ctx: &egui::Context) {
        if self.url.trim().is_empty() || self.output_dir.as_os_str().is_empty() {
            self.status = t(&self.lang, 
                "Заполните URL и выберите папку.",
                "Fill in URL and select a folder.",
            );
            return;
        }

        self.busy = true;
        self.progress = 0.0;
        self.log.clear();
        self.status = t(&self.lang, "Загрузка...", "Downloading...");
        self.log.push(t(&self.lang, "Загрузка...", "Downloading..."));

        let yt_dlp = self.yt_dlp.clone();
        let ffmpeg = self.ffmpeg.clone();
        let url = self.url.trim().to_owned();
        let dir = self.output_dir.clone();
        let convert = self.convert_192;
        let format_str = match self.output_format {
            OutputFormat::Mp3 => "mp3",
            OutputFormat::Ogg => "ogg",
        }.to_owned();
        let silence = self.add_silence;
        let ms: u64 = self.silence_ms.parse().unwrap_or(2000);
        let playlist = self.playlist;
        let lang_is_en = matches!(self.lang, Lang::En);

        let (tx, rx) = mpsc::channel();
        self.bg_task = Some(rx);

        thread::spawn(move || {
            let result = Self::run_pipeline(
                &yt_dlp, &ffmpeg, &url, &dir, convert, &format_str, silence, ms, playlist, lang_is_en, &tx,
            );
            match result {
                Ok(()) => tx.send(BgMsg::Done).ok(),
                Err(e) => tx.send(BgMsg::Error(e.to_string())).ok(),
            }
        });

        ctx.request_repaint();
    }

    fn run_pipeline(
        yt_dlp: &Path,
        ffmpeg: &Path,
        url: &str,
        dir: &Path,
        convert: bool,
        format: &str,
        silence: bool,
        ms: u64,
        playlist: bool,
        en: bool,
        tx: &mpsc::Sender<BgMsg>,
    ) -> anyhow::Result<()> {
        let s = |ru: &str, en_str: &str| -> String {
            if en {
                en_str.to_owned()
            } else {
                ru.to_owned()
            }
        };

        tx.send(BgMsg::Progress(0.05)).ok();
        tx.send(BgMsg::Status(s("▶ Скачивание...", "▶ Downloading..."))).ok();
        let first = downloader::download_audio(yt_dlp, url, dir, playlist, ffmpeg.parent())?;
        tx.send(BgMsg::Progress(0.30)).ok();
        tx.send(BgMsg::Status(s("  ✓ Скачано", "  ✓ Downloaded"))).ok();
        let mut current = first;

        if convert {
            let (bitrate_label, codec_label) = match format {
                "ogg" => ("208 kbps", "OGG"),
                _ => ("192 kbps", "MP3"),
            };
            tx.send(BgMsg::Progress(0.40)).ok();
            tx.send(BgMsg::Status(s(
                &format!("▶ Конвертация в {} ({})...", bitrate_label, codec_label),
                &format!("▶ Converting to {} ({})...", bitrate_label, codec_label),
            )))
            .ok();
            current = converter::convert_audio(ffmpeg, &current, format)?;
            tx.send(BgMsg::Progress(0.60)).ok();
            tx.send(BgMsg::Status(s(
                "  ✓ Конвертировано",
                "  ✓ Converted",
            )))
            .ok();
        }

        if silence {
            tx.send(BgMsg::Progress(0.70)).ok();
            tx.send(BgMsg::Status(s(
                "▶ Добавление тишины...",
                "▶ Adding silence...",
            )))
            .ok();
            current = converter::add_silence(ffmpeg, &current, ms, format)?;
            tx.send(BgMsg::Progress(0.85)).ok();
            tx.send(BgMsg::Status(s(
                "  ✓ Тишина добавлена",
                "  ✓ Silence added",
            )))
            .ok();
        }

        tx.send(BgMsg::Progress(0.90)).ok();
        tx.send(BgMsg::Status(s(
            "▶ Очистка временных файлов...",
            "▶ Cleaning temp files...",
        )))
        .ok();
        converter::cleanup(dir, &current, format);
        tx.send(BgMsg::Progress(0.95)).ok();
        tx.send(BgMsg::Status(s("  ✓ Очищено", "  ✓ Cleaned"))).ok();

        tx.send(BgMsg::Progress(1.0)).ok();
        tx.send(BgMsg::Status(s("  ✓ Готово", "  ✓ Done"))).ok();
        Ok(())
    }

    fn handle_messages(&mut self, ctx: &egui::Context) {
        let rx = match self.bg_task.take() {
            Some(r) => r,
            None => return,
        };

        let mut should_end = false;
        while let Ok(msg) = rx.try_recv() {
            match msg {
                BgMsg::Status(s) => {
                    self.status = s.clone();
                    self.log.push(s);
                }
                BgMsg::Progress(p) => {
                    self.progress = p;
                }
                BgMsg::Error(e) => {
                    let prefix = t(&self.lang, "Ошибка: ", "Error: ");
                    let msg = format!("{prefix}{e}");
                    self.status = msg.clone();
                    self.log.push(msg);
                    should_end = true;
                }
                BgMsg::Done => {
                    should_end = true;
                }
            }
            ctx.request_repaint();
        }

        if should_end {
            self.busy = false;
        } else {
            self.bg_task = Some(rx);
        }
    }
}

fn dirs_audio_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let profile = std::env::var("USERPROFILE").ok()?;
        return Some(PathBuf::from(profile).join("Music"));
    }
    #[cfg(not(target_os = "windows"))]
    {
        let home = std::env::var("HOME").ok()?;
        let xdg = std::env::var("XDG_MUSIC_DIR").unwrap_or_default();
        if !xdg.is_empty() {
            let p = PathBuf::from(&xdg);
            if p.is_absolute() {
                return Some(p);
            }
        }
        Some(PathBuf::from(home).join("Music"))
    }
}

impl Default for NeuMusicApp {
    fn default() -> Self {
        Self {
            yt_dlp: PathBuf::new(),
            ffmpeg: PathBuf::new(),
            url: String::new(),
            output_dir: PathBuf::new(),
            convert_192: true,
            output_format: OutputFormat::Mp3,
            add_silence: false,
            silence_ms: "2000".to_owned(),
            playlist: false,
            lang: Lang::En,
            status: "Готов".to_owned(),
            progress: 0.0,
            busy: false,
            bg_task: None,
            log: Vec::new(),
        }
    }
}

impl eframe::App for NeuMusicApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_messages(ctx);

        if ctx.input(|i| i.modifiers.ctrl && i.events.iter().any(|e| matches!(e, egui::Event::Key { key: egui::Key::V, pressed: true, .. }))) {
            if let Some(text) = paste_from_clipboard() {
                self.url = text;
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("NeuMusic");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(if matches!(self.lang, Lang::Ru) { "EN" } else { "RU" }).clicked() {
                            self.toggle_lang();
                        }
                    });
                });

                ui.horizontal(|ui| {
                    ui.label(t(&self.lang, "URL:", "URL:"));
                    ui.text_edit_singleline(&mut self.url);
                    if ui.button("Paste").clicked() {
                        if let Some(text) = paste_from_clipboard() {
                            self.url = text;
                        } else if self.lang == Lang::Ru {
                            self.status = "Буфер обмена: установи xclip, xsel или wl-clipboard".to_owned();
                        } else {
                            self.status = "Clipboard: install xclip, xsel or wl-clipboard".to_owned();
                        }
                    }
                });

                ui.horizontal(|ui| {
                    if ui.button(t(&self.lang, "Обзор...", "Browse...")).clicked() {
                        self.pick_dir();
                    }
                    ui.label(self.output_dir.to_str().unwrap_or(
                        &t(&self.lang, "(не выбрано)", "(not selected)"),
                    ));
                });

                ui.separator();

                ui.horizontal(|ui| {
                    ui.checkbox(
                        &mut self.convert_192,
                        t(&self.lang, "Конвертировать аудио", "Convert audio"),
                    );
                    if self.convert_192 {
                        ui.radio_value(&mut self.output_format, OutputFormat::Mp3, "MP3 192kbps");
                        ui.radio_value(&mut self.output_format, OutputFormat::Ogg, "OGG 208kbps");
                    }
                });

                ui.checkbox(
                    &mut self.add_silence,
                    t(&self.lang, "Добавить тишину в начало", "Add silence at start"),
                );
                if self.add_silence {
                    ui.horizontal(|ui| {
                        ui.label(t(&self.lang, "Длительность (мс):", "Duration (ms):"));
                        ui.text_edit_singleline(&mut self.silence_ms);
                    });
                }

                ui.checkbox(
                    &mut self.playlist,
                    t(&self.lang, "Скачать весь плейлист", "Download entire playlist"),
                );

                ui.separator();

                let can_start = !self.busy
                    && !self.url.trim().is_empty()
                    && !self.output_dir.as_os_str().is_empty();

                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(can_start, egui::Button::new(t(&self.lang, "Скачать", "Download")))
                        .clicked()
                    {
                        self.start_download(ctx);
                    }
                    if self.busy {
                        ui.spinner();
                    }
                });

                ui.horizontal(|ui| {
                    ui.label(&self.status);
                });

                if self.progress > 0.0 {
                    ui.add(egui::ProgressBar::new(self.progress).show_percentage());
                }

                if !self.log.is_empty() {
                    ui.separator();
                    ui.label(t(&self.lang, "Лог:", "Log:"));
                    egui::ScrollArea::vertical()
                        .max_height(150.0)
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for line in &self.log {
                                ui.label(line);
                            }
                        });
                }
            });
        });
    }
}
