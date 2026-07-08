use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use rfd::FileDialog;

use crate::{converter, downloader};
use std::process::Command;
#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[derive(PartialEq, Clone, Copy)]
enum Lang {
    Ru,
    En,
    Es,
    Ja,
    Ko,
    Zh,
    Pt,
}

impl Lang {
    fn label(&self) -> &str {
        match self {
            Lang::Ru => "Русский",
            Lang::En => "English",
            Lang::Es => "Español",
            Lang::Ja => "日本語",
            Lang::Ko => "한국어",
            Lang::Zh => "中文",
            Lang::Pt => "Português",
        }
    }
}

#[derive(PartialEq, Clone)]
enum OutputFormat {
    Mp3,
    Ogg,
}

enum BgMsg {
    Status(String),
    Warning(String),
    Error(String),
    Progress(f32),
    Done,
}

enum UpdateState {
    Idle,
    Checking,
    Available(String),
    Current,
    Error(String),
}

#[derive(Clone)]
pub enum YtdlpUpdateEvent {
    Checking,
    Version(String),
    Current,
    Available(String),
    Downloaded,
    Error(String),
    Done,
}

enum YtdlpUpdateState {
    Idle,
    Checking,
    UpToDate,
    Downloading(String),
    Downloaded,
    Error(String),
}

fn t(lang: &Lang, ru: &str, en: &str, es: &str, ja: &str, ko: &str, zh: &str, pt: &str) -> String {
    match lang {
        Lang::Ru => ru,
        Lang::En => en,
        Lang::Es => es,
        Lang::Ja => ja,
        Lang::Ko => ko,
        Lang::Zh => zh,
        Lang::Pt => pt,
    }.to_owned()
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

    ytdlp_display: String,
    ytdlp_update_state: YtdlpUpdateState,
    ytdlp_update_rx: Option<mpsc::Receiver<YtdlpUpdateEvent>>,
    cancel_flag: Arc<AtomicBool>,
    active_child: Arc<Mutex<Option<Child>>>,
}

impl NeuMusicApp {
    pub fn new(
        yt_dlp: PathBuf,
        ffmpeg: PathBuf,
        saved_dir: Option<String>,
        ytdlp_update_rx: Option<mpsc::Receiver<YtdlpUpdateEvent>>,
    ) -> Self {
        let output_dir = saved_dir
            .map(PathBuf::from)
            .unwrap_or_default();

        Self {
            yt_dlp,
            ffmpeg,
            output_dir,
            url: String::new(),
            local_file: None,
            convert_192: true,
            output_format: OutputFormat::Mp3,
            add_silence: false,
            silence_ms: "2000".to_owned(),
            playlist: false,
            debloat_enabled: false,
            debloat_bitrate: 0,
            lang: Lang::En,
            status: "Ready".to_owned(),
            progress: 0.0,
            busy: false,
            bg_task: None,
            log: vec![("Ready".to_owned(), egui::Color32::WHITE)],
            update_state: UpdateState::Idle,
            update_rx: None,
            ytdlp_display: String::new(),
            ytdlp_update_state: YtdlpUpdateState::Idle,
            ytdlp_update_rx,
            cancel_flag: Arc::new(AtomicBool::new(false)),
            active_child: Arc::new(Mutex::new(None)),
        }
    }

    fn on_lang_changed(&mut self) {
        let msg = match self.lang {
            Lang::Ru => "Готов",
            Lang::En => "Ready",
            Lang::Es => "Listo",
            Lang::Ja => "準備完了",
            Lang::Ko => "준비 완료",
            Lang::Zh => "准备就绪",
            Lang::Pt => "Pronto",
        };
        self.status = msg.to_owned();
        self.log = vec![(msg.to_owned(), egui::Color32::WHITE)];
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
                "Ingrese URL o seleccione un archivo local.",
                "URLを入力するか、ローカルファイルを選択してください。",
                "URL을 입력하거나 로컬 파일을 선택하세요.",
                "输入URL或选择本地文件。",
                "Insira URL ou selecione um arquivo local.",
            );
            return;
        }
        if self.output_dir.as_os_str().is_empty() {
            self.status = t(&self.lang,
                "Выберите папку для сохранения.",
                "Select an output folder.",
                "Seleccione una carpeta de salida.",
                "出力フォルダを選択してください。",
                "출력 폴더를 선택하세요.",
                "选择输出文件夹。",
                "Selecione uma pasta de saída.",
            );
            return;
        }

        self.busy = true;
        self.cancel_flag.store(false, Ordering::Relaxed);
        self.progress = 0.0;
        self.log.clear();
        self.status = t(&self.lang, "Загрузка...", "Downloading...", "Descargando...", "ダウンロード中...", "다운로드 중...", "下载中...", "Baixando...");
        self.log.push((t(&self.lang, "Загрузка...", "Downloading...", "Descargando...", "ダウンロード中...", "다운로드 중...", "下载中...", "Baixando..."), egui::Color32::WHITE));

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
        let lang = self.lang;
        let cancel = self.cancel_flag.clone();
        let child_lock = self.active_child.clone();

        let (tx, rx) = mpsc::channel();
        self.bg_task = Some(rx);

        thread::spawn(move || {
            let result = Self::run_pipeline(
                &yt_dlp, &ffmpeg, &url, &dir,
                convert, &format_str, silence, ms, playlist, debloat, debloat_bitrate as u64, local_file,
                &lang, &tx, &cancel, &child_lock,
            );
            match result {
                Ok(()) => tx.send(BgMsg::Done).ok(),
                Err(e) => tx.send(BgMsg::Error(e.to_string())).ok(),
            }
        });

        ctx.request_repaint();
    }

    #[allow(clippy::too_many_arguments)]
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
        lang: &Lang,
        tx: &mpsc::Sender<BgMsg>,
        cancel_flag: &AtomicBool,
        child_lock: &Mutex<Option<Child>>,
    ) -> anyhow::Result<()> {
        let s = |ru: &str, en: &str, es: &str, ja: &str, ko: &str, zh: &str, pt: &str| -> String {
            match lang {
                Lang::Ru => ru,
                Lang::En => en,
                Lang::Es => es,
                Lang::Ja => ja,
                Lang::Ko => ko,
                Lang::Zh => zh,
                Lang::Pt => pt,
            }.to_owned()
        };

        let mut local_work_rename: Option<String> = None;
        let has_local = local_file.is_some();

        let snapshot: std::collections::HashSet<String> = if playlist && !has_local {
            std::fs::read_dir(dir).ok()
                .into_iter().flatten()
                .filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().to_str().map(String::from))
                .collect()
        } else {
            std::collections::HashSet::new()
        };

        let first_current: Option<PathBuf>;
        if let Some(ref local) = local_file {
            let name = local.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file")
                .to_owned();
            let dest = dir.join(&name);

            if dest == *local {
                let work_name = format!("_work_{}", name);
                let work = dir.join(&work_name);
                std::fs::copy(local, &work)?;
                first_current = Some(work);
                local_work_rename = Some(name.clone());
            } else {
                std::fs::copy(local, &dest)?;
                first_current = Some(dest);
            }
            tx.send(BgMsg::Status(s(
                &format!("▶ Файл: {}", name),
                &format!("▶ File: {}", name),
                &format!("▶ Archivo: {}", name),
                &format!("▶ ファイル: {}", name),
                &format!("▶ 파일: {}", name),
                &format!("▶ 文件: {}", name),
                &format!("▶ Arquivo: {}", name),
            ))).ok();
        } else {
            tx.send(BgMsg::Progress(0.05)).ok();
            tx.send(BgMsg::Status(s("▶ Скачивание...", "▶ Downloading...", "▶ Descargando...", "▶ ダウンロード中...", "▶ 다운로드 중...", "▶ 下载中...", "▶ Baixando..."))).ok();
            first_current = Some(downloader::download_audio(yt_dlp, url, dir, playlist, ffmpeg.parent(), cancel_flag, child_lock)?);
            tx.send(BgMsg::Progress(0.30)).ok();
            tx.send(BgMsg::Status(s("  ✓ Скачано", "  ✓ Downloaded", "  ✓ Descargado", "  ✓ ダウンロード完了", "  ✓ 다운로드 완료", "  ✓ 下载完成", "  ✓ Baixado"))).ok();
        }

        let files: Vec<PathBuf> = if has_local {
            vec![first_current.unwrap()]
        } else if playlist {
            downloader::find_all_audio(dir)
                .into_iter()
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| !snapshot.contains(n))
                        .unwrap_or(false)
                })
                .collect()
        } else {
            vec![first_current.unwrap()]
        };

        let target_kbps = match format { "ogg" => 208, _ => 192 };
        let total = files.len();
        let mut processed_first: Option<PathBuf> = None;

        for (idx, input) in files.iter().enumerate() {
            let mut current = input.clone();

            let src_kbps = converter::get_actual_bitrate(&current, ffmpeg)
                .ok().map(|b| b / 1000).unwrap_or(0);

            if convert {
                let codec_label = match format { "ogg" => "OGG", _ => "MP3" };
                tx.send(BgMsg::Progress(
                    0.30 + (idx as f32 / total as f32) * 0.65
                )).ok();
                tx.send(BgMsg::Status(s(
                    &format!("▶ Конвертация в {} kbps ({})...", target_kbps, codec_label),
                    &format!("▶ Converting to {} kbps ({})...", target_kbps, codec_label),
                    &format!("▶ Convirtiendo a {} kbps ({})...", target_kbps, codec_label),
                    &format!("▶ {} kbps ({}) に変換中...", target_kbps, codec_label),
                    &format!("▶ {} kbps ({}) 변환 중...", target_kbps, codec_label),
                    &format!("▶ 正在转换为 {} kbps ({})...", target_kbps, codec_label),
                    &format!("▶ Convertendo para {} kbps ({})...", target_kbps, codec_label),
                ))).ok();
                current = converter::convert_audio(ffmpeg, &current, format)?;

                if let Ok(bps) = converter::get_actual_bitrate(&current, ffmpeg) {
                    let kbps = (bps / 1000) as u64;
                    tx.send(BgMsg::Status(s(
                        &format!("  ✓ Конвертировано ({} kbps avg)", kbps),
                        &format!("  ✓ Converted ({} kbps avg)", kbps),
                        &format!("  ✓ Convertido ({} kbps promedio)", kbps),
                        &format!("  ✓ 変換完了 (平均 {} kbps)", kbps),
                        &format!("  ✓ 변환 완료 (평균 {} kbps)", kbps),
                        &format!("  ✓ 转换完成（平均 {} kbps）", kbps),
                        &format!("  ✓ Convertido (média {} kbps)", kbps),
                    ))).ok();
                } else {
                    tx.send(BgMsg::Status(s(
                        "  ✓ Конвертировано",
                        "  ✓ Converted",
                        "  ✓ Convertido",
                        "  ✓ 変換完了",
                        "  ✓ 변환 완료",
                        "  ✓ 转换完成",
                        "  ✓ Convertido",
                    ))).ok();
                }
            }

            if silence {
                tx.send(BgMsg::Status(s(
                    "▶ Добавление тишины...",
                    "▶ Adding silence...",
                    "▶ Agregando silencio...",
                    "▶ 無音を追加中...",
                    "▶ 무음 추가 중...",
                    "▶ 正在添加静音...",
                    "▶ Adicionando silêncio...",
                ))).ok();
                current = converter::add_silence(ffmpeg, &current, ms, format)?;
                tx.send(BgMsg::Status(s(
                    "  ✓ Тишина добавлена",
                    "  ✓ Silence added",
                    "  ✓ Silencio agregado",
                    "  ✓ 無音を追加しました",
                    "  ✓ 무음 추가 완료",
                    "  ✓ 静音已添加",
                    "  ✓ Silêncio adicionado",
                ))).ok();
            }

            if debloat && debloat_bitrate > 0 {
                let max_kbps = debloat_bitrate;
                tx.send(BgMsg::Status(s(
                    &format!("▶ Деблоатинг: cap {} kbps...", max_kbps),
                    &format!("▶ Debloat: cap {} kbps...", max_kbps),
                    &format!("▶ Reduciendo: límite {} kbps...", max_kbps),
                    &format!("▶ デブロート: 上限 {} kbps...", max_kbps),
                    &format!("▶ 디블로팅: 상한 {} kbps...", max_kbps),
                    &format!("▶ 去膨胀：上限 {} kbps...", max_kbps),
                    &format!("▶ Reduzindo: limite {} kbps...", max_kbps),
                ))).ok();
                current = converter::debloat(ffmpeg, &current, format, max_kbps)?;
                tx.send(BgMsg::Status(s(
                    &format!("  ✓ Деблоатинг завершён ({} kbps)", max_kbps),
                    &format!("  ✓ Debloat done ({} kbps)", max_kbps),
                    &format!("  ✓ Reducción completada ({} kbps)", max_kbps),
                    &format!("  ✓ デブロート完了 ({} kbps)", max_kbps),
                    &format!("  ✓ 디블로팅 완료 ({} kbps)", max_kbps),
                    &format!("  ✓ 去膨胀完成（{} kbps）", max_kbps),
                    &format!("  ✓ Redução concluída ({} kbps)", max_kbps),
                ))).ok();
            }

            let actual = converter::get_actual_bitrate(&current, ffmpeg)
                .ok().map(|b| b / 1000).unwrap_or(0);
            if actual > 0 {
                tx.send(BgMsg::Status(s(
                    &format!("  ✓ Итоговый битрейт: {} kbps", actual),
                    &format!("  ✓ Final bitrate: {} kbps", actual),
                    &format!("  ✓ Tasa de bits final: {} kbps", actual),
                    &format!("  ✓ 最終ビットレート: {} kbps", actual),
                    &format!("  ✓ 최종 비트레이트: {} kbps", actual),
                    &format!("  ✓ 最终比特率：{} kbps", actual),
                    &format!("  ✓ Taxa de bits final: {} kbps", actual),
                ))).ok();
            }

            converter::cleanup(dir, &current, format);

            if !debloat && src_kbps > 0 && actual > src_kbps && src_kbps < target_kbps {
                tx.send(BgMsg::Warning(s(
                    &format!("⚠ Возможно раздутие аудио: {} → {} kbps. Проверьте спектрограмму вручную.", src_kbps, actual),
                    &format!("⚠ Audio possibly bloated: {} → {} kbps. Manual spectrogram check required.", src_kbps, actual),
                    &format!("⚠ Posible inflado de audio: {} → {} kbps. Verifique el espectrograma manualmente.", src_kbps, actual),
                    &format!("⚠ オーディオが膨張している可能性があります: {} → {} kbps。手動でスペクトログラムを確認してください。", src_kbps, actual),
                    &format!("⚠ 오디오가 부풀려졌을 가능성: {} → {} kbps. 수동으로 스펙트로그램을 확인하세요.", src_kbps, actual),
                    &format!("⚠ 音频可能已膨胀：{} → {} kbps。请手动检查频谱图。", src_kbps, actual),
                    &format!("⚠ Áudio possivelmente inflado: {} → {} kbps. Verifique o espectrograma manualmente.", src_kbps, actual),
                ))).ok();
            }

            tx.send(BgMsg::Progress(
                0.30 + ((idx + 1) as f32 / total as f32) * 0.65
            )).ok();

            if idx == 0 {
                processed_first = Some(current);
            }
        }

        if let Some(ref orig_name) = local_work_rename {
            if let Some(ref processed) = processed_first {
                let final_path = dir.join(orig_name).with_extension(
                    processed.extension().unwrap_or_default()
                );
                std::fs::rename(processed, &final_path)?;
            }
        }

        tx.send(BgMsg::Progress(1.0)).ok();
        tx.send(BgMsg::Status(s("  ✓ Готово", "  ✓ Done", "  ✓ Hecho", "  ✓ 完了", "  ✓ 완료", "  ✓ 完成", "  ✓ Pronto"))).ok();

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
                    let prefix = t(&self.lang, "Ошибка: ", "Error: ", "Error: ", "エラー: ", "오류: ", "错误: ", "Erro: ");
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

impl Drop for NeuMusicApp {
    fn drop(&mut self) {
        self.cancel_flag.store(true, Ordering::Relaxed);
        if let Some(mut child) = self.active_child.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
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
            status: "Ready".to_owned(),
            progress: 0.0,
            busy: false,
            bg_task: None,
            log: Vec::new(),
            update_state: UpdateState::Idle,
            update_rx: None,
            ytdlp_display: String::new(),
            ytdlp_update_state: YtdlpUpdateState::Idle,
            ytdlp_update_rx: None,
            cancel_flag: Arc::new(AtomicBool::new(false)),
            active_child: Arc::new(Mutex::new(None)),
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

        if let Some(rx) = &self.ytdlp_update_rx {
            if let Ok(ev) = rx.try_recv() {
                match ev {
                    YtdlpUpdateEvent::Checking => {
                        self.ytdlp_display = String::new();
                        self.ytdlp_update_state = YtdlpUpdateState::Checking;
                    }
                    YtdlpUpdateEvent::Version(v) => {
                        self.ytdlp_display = format!("yt-dlp v{}", v);
                    }
                    YtdlpUpdateEvent::Current => {
                        self.ytdlp_update_state = YtdlpUpdateState::UpToDate;
                    }
                    YtdlpUpdateEvent::Available(v) => {
                        self.ytdlp_update_state = YtdlpUpdateState::Downloading(v);
                    }
                    YtdlpUpdateEvent::Downloaded => {
                        let v = match &self.ytdlp_update_state {
                            YtdlpUpdateState::Downloading(v) => v.clone(),
                            _ => String::new(),
                        };
                        self.ytdlp_display = format!("yt-dlp v{}", v);
                        self.ytdlp_update_state = YtdlpUpdateState::Downloaded;
                    }
                    YtdlpUpdateEvent::Error(e) => {
                        self.ytdlp_update_state = YtdlpUpdateState::Error(e);
                    }
                    YtdlpUpdateEvent::Done => {
                        self.ytdlp_update_rx = None;
                    }
                }
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
                        let prev_lang = self.lang;
                        egui::ComboBox::from_id_salt("lang_selector")
                            .selected_text(self.lang.label())
                            .width(120.0)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.lang, Lang::En, "English");
                                ui.selectable_value(&mut self.lang, Lang::Ru, "Русский");
                                ui.selectable_value(&mut self.lang, Lang::Es, "Español");
                                ui.selectable_value(&mut self.lang, Lang::Ja, "日本語");
                                ui.selectable_value(&mut self.lang, Lang::Ko, "한국어");
                                ui.selectable_value(&mut self.lang, Lang::Zh, "中文");
                                ui.selectable_value(&mut self.lang, Lang::Pt, "Português");
                            });
                        if self.lang != prev_lang {
                            self.on_lang_changed();
                        }
                    });
                });

                ui.horizontal(|ui| {
                    let url_enabled = self.local_file.is_none();
                    ui.label(t(&self.lang, "URL:", "URL:", "URL:", "URL:", "URL:", "URL:", "URL:"));
                    ui.add_enabled(url_enabled, egui::TextEdit::singleline(&mut self.url).desired_width(200.0));
                    if ui.add_enabled(url_enabled, egui::Button::new("Paste")).clicked() {
                        if let Some(text) = paste_from_clipboard() {
                            self.url = text;
                        } else {
                            self.status = t(&self.lang,
                                "Буфер обмена: установи xclip, xsel или wl-clipboard",
                                "Clipboard: install xclip, xsel or wl-clipboard",
                                "Portapapeles: instale xclip, xsel o wl-clipboard",
                                "クリップボード: xclip、xsel、またはwl-clipboardをインストールしてください",
                                "클립보드: xclip, xsel 또는 wl-clipboard를 설치하세요",
                                "剪贴板：请安装 xclip、xsel 或 wl-clipboard",
                                "Área de transferência: instale xclip, xsel ou wl-clipboard",
                            );
                        }
                    }
                    ui.add(egui::Separator::default().vertical());
                    if ui.button(t(&self.lang, "📁 Загрузить файл", "📁 Load file", "📁 Cargar archivo", "📁 ファイルを読み込む", "📁 파일 불러오기", "📁 加载文件", "📁 Carregar arquivo")).clicked() {
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
                    if ui.button(t(&self.lang, "Выбрать папку", "Choose directory", "Elegir carpeta", "フォルダを選択", "폴더 선택", "选择文件夹", "Escolher pasta")).clicked() {
                        self.pick_dir();
                    }
                    ui.label(self.output_dir.to_str().unwrap_or(
                        &t(&self.lang, "(не выбрано)", "(not selected)", "(no seleccionado)", "(未選択)", "(선택 안 됨)", "(未选择)", "(não selecionado)"),
                    ));
                });

                ui.separator();

                egui::CollapsingHeader::new(t(&self.lang, "Настройки", "Settings", "Configuración", "設定", "설정", "设置", "Configurações"))
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.checkbox(
                                &mut self.convert_192,
                                t(&self.lang, "Конвертировать аудио", "Convert audio", "Convertir audio", "音声を変換", "오디오 변환", "转换音频", "Converter áudio"),
                            );
                            if self.convert_192 {
                                ui.radio_value(&mut self.output_format, OutputFormat::Mp3, "MP3 192kbps");
                                ui.radio_value(&mut self.output_format, OutputFormat::Ogg, "OGG 208kbps");
                            }
                        });

                        ui.checkbox(
                            &mut self.add_silence,
                            t(&self.lang, "Добавить тишину в начало", "Add silence at start", "Agregar silencio al inicio", "先頭に無音を追加", "시작 부분에 무음 추가", "在开头添加静音", "Adicionar silêncio no início"),
                        );
                        if self.add_silence {
                            ui.horizontal(|ui| {
                                ui.label(t(&self.lang, "Длительность (мс):", "Duration (ms):", "Duración (ms):", "長さ (ミリ秒):", "길이 (ms):", "时长（毫秒）:", "Duração (ms):"));
                                ui.text_edit_singleline(&mut self.silence_ms);
                            });
                        }

                        ui.checkbox(
                            &mut self.playlist,
                            t(&self.lang, "Скачать весь плейлист", "Download entire playlist", "Descargar lista de reproducción completa", "プレイリスト全体をダウンロード", "전체 재생목록 다운로드", "下载整个播放列表", "Baixar lista de reprodução inteira"),
                        );

                        ui.separator();
                        ui.checkbox(
                            &mut self.debloat_enabled,
                            t(&self.lang, "Деблоатинг аудио", "Debloat audio", "Reducir tamaño de audio", "オーディオのデブロート", "오디오 디블로팅", "音频去膨胀", "Reduzir tamanho do áudio"),
                        );
                        if self.debloat_enabled {
                            let mut val = self.debloat_bitrate;
                            ui.add(egui::Slider::new(&mut val, 8..=320).text("kbps"));
                            self.debloat_bitrate = val;
                        }

                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.button(t(&self.lang, "Проверить обновления", "Check for updates", "Buscar actualizaciones", "アップデートを確認", "업데이트 확인", "检查更新", "Verificar atualizações")).clicked() {
                                self.check_for_updates();
                            }
                            match &self.update_state {
                                UpdateState::Idle => {}
                                UpdateState::Checking => {
                                    ui.spinner();
                                    ui.label(t(&self.lang, "Проверка...", "Checking...", "Verificando...", "確認中...", "확인 중...", "检查中...", "Verificando..."));
                                }
                                UpdateState::Available(v) => {
                                    ui.colored_label(egui::Color32::YELLOW, &t(
                                        &self.lang,
                                        &format!("Доступно обновление: {}", v),
                                        &format!("Update available: {}", v),
                                        &format!("Actualización disponible: {}", v),
                                        &format!("アップデートがあります: {}", v),
                                        &format!("업데이트 가능: {}", v),
                                        &format!("有可用更新: {}", v),
                                        &format!("Atualização disponível: {}", v),
                                    ));
                                    if ui.button("GitHub").clicked() {
                                        open_releases_page();
                                    }
                                }
                                UpdateState::Current => {
                                    ui.colored_label(egui::Color32::GREEN, t(&self.lang, "Последняя версия", "Up to date", "Actualizado", "最新です", "최신 버전입니다", "已是最新", "Atualizado"));
                                }
                                UpdateState::Error(e) => {
                                    ui.colored_label(egui::Color32::RED, e);
                                }
                            }
                        });

                        ui.separator();
                        ui.horizontal(|ui| {
                            match &self.ytdlp_update_state {
                                YtdlpUpdateState::Idle | YtdlpUpdateState::Checking => {
                                    ui.colored_label(egui::Color32::from_rgba_premultiplied(150, 150, 150, 160), "yt-dlp:");
                                    ui.spinner();
                                    ui.label(t(&self.lang,
                                        "Проверка обновлений...",
                                        "Checking for updates...",
                                        "Buscando actualizaciones...",
                                        "アップデートを確認中...",
                                        "업데이트 확인 중...",
                                        "正在检查更新...",
                                        "Verificando atualizações...",
                                    ));
                                }
                                YtdlpUpdateState::UpToDate => {
                                    ui.colored_label(egui::Color32::from_rgba_premultiplied(150, 150, 150, 160), &self.ytdlp_display);
                                    ui.colored_label(egui::Color32::GREEN, t(&self.lang,
                                        "✓ Актуальна",
                                        "✓ Up to date",
                                        "✓ Actualizado",
                                        "✓ 最新です",
                                        "✓ 최신 버전",
                                        "✓ 已是最新",
                                        "✓ Atualizado",
                                    ));
                                }
                                YtdlpUpdateState::Downloading(v) => {
                                    ui.colored_label(egui::Color32::from_rgba_premultiplied(150, 150, 150, 160), &self.ytdlp_display);
                                    ui.spinner();
                                    ui.colored_label(egui::Color32::YELLOW, &t(&self.lang,
                                        &format!("Скачиваю {}...", v),
                                        &format!("Downloading {}...", v),
                                        &format!("Descargando {}...", v),
                                        &format!("{} をダウンロード中...", v),
                                        &format!("{} 다운로드 중...", v),
                                        &format!("正在下载 {}...", v),
                                        &format!("Baixando {}...", v),
                                    ));
                                }
                                YtdlpUpdateState::Downloaded => {
                                    ui.colored_label(egui::Color32::from_rgba_premultiplied(150, 150, 150, 160), &self.ytdlp_display);
                                    ui.colored_label(egui::Color32::GREEN, t(&self.lang,
                                        "✓ Обновлён (перезапустите)",
                                        "✓ Updated (restart to use)",
                                        "✓ Actualizado (reiniciar)",
                                        "✓ 更新完了 (再起動してください)",
                                        "✓ 업데이트 완료 (재시작 필요)",
                                        "✓ 已更新（重启生效）",
                                        "✓ Atualizado (reiniciar)",
                                    ));
                                }
                                YtdlpUpdateState::Error(e) => {
                                    ui.colored_label(egui::Color32::from_rgba_premultiplied(150, 150, 150, 160), &self.ytdlp_display);
                                    ui.colored_label(egui::Color32::RED, t(&self.lang,
                                        &format!("⚠ {}", e),
                                        &format!("⚠ {}", e),
                                        &format!("⚠ {}", e),
                                        &format!("⚠ {}", e),
                                        &format!("⚠ {}", e),
                                        &format!("⚠ {}", e),
                                        &format!("⚠ {}", e),
                                    ));
                                }
                            }
                        });
                    });

                ui.separator();

                let has_url = !self.url.trim().is_empty();
                let has_local = self.local_file.is_some();
                let can_start = !self.busy && (has_url || has_local) && !self.output_dir.as_os_str().is_empty();
                let btn_label = if has_local {
                    t(&self.lang, "Конвертировать", "Convert", "Convertir", "変換", "변환", "转换", "Converter")
                } else {
                    t(&self.lang, "Скачать", "Download", "MATUSABOMBER", "ダウンロード", "다운로드", "下载", "Baixar")
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
                        if ui.button(t(&self.lang, "✕ Отмена", "✕ Cancel", "✕ Cancelar", "✕ キャンセル", "✕ 취소", "✕ 取消", "✕ Cancelar")).clicked() {
                            self.cancel_flag.store(true, Ordering::Relaxed);
                            if let Some(mut child) = self.active_child.lock().unwrap().take() {
                                let _ = child.kill();
                                let _ = child.wait();
                            }
                        }
                    }
                    if !self.busy && self.output_dir.as_os_str().is_empty() {
                        ui.colored_label(egui::Color32::GRAY, t(&self.lang,
                            "Выберите папку для сохранения",
                            "Select a download directory",
                            "Seleccione una carpeta de descarga",
                            "保存フォルダを選択してください",
                            "저장 폴더를 선택하세요",
                            "请选择下载目录",
                            "Selecione uma pasta de download",
                        ));
                    }
                });

                ui.horizontal(|ui| {
                    ui.label(&self.status);
                });

                if self.progress > 0.0 {
                    let mut bar = egui::ProgressBar::new(self.progress);
                    if self.progress >= 1.0 {
                        bar = bar.fill(egui::Color32::GREEN)
                                 .text(egui::RichText::new(t(&self.lang, "✓ Завершено", "✓ Completed", "✓ Completado", "✓ 完了", "✓ 완료", "✓ 完成", "✓ Concluído"))
                                     .color(egui::Color32::BLACK));
                    } else {
                        bar = bar.show_percentage();
                    }
                    ui.add(bar);
                }

                if !self.log.is_empty() {
                    ui.separator();
                    ui.label(t(&self.lang, "Лог:", "Log:", "Registro:", "ログ:", "로그:", "日志:", "Registro:"));
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
