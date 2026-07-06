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

    local_file: Option<PathBuf>,

    convert_192: bool,
    output_format: OutputFormat,
    add_silence: bool,
    silence_ms: String,
    playlist: bool,
    debloat_enabled: bool,
    debloat_bitrate: u32,

    lang: Lang,
    status: String,
    progress: f32,
    busy: bool,
    bg_task: Option<mpsc::Receiver<BgMsg>>,

    log: Vec<(String, egui::Color32)>,
    update_state: UpdateState,
    update_rx: Option<mpsc::Receiver<UpdateState>>,
}

enum BgMsg {
    Status(String),
    Progress(f32),
    Error(String),
    Done,
    Warning(String),
}

enum UpdateState {
    Idle,
    Checking,
    Available(String),
    Current,
    Error(String),
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
    pub fn new(yt_dlp: PathBuf, ffmpeg: PathBuf, saved_dir: Option<String>) -> Self {
        let output_dir = saved_dir
            .map(PathBuf::from)
            .unwrap_or_default();

        Self {
            yt_dlp,
            ffmpeg,
            output_dir,
            convert_192: true,
            silence_ms: "2000".to_owned(),
            output_format: OutputFormat::Mp3,
            debloat_enabled: false,
            debloat_bitrate: 0,
            lang: Lang::En,
            status: "Ready".to_owned(),
            progress: 0.0,
            log: vec![("Ready".to_owned(), egui::Color32::WHITE)],
            ..Default::default()
        }
    }

    fn toggle_lang(&mut self) {
        self.lang = match self.lang {
            Lang::En => {
                self.status = "Готов".to_owned();
                self.log = vec![("Готов".to_owned(), egui::Color32::WHITE)];
                Lang::Ru
            }
            Lang::Ru => {
                self.status = "Ready".to_owned();
                self.log = vec![("Ready".to_owned(), egui::Color32::WHITE)];
                Lang::En
            }
        };
    }

    fn pick_dir(&mut self) {
        if let Some(dir) = FileDialog::new().pick_folder() {
            self.output_dir = dir;
        }
    }

    fn pick_local_file(&mut self) {
        if let Some(path) = FileDialog::new()
            .add_filter("Audio", &["mp3", "m4a", "webm", "opus", "mka", "ogg", "aac", "wav", "flac", "wma"])
            .pick_file()
        {
            self.local_file = Some(path);
        }
    }

    fn clear_local_file(&mut self) {
        self.local_file = None;
    }

    fn start_processing(&mut self, ctx: &egui::Context) {
        let has_url = !self.url.trim().is_empty();
        let has_local = self.local_file.is_some();

        if !has_url && !has_local {
            self.status = t(&self.lang,
                "Введите URL или выберите локальный файл.",
                "Enter URL or select a local file.",
            );
            return;
        }
        if self.output_dir.as_os_str().is_empty() {
            self.status = t(&self.lang,
                "Выберите папку для сохранения.",
                "Select an output folder.",
            );
            return;
        }

        self.busy = true;
        self.progress = 0.0;
        self.log.clear();
        self.status = t(&self.lang, "Загрузка...", "Downloading...");
        self.log.push((t(&self.lang, "Загрузка...", "Downloading..."), egui::Color32::WHITE));

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
        let debloat = self.debloat_enabled;
        let debloat_bitrate = self.debloat_bitrate;
        let local_file = self.local_file.clone();
        let lang_is_en = matches!(self.lang, Lang::En);

        let (tx, rx) = mpsc::channel();
        self.bg_task = Some(rx);

        thread::spawn(move || {
            let result = Self::run_pipeline(
                &yt_dlp, &ffmpeg, &url, &dir,
                convert, &format_str, silence, ms, playlist, debloat, debloat_bitrate as u64, local_file,
                lang_is_en, &tx,
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
        debloat: bool,
        debloat_bitrate: u64,
        local_file: Option<PathBuf>,
        en: bool,
        tx: &mpsc::Sender<BgMsg>,
    ) -> anyhow::Result<()> {
        let s = |ru: &str, en_str: &str| -> String {
            if en { en_str.to_owned() } else { ru.to_owned() }
        };

        let mut current: PathBuf;
        let mut local_work_rename: Option<String> = None;

        if let Some(local) = local_file {
            let name = local.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file")
                .to_owned();
            let dest = dir.join(&name);

            if dest == local {
                let work_name = format!("_work_{}", name);
                let work = dir.join(&work_name);
                std::fs::copy(&local, &work)?;
                current = work;
                local_work_rename = Some(name.clone());
            } else {
                std::fs::copy(&local, &dest)?;
                current = dest;
            }
            tx.send(BgMsg::Status(s(
                &format!("▶ Файл: {}", name),
                &format!("▶ File: {}", name),
            ))).ok();
        } else {
            tx.send(BgMsg::Progress(0.05)).ok();
            tx.send(BgMsg::Status(s("▶ Скачивание...", "▶ Downloading..."))).ok();
            current = downloader::download_audio(yt_dlp, url, dir, playlist, ffmpeg.parent())?;
            tx.send(BgMsg::Progress(0.30)).ok();
            tx.send(BgMsg::Status(s("  ✓ Скачано", "  ✓ Downloaded"))).ok();
        }

        let src_kbps = converter::get_actual_bitrate(&current, ffmpeg)
            .ok().map(|b| b / 1000).unwrap_or(0);
        let target_kbps = match format { "ogg" => 208, _ => 192 };

        if convert {
            let codec_label = match format { "ogg" => "OGG", _ => "MP3" };
            let progress_start = if current.extension().map_or(false, |e| e == "mp3" || e == "ogg") {
                0.40
            } else {
                0.10
            };
            tx.send(BgMsg::Progress(progress_start)).ok();
            tx.send(BgMsg::Status(s(
                &format!("▶ Конвертация в {} kbps ({})...", target_kbps, codec_label),
                &format!("▶ Converting to {} kbps ({})...", target_kbps, codec_label),
            ))).ok();
            current = converter::convert_audio(ffmpeg, &current, format)?;
            tx.send(BgMsg::Progress(0.60)).ok();

            if let Ok(bps) = converter::get_actual_bitrate(&current, ffmpeg) {
                let kbps = (bps / 1000) as u64;
                tx.send(BgMsg::Status(s(
                    &format!("  ✓ Конвертировано ({} kbps avg)", kbps),
                    &format!("  ✓ Converted ({} kbps avg)", kbps),
                ))).ok();
            } else {
                tx.send(BgMsg::Status(s(
                    "  ✓ Конвертировано",
                    "  ✓ Converted",
                ))).ok();
            }
        }

        if silence {
            tx.send(BgMsg::Progress(0.65)).ok();
            tx.send(BgMsg::Status(s(
                "▶ Добавление тишины...",
                "▶ Adding silence...",
            ))).ok();
            current = converter::add_silence(ffmpeg, &current, ms, format)?;
            tx.send(BgMsg::Progress(0.80)).ok();
            tx.send(BgMsg::Status(s(
                "  ✓ Тишина добавлена",
                "  ✓ Silence added",
            ))).ok();
        }

        if debloat && debloat_bitrate > 0 {
            let max_kbps = debloat_bitrate;
            tx.send(BgMsg::Progress(0.85)).ok();
            tx.send(BgMsg::Status(s(
                &format!("▶ Деблоатинг: cap {} kbps...", max_kbps),
                &format!("▶ Debloat: cap {} kbps...", max_kbps),
            ))).ok();
            current = converter::debloat(ffmpeg, &current, format, max_kbps)?;
            tx.send(BgMsg::Progress(0.92)).ok();
            tx.send(BgMsg::Status(s(
                &format!("  ✓ Деблоатинг завершён ({} kbps)", max_kbps),
                &format!("  ✓ Debloat done ({} kbps)", max_kbps),
            ))).ok();
        }

        let actual = converter::get_actual_bitrate(&current, ffmpeg)
            .ok().map(|b| b / 1000).unwrap_or(0);
        if actual > 0 {
            tx.send(BgMsg::Status(s(
                &format!("  ✓ Итоговый битрейт: {} kbps", actual),
                &format!("  ✓ Final bitrate: {} kbps", actual),
            ))).ok();
        }

        tx.send(BgMsg::Progress(0.90)).ok();
        tx.send(BgMsg::Status(s(
            "▶ Очистка временных файлов...",
            "▶ Cleaning temp files...",
        ))).ok();
        converter::cleanup(dir, &current, format);
        tx.send(BgMsg::Progress(0.95)).ok();
        tx.send(BgMsg::Status(s("  ✓ Очищено", "  ✓ Cleaned"))).ok();

        if let Some(ref orig_name) = local_work_rename {
            let final_path = dir.join(orig_name).with_extension(
                current.extension().unwrap_or_default()
            );
            std::fs::rename(&current, &final_path)?;
        }

        tx.send(BgMsg::Progress(1.0)).ok();
        tx.send(BgMsg::Status(s("  ✓ Готово", "  ✓ Done"))).ok();

        if !debloat && src_kbps > 0 && actual > src_kbps && src_kbps < target_kbps {
            tx.send(BgMsg::Warning(s(
                &format!("⚠ Возможно раздутие аудио: {} → {} kbps. Проверьте спектрограмму вручную.", src_kbps, actual),
                &format!("⚠ Audio possibly bloated: {} → {} kbps. Manual spectrogram check required.", src_kbps, actual),
            ))).ok();
        }

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
                    self.log.push((s, egui::Color32::WHITE));
                }
                BgMsg::Progress(p) => {
                    self.progress = p;
                }
                BgMsg::Warning(w) => {
                    self.log.push((w, egui::Color32::YELLOW));
                }
                BgMsg::Error(e) => {
                    let prefix = t(&self.lang, "Ошибка: ", "Error: ");
                    let msg = format!("{prefix}{e}");
                    self.status = msg.clone();
                    self.log.push((msg, egui::Color32::RED));
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

    fn check_for_updates(&mut self) {
        self.update_state = UpdateState::Checking;
        let (tx, rx) = mpsc::channel();
        self.update_rx = Some(rx);
        let current = env!("CARGO_PKG_VERSION").to_owned();
        thread::spawn(move || {
            let state = (|| -> Result<UpdateState, String> {
                let resp = ureq::get("https://api.github.com/repos/Dekovner/neumusic/releases/latest")
                    .set("User-Agent", "neumusic/1.0")
                    .call()
                    .map_err(|e| format!("HTTP: {}", e))?;
                let body = resp.into_string()
                    .map_err(|e| format!("Read: {}", e))?;
                let tag = body.split("\"tag_name\":\"")
                    .nth(1)
                    .and_then(|s| s.split('"').next())
                    .unwrap_or("")
                    .trim_start_matches('v');
                if tag.is_empty() {
                    return Err("Parse error".to_owned());
                }
                if tag == current {
                    Ok(UpdateState::Current)
                } else {
                    Ok(UpdateState::Available(format!("v{}", tag)))
                }
            })();
            tx.send(state.unwrap_or_else(|e| UpdateState::Error(e))).ok();
        });
    }
}

fn open_releases_page() {
    let url = "https://github.com/Dekovner/neumusic/releases";
    #[cfg(target_os = "windows")]
    { let _ = std::process::Command::new("cmd").args(["/c", "start", url]).spawn(); }
    #[cfg(not(target_os = "windows"))]
    { let _ = std::process::Command::new("xdg-open").arg(url).spawn(); }
}

impl Default for NeuMusicApp {
    fn default() -> Self {
        Self {
            yt_dlp: PathBuf::new(),
            ffmpeg: PathBuf::new(),
            url: String::new(),
            output_dir: PathBuf::new(),
            local_file: None,
            convert_192: true,
            output_format: OutputFormat::Mp3,
            add_silence: false,
            silence_ms: "2000".to_owned(),
            playlist: false,
            debloat_enabled: false,
            debloat_bitrate: 0,
            lang: Lang::En,
            status: "Готов".to_owned(),
            progress: 0.0,
            busy: false,
            bg_task: None,
            log: Vec::new(),
            update_state: UpdateState::Idle,
            update_rx: None,
        }
    }
}

impl eframe::App for NeuMusicApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_messages(&ctx);

        if let Some(rx) = &self.update_rx {
            if let Ok(state) = rx.try_recv() {
                self.update_state = state;
                self.update_rx = None;
            }
        }

        if ui.input(|i| i.modifiers.ctrl && i.events.iter().any(|e| matches!(e, egui::Event::Key { key: egui::Key::V, pressed: true, .. }))) {
            if let Some(text) = paste_from_clipboard() {
                self.url = text;
            }
        }

        egui::CentralPanel::default().show(ui, |ui| {
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
                    let url_enabled = self.local_file.is_none();
                    ui.label(t(&self.lang, "URL:", "URL:"));
                    ui.add_enabled(url_enabled, egui::TextEdit::singleline(&mut self.url).desired_width(200.0));
                    if ui.add_enabled(url_enabled, egui::Button::new("Paste")).clicked() {
                        if let Some(text) = paste_from_clipboard() {
                            self.url = text;
                        } else if self.lang == Lang::Ru {
                            self.status = "Буфер обмена: установи xclip, xsel или wl-clipboard".to_owned();
                        } else {
                            self.status = "Clipboard: install xclip, xsel or wl-clipboard".to_owned();
                        }
                    }
                    ui.add(egui::Separator::default().vertical());
                    if ui.button(t(&self.lang, "📁 Загрузить файл", "📁 Load file")).clicked() {
                        self.pick_local_file();
                    }
                });

                if let Some(ref path) = self.local_file.clone() {
                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new(
                            path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
                        ).truncate());
                        if ui.button("✕").clicked() {
                            self.clear_local_file();
                        }
                    });
                }

                ui.horizontal(|ui| {
                    if ui.button(t(&self.lang, "Обзор...", "Browse...")).clicked() {
                        self.pick_dir();
                    }
                    ui.label(self.output_dir.to_str().unwrap_or(
                        &t(&self.lang, "(не выбрано)", "(not selected)"),
                    ));
                });

                ui.separator();

                egui::CollapsingHeader::new(t(&self.lang, "Настройки", "Settings"))
                    .default_open(false)
                    .show(ui, |ui| {
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

                        ui.checkbox(
                            &mut self.debloat_enabled,
                            t(&self.lang, "Деблоатинг аудио", "Debloat audio"),
                        );
                        if self.debloat_enabled {
                            let mut val = self.debloat_bitrate;
                            ui.add(egui::Slider::new(&mut val, 8..=320).text("kbps"));
                            self.debloat_bitrate = val;
                        }

                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.button(t(&self.lang, "Проверить обновления", "Check for updates")).clicked() {
                                self.check_for_updates();
                            }
                            match &self.update_state {
                                UpdateState::Idle => {}
                                UpdateState::Checking => {
                                    ui.spinner();
                                    ui.label(t(&self.lang, "Проверка...", "Checking..."));
                                }
                                UpdateState::Available(v) => {
                                    ui.colored_label(egui::Color32::YELLOW, &t(
                                        &self.lang,
                                        &format!("Доступно обновление: {}", v),
                                        &format!("Update available: {}", v),
                                    ));
                                    if ui.button("GitHub").clicked() {
                                        open_releases_page();
                                    }
                                }
                                UpdateState::Current => {
                                    ui.colored_label(egui::Color32::GREEN, t(&self.lang, "Последняя версия", "Up to date"));
                                }
                                UpdateState::Error(e) => {
                                    ui.colored_label(egui::Color32::RED, e);
                                }
                            }
                        });
                    });

                ui.separator();

                let has_url = !self.url.trim().is_empty();
                let has_local = self.local_file.is_some();
                let can_start = !self.busy && (has_url || has_local) && !self.output_dir.as_os_str().is_empty();
                let btn_label = if has_local {
                    t(&self.lang, "Конвертировать", "Convert")
                } else {
                    t(&self.lang, "Скачать", "Download")
                };

                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(can_start, egui::Button::new(btn_label))
                        .clicked()
                    {
                        self.start_processing(&ctx);
                    }
                    if self.busy {
                        ui.spinner();
                    }
                });

                ui.horizontal(|ui| {
                    ui.label(&self.status);
                });

                if self.progress > 0.0 {
                    let mut bar = egui::ProgressBar::new(self.progress);
                    if self.progress >= 1.0 {
                        bar = bar.fill(egui::Color32::GREEN)
                                 .text(egui::RichText::new(t(&self.lang, "✓ Завершено", "✓ Completed"))
                                     .color(egui::Color32::BLACK));
                    } else {
                        bar = bar.show_percentage();
                    }
                    ui.add(bar);
                }

                if !self.log.is_empty() {
                    ui.separator();
                    ui.label(t(&self.lang, "Лог:", "Log:"));
                    egui::ScrollArea::vertical()
                        .max_height(150.0)
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for (text, color) in &self.log {
                                ui.colored_label(*color, text);
                            }
                        });
                }
            });
        });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if !self.output_dir.as_os_str().is_empty() {
            storage.set_string("output_dir", self.output_dir.to_string_lossy().to_string());
        }
    }
}
