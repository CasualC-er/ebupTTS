[dependencies]
clap = { version = "4.0", features = ["derive"] }
epub = "2.0"
html2text = "0.6"
regex = "1.10"
rayon = "1.8"
tokio = { version = "1.0", features = ["full"] }
reqwest = { version = "0.11", features = ["json"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tempfile = "3.8"
sha2 = "0.10"
lru = "0.12"
indicatif = { version = "0.17", features = ["rayon"] }
symphonia = { version = "0.5", features = ["all"] }
hound = "3.5"
rodio = { version = "0.17", features = ["vorbis"] }

use clap::{Arg, Command};
use epub::doc::EpubDoc;
use html2text::from_read;
use indicatif::{ParallelProgressIterator, ProgressBar, ProgressStyle};
use lru::LruCache;
use rayon::prelude::*;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Config {
    sample_rate: u32,
    voice_speed: f32,
    voice_pitch: f32,
    output_format: AudioFormat,
    quality: f32,
    chunk_size: usize,
    max_workers: usize,
    cache_enabled: bool,
    preprocessing_aggressive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum AudioFormat {
    Vorbis,
    Flac,
    Mp3,
    Wav,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            sample_rate: 22050,
            voice_speed: 1.0,
            voice_pitch: 1.0,
            output_format: AudioFormat::Vorbis,
            quality: 0.7,
            chunk_size: 1000,
            max_workers: num_cpus::get(),
            cache_enabled: true,
            preprocessing_aggressive: true,
        }
    }
}

#[derive(Debug)]
struct Chapter {
    title: String,
    content: String,
    order: usize,
    word_count: usize,
}

struct TextProcessor {
    cleanup_regex: Vec<(Regex, &'static str)>,
    sentence_splitter: Regex,
    word_cache: Arc<Mutex<LruCache<String, String>>>,
}

impl TextProcessor {
    fn new() -> Self {
        let cleanup_patterns = vec![
            // Remove HTML entities and special characters
            (Regex::new(r"&[a-zA-Z0-9#]+;").unwrap(), " "),
            // Normalize whitespace
            (Regex::new(r"\s+").unwrap(), " "),
            // Fix common OCR errors
            (Regex::new(r"\bl\b").unwrap(), "I"), // lowercase L to I
            (Regex::new(r"\bO\b").unwrap(), "0"), // O to zero in numbers
            // Remove page numbers and references
            (Regex::new(r"\b[Pp]age\s+\d+\b").unwrap(), ""),
            (Regex::new(r"\b\d+\s*[-–—]\s*\d+\b").unwrap(), ""),
            // Fix quotation marks
            (Regex::new(r"[""''`]").unwrap(), "\""),
            // Normalize dashes
            (Regex::new(r"[–—]").unwrap(), "-"),
            // Remove multiple periods
            (Regex::new(r"\.{3,}").unwrap(), "..."),
            // Fix spacing around punctuation
            (Regex::new(r"\s+([,.!?;:])").unwrap(), "$1"),
            (Regex::new(r"([,.!?;:])\s+").unwrap(), "$1 "),
        ];

        Self {
            cleanup_regex: cleanup_patterns,
            sentence_splitter: Regex::new(r"[.!?]+\s+").unwrap(),
            word_cache: Arc::new(Mutex::new(LruCache::new(
                NonZeroUsize::new(10000).unwrap(),
            ))),
        }
    }

    fn clean_text(&self, text: &str, aggressive: bool) -> String {
        let mut cleaned = text.to_string();

        // Apply basic cleanup patterns
        for (regex, replacement) in &self.cleanup_regex {
            cleaned = regex.replace_all(&cleaned, *replacement).to_string();
        }

        if aggressive {
            // Additional aggressive cleaning
            cleaned = self.fix_hyphenation(&cleaned);
            cleaned = self.normalize_abbreviations(&cleaned);
            cleaned = self.fix_sentence_boundaries(&cleaned);
        }

        // Final cleanup
        cleaned.trim().to_string()
    }

    fn fix_hyphenation(&self, text: &str) -> String {
        // Fix words split across lines
        let hyphen_regex = Regex::new(r"(\w+)-\s*\n\s*(\w+)").unwrap();
        hyphen_regex.replace_all(text, "$1$2").to_string()
    }

    fn normalize_abbreviations(&self, text: &str) -> String {
        let mut result = text.to_string();

        // Common abbreviations that should be expanded for better TTS
        let abbreviations = vec![
            ("Mr.", "Mister"),
            ("Mrs.", "Missus"),
            ("Dr.", "Doctor"),
            ("Prof.", "Professor"),
            ("St.", "Saint"),
            ("vs.", "versus"),
            ("etc.", "etcetera"),
            ("i.e.", "that is"),
            ("e.g.", "for example"),
        ];

        for (abbrev, expansion) in abbreviations {
            let pattern = format!(r"\b{}\b", regex::escape(abbrev));
            let regex = Regex::new(&pattern).unwrap();
            result = regex.replace_all(&result, expansion).to_string();
        }

        result
    }

    fn fix_sentence_boundaries(&self, text: &str) -> String {
        // Ensure proper spacing after sentence endings
        let sentence_regex = Regex::new(r"([.!?])\s*([A-Z])").unwrap();
        sentence_regex.replace_all(text, "$1 $2").to_string()
    }

    fn split_into_chunks(&self, text: &str, chunk_size: usize) -> Vec<String> {
        let sentences: Vec<&str> = self.sentence_splitter.split(text).collect();
        let mut chunks = Vec::new();
        let mut current_chunk = String::new();
        let mut current_length = 0;

        for sentence in sentences {
            let sentence_length = sentence.len();

            if current_length + sentence_length > chunk_size && !current_chunk.is_empty() {
                chunks.push(current_chunk.trim().to_string());
                current_chunk.clear();
                current_length = 0;
            }

            current_chunk.push_str(sentence);
            current_chunk.push(' ');
            current_length += sentence_length + 1;
        }

        if !current_chunk.trim().is_empty() {
            chunks.push(current_chunk.trim().to_string());
        }

        chunks
    }
}

struct TTSEngine {
    config: Config,
    cache_dir: PathBuf,
}

impl TTSEngine {
    fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let cache_dir = PathBuf::from("./tts_cache");
        if config.cache_enabled {
            fs::create_dir_all(&cache_dir)?;
        }

        Ok(Self { config, cache_dir })
    }

    fn text_to_speech(
        &self,
        text: &str,
        output_path: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Generate cache key
        let cache_key = if self.config.cache_enabled {
            let mut hasher = Sha256::new();
            hasher.update(text.as_bytes());
            hasher.update(&self.config.voice_speed.to_be_bytes());
            hasher.update(&self.config.voice_pitch.to_be_bytes());
            hasher.update(&self.config.sample_rate.to_be_bytes());
            Some(format!("{:x}", hasher.finalize()))
        } else {
            None
        };

        // Check cache
        if let Some(ref key) = cache_key {
            let cache_path = self.cache_dir.join(format!("{}.wav", key));
            if cache_path.exists() {
                return self.convert_audio(&cache_path, output_path);
            }
        }

        // Generate speech using espeak-ng (highly optimized CPU-based TTS)
        let temp_wav = if let Some(ref key) = cache_key {
            self.cache_dir.join(format!("{}.wav", key))
        } else {
            tempfile::NamedTempFile::new()?.into_temp_path().to_path_buf()
        };

        // Check for available TTS engines on Arch Linux
        let tts_command = self.detect_tts_engine()?;

        let espeak_output = match tts_command.as_str() {
            "espeak-ng" => self.run_espeak_ng(text)?,
            "espeak" => self.run_espeak(text)?,
            "festival" => self.run_festival(text)?,
            _ => return Err("No suitable TTS engine found".into()),
        };

        if !espeak_output.status.success() {
            return Err(format!("TTS generation failed with {}", tts_command).into());
        }

        // Write raw audio to temp file
        fs::write(&temp_wav, &espeak_output.stdout)?;

        // Convert to target format
        self.convert_audio(&temp_wav, output_path)?;

        // Clean up temp file if not cached
        if cache_key.is_none() {
            let _ = fs::remove_file(&temp_wav);
        }

        Ok(())
    }

    fn detect_tts_engine(&self) -> Result<String, Box<dyn std::error::Error>> {
        let engines = ["espeak-ng", "espeak", "festival"];

        for engine in &engines {
            if ProcessCommand::new("which")
                .arg(engine)
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false)
                {
                    return Ok(engine.to_string());
                }
        }

        Err("No TTS engine found. Please install espeak-ng, espeak, or festival".into())
    }

    fn run_espeak_ng(&self, text: &str) -> Result<std::process::Output, Box<dyn std::error::Error>> {
        let mut cmd = ProcessCommand::new("espeak-ng");
        cmd.arg("-v")
        .arg("en")
        .arg("-s")
        .arg(format!("{}", (self.config.voice_speed * 175.0) as u32))
        .arg("-p")
        .arg(format!("{}", (self.config.voice_pitch * 50.0) as u32))
        .arg("-a")
        .arg("100")
        .arg("--stdout")
        .arg(text)
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

        Ok(cmd.output()?)
    }

    fn run_espeak(&self, text: &str) -> Result<std::process::Output, Box<dyn std::error::Error>> {
        let mut cmd = ProcessCommand::new("espeak");
        cmd.arg("-v")
        .arg("en")
        .arg("-s")
        .arg(format!("{}", (self.config.voice_speed * 175.0) as u32))
        .arg("-p")
        .arg(format!("{}", (self.config.voice_pitch * 50.0) as u32))
        .arg("-a")
        .arg("100")
        .arg("--stdout")
        .arg(text)
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

        Ok(cmd.output()?)
    }

    fn run_festival(&self, text: &str) -> Result<std::process::Output, Box<dyn std::error::Error>> {
        let mut cmd = ProcessCommand::new("festival");
        cmd.arg("--tts")
        .arg("--pipe")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

        let mut child = cmd.spawn()?;

        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(text.as_bytes())?;
        }

        Ok(child.wait_with_output()?)
    }

    fn convert_audio(
        &self,
        input_path: &Path,
        output_path: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match self.config.output_format {
            AudioFormat::Vorbis => self.convert_to_vorbis(input_path, output_path),
            AudioFormat::Flac => self.convert_to_flac(input_path, output_path),
            AudioFormat::Mp3 => self.convert_to_mp3(input_path, output_path),
            AudioFormat::Wav => {
                fs::copy(input_path, output_path)?;
                Ok(())
            }
        }
    }

    fn convert_to_vorbis(
        &self,
        input_path: &Path,
        output_path: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Try oggenc first (preferred), then ffmpeg as fallback
        let encoders = ["oggenc", "ffmpeg"];

        for encoder in &encoders {
            if ProcessCommand::new("which")
                .arg(encoder)
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false)
                {
                    return match *encoder {
                        "oggenc" => self.encode_with_oggenc(input_path, output_path),
                        "ffmpeg" => self.encode_vorbis_with_ffmpeg(input_path, output_path),
                        _ => continue,
                    };
                }
        }

        Err("No Vorbis encoder found. Please install vorbis-tools or ffmpeg".into())
    }

    fn encode_with_oggenc(
        &self,
        input_path: &Path,
        output_path: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = ProcessCommand::new("oggenc");
        cmd.arg("-q")
        .arg(format!("{}", (self.config.quality * 10.0) as u32))
        .arg("-o")
        .arg(output_path)
        .arg(input_path);

        let output = cmd.output()?;
        if !output.status.success() {
            return Err("oggenc encoding failed".into());
        }
        Ok(())
    }

    fn encode_vorbis_with_ffmpeg(
        &self,
        input_path: &Path,
        output_path: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = ProcessCommand::new("ffmpeg");
        cmd.arg("-i")
        .arg(input_path)
        .arg("-c:a")
        .arg("libvorbis")
        .arg("-q:a")
        .arg(format!("{}", (self.config.quality * 10.0) as u32))
        .arg("-y")
        .arg(output_path);

        let output = cmd.output()?;
        if !output.status.success() {
            return Err("ffmpeg Vorbis encoding failed".into());
        }
        Ok(())
    }

    fn convert_to_flac(
        &self,
        input_path: &Path,
        output_path: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let encoders = ["flac", "ffmpeg"];

        for encoder in &encoders {
            if ProcessCommand::new("which")
                .arg(encoder)
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false)
                {
                    return match *encoder {
                        "flac" => self.encode_with_flac(input_path, output_path),
                        "ffmpeg" => self.encode_flac_with_ffmpeg(input_path, output_path),
                        _ => continue,
                    };
                }
        }

        Err("No FLAC encoder found. Please install flac or ffmpeg".into())
    }

    fn encode_with_flac(
        &self,
        input_path: &Path,
        output_path: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = ProcessCommand::new("flac");
        cmd.arg("--compression-level-8")
        .arg("-o")
        .arg(output_path)
        .arg(input_path);

        let output = cmd.output()?;
        if !output.status.success() {
            return Err("FLAC encoding failed".into());
        }
        Ok(())
    }

    fn encode_flac_with_ffmpeg(
        &self,
        input_path: &Path,
        output_path: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = ProcessCommand::new("ffmpeg");
        cmd.arg("-i")
        .arg(input_path)
        .arg("-c:a")
        .arg("flac")
        .arg("-compression_level")
        .arg("8")
        .arg("-y")
        .arg(output_path);

        let output = cmd.output()?;
        if !output.status.success() {
            return Err("ffmpeg FLAC encoding failed".into());
        }
        Ok(())
    }

    fn convert_to_mp3(
        &self,
        input_path: &Path,
        output_path: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let encoders = ["lame", "ffmpeg"];

        for encoder in &encoders {
            if ProcessCommand::new("which")
                .arg(encoder)
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false)
                {
                    return match *encoder {
                        "lame" => self.encode_with_lame(input_path, output_path),
                        "ffmpeg" => self.encode_mp3_with_ffmpeg(input_path, output_path),
                        _ => continue,
                    };
                }
        }

        Err("No MP3 encoder found. Please install lame or ffmpeg".into())
    }

    fn encode_with_lame(
        &self,
        input_path: &Path,
        output_path: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = ProcessCommand::new("lame");
        cmd.arg("-V")
        .arg(format!("{}", (9.0 - self.config.quality * 9.0) as u32))
        .arg(input_path)
        .arg(output_path);

        let output = cmd.output()?;
        if !output.status.success() {
            return Err("LAME encoding failed".into());
        }
        Ok(())
    }

    fn encode_mp3_with_ffmpeg(
        &self,
        input_path: &Path,
        output_path: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = ProcessCommand::new("ffmpeg");
        cmd.arg("-i")
        .arg(input_path)
        .arg("-c:a")
        .arg("libmp3lame")
        .arg("-q:a")
        .arg(format!("{}", (9.0 - self.config.quality * 9.0) as u32))
        .arg("-y")
        .arg(output_path);

        let output = cmd.output()?;
        if !output.status.success() {
            return Err("ffmpeg MP3 encoding failed".into());
        }
        Ok(())
    }
}

struct EpubProcessor {
    text_processor: TextProcessor,
    tts_engine: TTSEngine,
    config: Config,
}

impl EpubProcessor {
    fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let tts_engine = TTSEngine::new(config.clone())?;
        Ok(Self {
            text_processor: TextProcessor::new(),
           tts_engine,
           config,
        })
    }

    fn extract_chapters(&self, epub_path: &Path) -> Result<Vec<Chapter>, Box<dyn std::error::Error>> {
        let mut doc = EpubDoc::new(epub_path)?;
        let mut chapters = Vec::new();

        // Get spine (reading order)
        let spine = doc.spine.clone();

        for (order, spine_item) in spine.iter().enumerate() {
            if let Some(content) = doc.get_resource_by_path(&spine_item.0) {
                let html_content = String::from_utf8_lossy(&content.0);

                // Extract title from HTML
                let title = self.extract_title(&html_content, order);

                // Convert HTML to plain text
                let plain_text = from_read(html_content.as_bytes(), 80);

                // Clean the text
                let cleaned_text = self.text_processor.clean_text(
                    &plain_text,
                    self.config.preprocessing_aggressive,
                );

                if !cleaned_text.trim().is_empty() {
                    let word_count = cleaned_text.split_whitespace().count();
                    chapters.push(Chapter {
                        title,
                        content: cleaned_text,
                        order,
                        word_count,
                    });
                }
            }
        }

        Ok(chapters)
    }

    fn extract_title(&self, html: &str, order: usize) -> String {
        // Try to extract title from h1, h2, h3 tags
        let title_regex = Regex::new(r"<h[1-3][^>]*>([^<]+)</h[1-3]>").unwrap();

        if let Some(captures) = title_regex.captures(html) {
            let title = captures.get(1).unwrap().as_str();
            return html2text::from_read(title.as_bytes(), 80).trim().to_string();
        }

        format!("Chapter {}", order + 1)
    }

    fn process_chapters(
        &self,
        chapters: Vec<Chapter>,
        output_dir: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        fs::create_dir_all(output_dir)?;

        let progress_bar = ProgressBar::new(chapters.len() as u64);
        progress_bar.set_style(
            ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos:>7}/{len:7} {msg}")?
            .progress_chars("█▉▊▋▌▍▎▏  ")
        );

        chapters
        .into_par_iter()
        .progress_with(progress_bar)
        .try_for_each(|chapter| -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            self.process_single_chapter(&chapter, output_dir)?;
            Ok(())
        })?;

        Ok(())
    }

    fn process_single_chapter(
        &self,
        chapter: &Chapter,
        output_dir: &Path,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let safe_title = sanitize_filename(&chapter.title);
        let chapter_dir = output_dir.join(format!("{:03}_{}", chapter.order, safe_title));
        fs::create_dir_all(&chapter_dir)?;

        // Split chapter into chunks for better TTS processing
        let chunks = self.text_processor.split_into_chunks(
            &chapter.content,
            self.config.chunk_size,
        );

        // Process chunks in sequence to maintain order
        for (chunk_idx, chunk) in chunks.iter().enumerate() {
            if chunk.trim().is_empty() {
                continue;
            }

            let output_filename = format!(
                "{:03}_{}.{}",
                chunk_idx,
                safe_title,
                self.get_file_extension()
            );
            let output_path = chapter_dir.join(output_filename);

            self.tts_engine.text_to_speech(chunk, &output_path)
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
                format!("TTS failed for chunk {}: {}", chunk_idx, e).into()
            })?;
        }

        // Create metadata file
        let metadata = serde_json::json!({
            "title": chapter.title,
            "order": chapter.order,
            "word_count": chapter.word_count,
            "chunks": chunks.len(),
                                         "config": self.config
        });

        let metadata_path = chapter_dir.join("metadata.json");
        let metadata_file = File::create(metadata_path)?;
        serde_json::to_writer_pretty(metadata_file, &metadata)?;

        Ok(())
    }

    fn get_file_extension(&self) -> &'static str {
        match self.config.output_format {
            AudioFormat::Vorbis => "ogg",
            AudioFormat::Flac => "flac",
            AudioFormat::Mp3 => "mp3",
            AudioFormat::Wav => "wav",
        }
    }
}

fn sanitize_filename(name: &str) -> String {
    let invalid_chars = Regex::new(r#"[<>:"/\\|?*]"#).unwrap();
    invalid_chars.replace_all(name, "_").to_string()
}

fn create_playlist(output_dir: &Path, format: &AudioFormat) -> Result<(), Box<dyn std::error::Error>> {
    let mut audio_files = Vec::new();

    // Collect all audio files in order
    for entry in fs::read_dir(output_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            for audio_entry in fs::read_dir(&path)? {
                let audio_entry = audio_entry?;
                let audio_path = audio_entry.path();

                if let Some(ext) = audio_path.extension() {
                    if ext == "ogg" || ext == "flac" || ext == "mp3" || ext == "wav" {
                        audio_files.push(audio_path);
                    }
                }
            }
        }
    }

    audio_files.sort();

    // Create M3U playlist
    let playlist_path = output_dir.join("audiobook.m3u");
    let mut playlist_file = BufWriter::new(File::create(playlist_path)?);

    writeln!(playlist_file, "#EXTM3U")?;
    for audio_file in audio_files {
        if let Some(filename) = audio_file.file_name() {
            writeln!(playlist_file, "{}", filename.to_string_lossy())?;
        }
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = Command::new("EPUB to Audiobook Converter")
    .version("1.0")
    .author("Advanced TTS Converter")
    .about("Converts EPUB files to high-quality audiobooks using CPU-optimized TTS")
    .arg(
        Arg::new("input")
        .short('i')
        .long("input")
        .value_name("FILE")
        .help("Input EPUB file")
        .required(true),
    )
    .arg(
        Arg::new("output")
        .short('o')
        .long("output")
        .value_name("DIR")
        .help("Output directory")
        .required(true),
    )
    .arg(
        Arg::new("format")
        .short('f')
        .long("format")
        .value_name("FORMAT")
        .help("Output audio format")
        .value_parser(["vorbis", "flac", "mp3", "wav"])
        .default_value("vorbis"),
    )
    .arg(
        Arg::new("quality")
        .short('q')
        .long("quality")
        .value_name("FLOAT")
        .help("Audio quality (0.0-1.0)")
        .value_parser(clap::value_parser!(f32))
        .default_value("0.7"),
    )
    .arg(
        Arg::new("speed")
        .short('s')
        .long("speed")
        .value_name("FLOAT")
        .help("Voice speed multiplier")
        .value_parser(clap::value_parser!(f32))
        .default_value("1.0"),
    )
    .arg(
        Arg::new("workers")
        .short('w')
        .long("workers")
        .value_name("NUM")
        .help("Number of worker threads")
        .value_parser(clap::value_parser!(usize))
        .default_value(&num_cpus::get().to_string()),
    )
    .get_matches();

    let input_path = Path::new(matches.get_one::<String>("input").unwrap());
    let output_dir = Path::new(matches.get_one::<String>("output").unwrap());

    let audio_format = match matches.get_one::<String>("format").unwrap().as_str() {
        "vorbis" => AudioFormat::Vorbis,
        "flac" => AudioFormat::Flac,
        "mp3" => AudioFormat::Mp3,
        "wav" => AudioFormat::Wav,
        _ => AudioFormat::Vorbis,
    };

    let config = Config {
        output_format: audio_format,
        quality: *matches.get_one::<f32>("quality").unwrap(),
        voice_speed: *matches.get_one::<f32>("speed").unwrap(),
        max_workers: *matches.get_one::<usize>("workers").unwrap(),
        ..Default::default()
    };

    // Configure Rayon thread pool
    rayon::ThreadPoolBuilder::new()
    .num_threads(config.max_workers)
    .build_global()?;

    println!("🔄 Initializing EPUB to Audiobook Converter...");
    let start_time = Instant::now();

    let processor = EpubProcessor::new(config.clone())?;

    println!("📖 Extracting chapters from EPUB...");
    let chapters = processor.extract_chapters(input_path)?;
    println!("✅ Found {} chapters", chapters.len());

    let total_words: usize = chapters.iter().map(|c| c.word_count).sum();
    println!("📊 Total words: {}", total_words);

    println!("🎤 Converting chapters to audio...");
    processor.process_chapters(chapters, output_dir)?;

    println!("📝 Creating playlist...");
    create_playlist(output_dir, &config.output_format)?;

    let duration = start_time.elapsed();
    println!("✅ Conversion completed in {:.2?}", duration);
    println!("📁 Output saved to: {}", output_dir.display());

    Ok(())
}
