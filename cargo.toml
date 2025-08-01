[package]
name = "epub-audiobook-converter"
version = "1.0.0"
edition = "2021"
authors = ["EPUB Converter Team"]
description = "High-performance EPUB to audiobook converter with CPU-optimized TTS"
license = "MIT"
repository = "https://github.com/your-repo/epub-audiobook-converter"
keywords = ["epub", "audiobook", "tts", "text-to-speech", "converter"]
categories = ["multimedia::audio", "command-line-utilities"]

[[bin]]
name = "epub_audiobook_converter"
path = "src/main.rs"

[[bin]]
name = "epub_converter_gui"
path = "src/gui.rs"

[dependencies]
# Core dependencies
clap = { version = "4.4", features = ["derive"] }
epub = "2.0"
html2text = "0.6"
regex = "1.10"
rayon = "1.8"
tokio = { version = "1.35", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tempfile = "3.8"
sha2 = "0.10"
lru = "0.12"
indicatif = { version = "0.17", features = ["rayon"] }
num_cpus = "1.16"

# Audio processing
hound = "3.5"

# GUI dependencies
eframe = { version = "0.24", optional = true }
egui = { version = "0.24", optional = true }
egui_extras = { version = "0.24", features = ["file"], optional = true }
rfd = { version = "0.12", optional = true }
env_logger = { version = "0.10", optional = true }

[features]
default = ["gui"]
gui = ["dep:eframe", "dep:egui", "dep:egui_extras", "dep:rfd", "dep:env_logger"]

[profile.release]
# Optimize for performance
opt-level = 3
lto = true
codegen-units = 1
panic = "abort"
strip = true

[profile.dev]
# Faster compilation for development
opt-level = 0
debug = true

# Arch Linux specific optimizations
[target.'cfg(target_os = "linux")']
[target.'cfg(target_os = "linux")'.dependencies]
# Linux-specific audio libraries could go here if needed

# Build script for checking system dependencies
[build-dependencies]
# No build dependencies needed currently
