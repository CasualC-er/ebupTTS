# 📚 EPUB to Audiobook Converter

A high-performance, CPU-optimized EPUB to audiobook converter written in Rust, with full Arch Linux support and a minimal GUI.

## ✨ Features

- **🚀 High Performance**: Parallel processing with CPU optimization
- **🎤 Multiple TTS Engines**: espeak-ng, espeak, festival support
- **🎵 Multiple Audio Formats**: Vorbis, FLAC, MP3, WAV
- **🧹 Advanced Text Cleanup**: OCR error correction, smart preprocessing
- **💾 Intelligent Caching**: Avoid re-generating identical audio segments
- **🖥️ Minimal GUI**: Easy-to-use graphical interface
- **⚡ Arch Linux Optimized**: Full compatibility with Arch Linux packages

## 📋 System Requirements

### Arch Linux
- `base-devel` package group
- `rust` and `cargo`
- Audio codecs and TTS engines (auto-installed by setup script)

## 🚀 Quick Start (Arch Linux)

### 1. Clone and Setup
```bash
git clone <repository-url>
cd epub-audiobook-converter
chmod +x setup_arch.sh
./setup_arch.sh
```

The setup script will:
- Install all necessary system dependencies via pacman
- Build both CLI and GUI versions
- Create desktop entries
- Set up command-line shortcuts

### 2. Usage

#### GUI Version
```bash
epub-to-audiobook-gui
```
Or find "EPUB to Audiobook Converter" in your applications menu.

#### CLI Version
```bash
# Basic usage
epub-to-audiobook -i book.epub -o output_folder

# Advanced usage
epub-to-audiobook \
  -i ~/Documents/mybook.epub \
  -o ~/Audiobooks/mybook \
  -f vorbis \
  -q 0.8 \
  -s 1.2 \
  -w 8
```

## 🔧 Manual Installation (Arch Linux)

If you prefer manual installation:

### 1. Install System Dependencies
```bash
# Core development tools
sudo pacman -S base-devel rust cargo git

# TTS engines (choose one or more)
sudo pacman -S espeak-ng  # Recommended
# sudo pacman -S espeak   # Alternative
# sudo pacman -S festival # Alternative

# Audio encoders
sudo pacman -S vorbis-tools flac lame ffmpeg

# GUI dependencies
sudo pacman -S gtk3 pkg-config

# Utility tools
sudo pacman -S which file
```

### 2. Build the Project
```bash
# Build CLI version
cargo build --release --bin epub_audiobook_converter

# Build GUI version  
cargo build --release --bin epub_converter_gui --features gui
```

### 3. Install Binaries
```bash
# Copy to system location
sudo cp target/release/epub_audiobook_converter /usr/local/bin/
sudo cp target/release/epub_converter_gui /usr/local/bin/

# Or create user-local symlinks
mkdir -p ~/.local/bin
ln -s $(pwd)/target/release/epub_audiobook_converter ~/.local/bin/epub-to-audiobook
ln -s $(pwd)/target/release/epub_converter_gui ~/.local/bin/epub-to-audiobook-gui
```

## 📖 Detailed Usage

### CLI Options
```
USAGE:
    epub_audiobook_converter [OPTIONS] -i <FILE> -o <DIR>

OPTIONS:
    -i, --input <FILE>      Input EPUB file
    -o, --output <DIR>      Output directory
    -f, --format <FORMAT>   Audio format [default: vorbis] [possible values: vorbis, flac, mp3, wav]
    -q, --quality <FLOAT>   Audio quality (0.0-1.0) [default: 0.7]
    -s, --speed <FLOAT>     Voice speed multiplier [default: 1.0]
    -w, --workers <NUM>     Number of worker threads [default: CPU_CORES]
    -h, --help             Print help information
    -V, --version          Print version information
```

### GUI Features
- **File Selection**: Browse for EPUB input and output directory
- **Audio Settings**: Configure format, quality, speed, and pitch
- **Advanced Settings**: Worker threads, caching, text preprocessing
- **Progress Tracking**: Real-time conversion progress with ETA
- **Dependency Check**: Verify system requirements
- **Settings Management**: Save/load configuration profiles

## 🎵 Audio Format Details

### Vorbis (.ogg) - Recommended
- Excellent compression ratio
- High quality at lower bitrates
- Open source, patent-free
- Package: `vorbis-tools`

### FLAC (.flac) - Lossless
- Perfect audio quality
- Larger file sizes
- Ideal for archival purposes
- Package: `flac`

### MP3 (.mp3) - Universal
- Maximum compatibility
- Moderate compression
- Widely supported
- Package: `lame`

### WAV (.wav) - Uncompressed
- Maximum quality
- Largest file sizes
- No additional encoding needed
- Built-in support

## 🎤 TTS Engine Comparison

### espeak-ng (Recommended)
```bash
sudo pacman -S espeak-ng
```
- Modern, actively maintained
- Good voice quality
- Fast processing
- Multi-language support

### espeak (Fallback)
```bash
sudo pacman -S espeak
```
- Older but stable
- Lightweight
- Basic voice quality
- Wide language support

### Festival (Alternative)
```bash
sudo pacman -S festival
```
- Higher quality voices
- Slower processing
- More configuration options
- Academic/research oriented

## ⚡ Performance Optimization

### CPU Usage
- Uses all available CPU cores by default
- Adjust with `-w` flag for specific thread count
- Rayon-based parallel processing

### Memory Usage
- Streaming text processing
- LRU cache for repeated content
- Configurable cache size

### Storage
- Intelligent TTS caching (optional)
- Temporary file cleanup
- Efficient audio encoding

## 🐛 Troubleshooting

### Common Issues

#### "No TTS engine found"
```bash
sudo pacman -S espeak-ng
# or
sudo pacman -S espeak festival
```

#### "Audio encoding failed"
```bash
sudo pacman -S vorbis-tools flac lame ffmpeg
```

#### "Permission denied"
```bash
chmod +x target/release/epub_audiobook_converter
chmod +x target/release/epub_converter_gui
```

#### GUI doesn't start
```bash
sudo pacman -S gtk3 pkg-config
export DISPLAY=:0  # if using SSH
```

### Debug Mode
```bash
RUST_LOG=debug ./epub_audiobook_converter -i book.epub -o output
```

### Check Dependencies
The GUI includes a built-in dependency checker, or run:
```bash
which espeak-ng oggenc flac lame ffmpeg
```

## 🔧 Configuration

### Default Settings
The converter uses sensible defaults:
- Audio format: Vorbis (.ogg)
- Quality: 0.7 (good balance of size/quality)
- Voice speed: 1.0 (normal)
- Workers: Number of CPU cores
- Caching: Enabled

### Custom Configuration
Settings can be saved/loaded through the GUI or by editing the generated JSON files.

## 📚
