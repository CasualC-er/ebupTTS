[dependencies]
eframe = "0.24"
egui = "0.24"
egui_extras = { version = "0.24", features = ["file"] }
rfd = "0.12"
tokio = { version = "1.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

use eframe::egui;
use egui::{CentralPanel, Grid, RichText, Slider, TopBottomPanel};
use rfd::FileDialog;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Debug, Clone)]
enum ConversionStatus {
    Idle,
    Running(String),
    Completed,
    Error(String),
}

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)]
struct ConverterApp {
    // File paths
    input_file: Option<PathBuf>,
    output_dir: Option<PathBuf>,

    // Conversion settings
    audio_format: AudioFormat,
    quality: f32,
    voice_speed: f32,
    voice_pitch: f32,
    workers: usize,
    aggressive_cleanup: bool,
    enable_cache: bool,

    // UI state
    #[serde(skip)]
    status: ConversionStatus,
    #[serde(skip)]
    progress_receiver: Option<mpsc::Receiver<ConversionProgress>>,
    #[serde(skip)]
    conversion_handle: Option<thread::JoinHandle<()>>,

    // Progress tracking
    #[serde(skip)]
    current_progress: ConversionProgress,
    #[serde(skip)]
    show_advanced: bool,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
enum AudioFormat {
    Vorbis,
    Flac,
    Mp3,
    Wav,
}

impl AudioFormat {
    fn as_str(&self) -> &'static str {
        match self {
            AudioFormat::Vorbis => "vorbis",
            AudioFormat::Flac => "flac",
            AudioFormat::Mp3 => "mp3",
            AudioFormat::Wav => "wav",
        }
    }

    fn display_name(&self) -> &'static str {
        match self {
            AudioFormat::Vorbis => "Ogg Vorbis (.ogg)",
            AudioFormat::Flac => "FLAC (.flac)",
            AudioFormat::Mp3 => "MP3 (.mp3)",
            AudioFormat::Wav => "WAV (.wav)",
        }
    }
}

#[derive(Debug, Clone, Default)]
struct ConversionProgress {
    current_chapter: String,
    chapters_completed: usize,
    total_chapters: usize,
    estimated_time_remaining: Option<std::time::Duration>,
}

impl Default for ConverterApp {
    fn default() -> Self {
        Self {
            input_file: None,
            output_dir: None,
            audio_format: AudioFormat::Vorbis,
            quality: 0.7,
            voice_speed: 1.0,
            voice_pitch: 1.0,
            workers: num_cpus::get(),
            aggressive_cleanup: true,
            enable_cache: true,
            status: ConversionStatus::Idle,
            progress_receiver: None,
            conversion_handle: None,
            current_progress: ConversionProgress::default(),
            show_advanced: false,
        }
    }
}

impl eframe::App for ConverterApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check for progress updates
        if let Some(receiver) = &self.progress_receiver {
            while let Ok(progress) = receiver.try_recv() {
                self.current_progress = progress;
                ctx.request_repaint();
            }
        }

        // Top panel with title
        TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("📚 EPUB to Audiobook Converter");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("💾 Save Settings").clicked() {
                        self.save_settings();
                    }
                    if ui.button("📁 Load Settings").clicked() {
                        self.load_settings();
                    }
                });
            });
            ui.separator();
        });

        // Main content
        CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                self.draw_file_selection(ui);
                ui.separator();
                self.draw_audio_settings(ui);
                ui.separator();
                self.draw_advanced_settings(ui);
                ui.separator();
                self.draw_conversion_controls(ui);
                ui.separator();
                self.draw_progress_section(ui);
            });
        });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }
}

impl ConverterApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        if let Some(storage) = cc.storage {
            return eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
        }
        Default::default()
    }

    fn draw_file_selection(&mut self, ui: &mut egui::Ui) {
        ui.heading("📂 File Selection");

        Grid::new("file_grid").num_columns(3).show(ui, |ui| {
            ui.label("Input EPUB:");
            if ui.button("📖 Select EPUB File").clicked() {
                if let Some(path) = FileDialog::new()
                    .add_filter("EPUB files", &["epub"])
                    .pick_file()
                    {
                        self.input_file = Some(path);
                    }
            }
            ui.label(
                self.input_file
                .as_ref()
                .map(|p| p.file_name().unwrap().to_string_lossy())
                .unwrap_or("No file selected".into())
            );
            ui.end_row();

            ui.label("Output Directory:");
            if ui.button("📁 Select Output Folder").clicked() {
                if let Some(path) = FileDialog::new().pick_folder() {
                    self.output_dir = Some(path);
                }
            }
            ui.label(
                self.output_dir
                .as_ref()
                .map(|p| p.to_string_lossy())
                .unwrap_or("No folder selected".into())
            );
            ui.end_row();
        });
    }

    fn draw_audio_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("🎵 Audio Settings");

        Grid::new("audio_grid").num_columns(2).show(ui, |ui| {
            ui.label("Output Format:");
            egui::ComboBox::from_label("")
            .selected_text(self.audio_format.display_name())
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.audio_format, AudioFormat::Vorbis, AudioFormat::Vorbis.display_name());
                ui.selectable_value(&mut self.audio_format, AudioFormat::Flac, AudioFormat::Flac.display_name());
                ui.selectable_value(&mut self.audio_format, AudioFormat::Mp3, AudioFormat::Mp3.display_name());
                ui.selectable_value(&mut self.audio_format, AudioFormat::Wav, AudioFormat::Wav.display_name());
            });
            ui.end_row();

            ui.label("Audio Quality:");
            ui.add(Slider::new(&mut self.quality, 0.1..=1.0).text("Quality"));
            ui.end_row();

            ui.label("Voice Speed:");
            ui.add(Slider::new(&mut self.voice_speed, 0.5..=2.0).text("Speed"));
            ui.end_row();

            ui.label("Voice Pitch:");
            ui.add(Slider::new(&mut self.voice_pitch, 0.5..=2.0).text("Pitch"));
            ui.end_row();
        });
    }

    fn draw_advanced_settings(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("⚙️ Advanced Settings");
            if ui.button(if self.show_advanced { "▼" } else { "▶" }).clicked() {
                self.show_advanced = !self.show_advanced;
            }
        });

        if self.show_advanced {
            Grid::new("advanced_grid").num_columns(2).show(ui, |ui| {
                ui.label("Worker Threads:");
                ui.add(Slider::new(&mut self.workers, 1..=num_cpus::get() * 2).text("Threads"));
                ui.end_row();

                ui.label("Aggressive Text Cleanup:");
                ui.checkbox(&mut self.aggressive_cleanup, "Enable aggressive preprocessing");
                ui.end_row();

                ui.label("Enable Caching:");
                ui.checkbox(&mut self.enable_cache, "Cache TTS results for faster re-runs");
                ui.end_row();
            });
        }
    }

    fn draw_conversion_controls(&mut self, ui: &mut egui::Ui) {
        ui.heading("🚀 Conversion");

        ui.horizontal(|ui| {
            let can_convert = self.input_file.is_some()
            && self.output_dir.is_some()
            && matches!(self.status, ConversionStatus::Idle | ConversionStatus::Completed | ConversionStatus::Error(_));

            if ui.button("▶️ Start Conversion")
                .ui_contains_pointer()
                && can_convert
                {
                    self.start_conversion();
                }

                if matches!(self.status, ConversionStatus::Running(_)) {
                    if ui.button("⏹️ Stop Conversion").clicked() {
                        self.stop_conversion();
                    }
                }

                // System dependencies check
                if ui.button("🔍 Check Dependencies").clicked() {
                    self.check_dependencies();
                }
        });

        // Status display
        match &self.status {
            ConversionStatus::Idle => {
                ui.label(RichText::new("Ready to convert").color(egui::Color32::GRAY));
            }
            ConversionStatus::Running(stage) => {
                ui.label(RichText::new(format!("Converting: {}", stage)).color(egui::Color32::BLUE));
            }
            ConversionStatus::Completed => {
                ui.label(RichText::new("✅ Conversion completed successfully!").color(egui::Color32::GREEN));
            }
            ConversionStatus::Error(error) => {
                ui.label(RichText::new(format!("❌ Error: {}", error)).color(egui::Color32::RED));
            }
        }
    }

    fn draw_progress_section(&mut self, ui: &mut egui::Ui) {
        if !matches!(self.status, ConversionStatus::Idle) {
            ui.heading("📊 Progress");

            if self.current_progress.total_chapters > 0 {
                let progress = self.current_progress.chapters_completed as f32 / self.current_progress.total_chapters as f32;
                let progress_bar = egui::ProgressBar::new(progress)
                .text(format!("{}/{} chapters",
                              self.current_progress.chapters_completed,
                              self.current_progress.total_chapters));
                ui.add(progress_bar);

                if !self.current_progress.current_chapter.is_empty() {
                    ui.label(format!("Current: {}", self.current_progress.current_chapter));
                }

                if let Some(eta) = &self.current_progress.estimated_time_remaining {
                    ui.label(format!("ETA: {:?}", eta));
                }
            } else {
                ui.spinner();
                ui.label("Initializing...");
            }
        }
    }

    fn start_conversion(&mut self) {
        let input_file = self.input_file.clone().unwrap();
        let output_dir = self.output_dir.clone().unwrap();
        let audio_format = self.audio_format.clone();
        let quality = self.quality;
        let voice_speed = self.voice_speed;
        let voice_pitch = self.voice_pitch;
        let workers = self.workers;
        let aggressive_cleanup = self.aggressive_cleanup;
        let enable_cache = self.enable_cache;

        let (progress_sender, progress_receiver) = mpsc::channel();
        self.progress_receiver = Some(progress_receiver);
        self.status = ConversionStatus::Running("Starting...".to_string());

        let handle = thread::spawn(move || {
            let result = run_conversion(
                input_file,
                output_dir,
                audio_format,
                quality,
                voice_speed,
                voice_pitch,
                workers,
                aggressive_cleanup,
                enable_cache,
                progress_sender,
            );

            if let Err(e) = result {
                eprintln!("Conversion failed: {}", e);
            }
        });

        self.conversion_handle = Some(handle);
    }

    fn stop_conversion(&mut self) {
        self.status = ConversionStatus::Idle;
        self.conversion_handle = None;
        self.progress_receiver = None;
    }

    fn check_dependencies(&mut self) {
        let deps = check_system_dependencies();
        let mut message = String::new();

        message.push_str("📋 System Dependencies Check:\n\n");

        // TTS Engines
        message.push_str("🎤 TTS Engines:\n");
        if deps.espeak_ng { message.push_str("✅ espeak-ng\n"); }
        else if deps.espeak { message.push_str("✅ espeak\n"); }
        else if deps.festival { message.push_str("✅ festival\n"); }
        else { message.push_str("❌ No TTS engine found\n"); }

        // Audio Encoders
        message.push_str("\n🎵 Audio Encoders:\n");
        if deps.oggenc { message.push_str("✅ oggenc (Vorbis)\n"); }
        if deps.flac { message.push_str("✅ flac (FLAC)\n"); }
        if deps.lame { message.push_str("✅ lame (MP3)\n"); }
        if deps.ffmpeg { message.push_str("✅ ffmpeg (All formats)\n"); }

        if !deps.oggenc && !deps.ffmpeg { message.push_str("❌ No Vorbis encoder\n"); }
        if !deps.flac && !deps.ffmpeg { message.push_str("❌ No FLAC encoder\n"); }
        if !deps.lame && !deps.ffmpeg { message.push_str("❌ No MP3 encoder\n"); }

        message.push_str("\n📦 Installation commands for Arch Linux:\n");
        message.push_str("sudo pacman -S espeak-ng vorbis-tools flac lame ffmpeg\n");

        // Show in a simple dialog (using native dialog)
        rfd::MessageDialog::new()
        .set_title("Dependencies Check")
        .set_description(&message)
        .show();
    }

    fn save_settings(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            if let Some(path) = FileDialog::new()
                .add_filter("JSON", &["json"])
                .set_file_name("epub_converter_settings.json")
                .save_file()
                {
                    let _ = std::fs::write(path, json);
                }
        }
    }

    fn load_settings(&mut self) {
        if let Some(path) = FileDialog::new()
            .add_filter("JSON", &["json"])
            .pick_file()
            {
                if let Ok(content) = std::fs::read_to_string(path) {
                    if let Ok(loaded) = serde_json::from_str::<ConverterApp>(&content) {
                        self.input_file = loaded.input_file;
                        self.output_dir = loaded.output_dir;
                        self.audio_format = loaded.audio_format;
                        self.quality = loaded.quality;
                        self.voice_speed = loaded.voice_speed;
                        self.voice_pitch = loaded.voice_pitch;
                        self.workers = loaded.workers;
                        self.aggressive_cleanup = loaded.aggressive_cleanup;
                        self.enable_cache = loaded.enable_cache;
                    }
                }
            }
    }
}

#[derive(Default)]
struct SystemDependencies {
    espeak_ng: bool,
    espeak: bool,
    festival: bool,
    oggenc: bool,
    flac: bool,
    lame: bool,
    ffmpeg: bool,
}

fn check_system_dependencies() -> SystemDependencies {
    let mut deps = SystemDependencies::default();

    let tools = [
        ("espeak-ng", &mut deps.espeak_ng),
        ("espeak", &mut deps.espeak),
        ("festival", &mut deps.festival),
        ("oggenc", &mut deps.oggenc),
        ("flac", &mut deps.flac),
        ("lame", &mut deps.lame),
        ("ffmpeg", &mut deps.ffmpeg),
    ];

    for (tool, flag) in &tools {
        *flag = Command::new("which")
        .arg(tool)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    }

    deps
}

fn run_conversion(
    input_file: PathBuf,
    output_dir: PathBuf,
    audio_format: AudioFormat,
    quality: f32,
    voice_speed: f32,
    voice_pitch: f32,
    workers: usize,
    aggressive_cleanup: bool,
    enable_cache: bool,
    progress_sender: mpsc::Sender<ConversionProgress>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Build command arguments
    let mut args = vec![
        "-i".to_string(),
        input_file.to_string_lossy().to_string(),
        "-o".to_string(),
        output_dir.to_string_lossy().to_string(),
        "-f".to_string(),
        audio_format.as_str().to_string(),
        "-q".to_string(),
        quality.to_string(),
        "-s".to_string(),
        voice_speed.to_string(),
        "-w".to_string(),
        workers.to_string(),
    ];

    if !aggressive_cleanup {
        args.push("--no-aggressive".to_string());
    }

    if !enable_cache {
        args.push("--no-cache".to_string());
    }

    // Send initial progress
    let _ = progress_sender.send(ConversionProgress {
        current_chapter: "Initializing...".to_string(),
                                 chapters_completed: 0,
                                 total_chapters: 0,
                                 estimated_time_remaining: None,
    });

    // Find the converter binary
    let converter_path = std::env::current_exe()?
    .parent()
    .unwrap()
    .join("epub_audiobook_converter");

    // Run the converter
    let mut child = Command::new(&converter_path)
    .args(&args)
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()?;

    // Monitor output for progress updates
    if let Some(stdout) = child.stdout.take() {
        use std::io::{BufRead, BufReader};
        let reader = BufReader::new(stdout);

        for line in reader.lines() {
            if let Ok(line) = line {
                // Parse progress from output
                if line.contains("Found") && line.contains("chapters") {
                    if let Some(total) = extract_number_from_line(&line, "Found", "chapters") {
                        let _ = progress_sender.send(ConversionProgress {
                            current_chapter: "Processing chapters...".to_string(),
                                                     chapters_completed: 0,
                                                     total_chapters: total,
                                                     estimated_time_remaining: None,
                        });
                    }
                } else if line.contains("Converting chapter") {
                    // Extract chapter info if available
                    let _ = progress_sender.send(ConversionProgress {
                        current_chapter: line.clone(),
                                                 chapters_completed: 0, // Would need more parsing
                                                 total_chapters: 0,     // Would need state tracking
                                                 estimated_time_remaining: None,
                    });
                }
            }
        }
    }

    let output = child.wait_with_output()?;

    if output.status.success() {
        let _ = progress_sender.send(ConversionProgress {
            current_chapter: "Completed!".to_string(),
                                     chapters_completed: 100,
                                     total_chapters: 100,
                                     estimated_time_remaining: None,
        });
        Ok(())
    } else {
        let error = String::from_utf8_lossy(&output.stderr);
        Err(format!("Conversion failed: {}", error).into())
    }
}

fn extract_number_from_line(line: &str, before: &str, after: &str) -> Option<usize> {
    if let Some(start) = line.find(before) {
        if let Some(end) = line[start..].find(after) {
            let number_part = &line[start + before.len()..start + end];
            return number_part.trim().parse().ok();
        }
    }
    None
}

fn main() -> Result<(), eframe::Error> {
    env_logger::init(); // Log to stderr (if you want to see it).

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
        .with_inner_size([800.0, 600.0])
        .with_min_inner_size([600.0, 400.0])
        .with_icon(eframe::icon_data::from_png_bytes(&[]).unwrap_or_default()),
        ..Default::default()
    };

    eframe::run_native(
        "EPUB to Audiobook Converter",
        options,
        Box::new(|cc| Box::new(ConverterApp::new(cc))),
    )
}
