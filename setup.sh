#!/bin/bash

# EPUB to Audiobook Converter - Arch Linux Setup Script
# This script installs all necessary dependencies and builds the converter

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BUILD_DIR="$SCRIPT_DIR/target/release"

echo "📚 EPUB to Audiobook Converter - Arch Linux Setup"
echo "=================================================="

# Check if running on Arch Linux
if ! command -v pacman &> /dev/null; then
    echo "❌ This script is designed for Arch Linux systems with pacman"
    exit 1
fi

# Function to check if package is installed
is_installed() {
    pacman -Qi "$1" &> /dev/null
}

# Function to install package if not already installed
install_if_missing() {
    if ! is_installed "$1"; then
        echo "📦 Installing $1..."
        sudo pacman -S --noconfirm "$1"
    else
        echo "✅ $1 is already installed"
    fi
}

echo
echo "🔍 Checking and installing system dependencies..."

# Core dependencies
install_if_missing "base-devel"
install_if_missing "rust"
install_if_missing "cargo"
install_if_missing "git"

# TTS engines (install espeak-ng as primary, others as alternatives)
echo
echo "🎤 Installing TTS engines..."
install_if_missing "espeak-ng"

# Optional TTS alternatives
if ! is_installed "espeak-ng"; then
    echo "⚠️  espeak-ng not available, trying alternatives..."
    install_if_missing "espeak"

    if ! is_installed "espeak"; then
        echo "⚠️  espeak not available, trying festival..."
        install_if_missing "festival"
    fi
fi

# Audio encoding tools
echo
echo "🎵 Installing audio encoders..."
install_if_missing "vorbis-tools"  # oggenc for Vorbis
install_if_missing "flac"          # flac encoder
install_if_missing "lame"          # MP3 encoder
install_if_missing "ffmpeg"        # Universal audio/video encoder

# GUI dependencies
echo
echo "🖥️  Installing GUI dependencies..."
install_if_missing "gtk3"
install_if_missing "pkg-config"

# Additional useful packages
echo
echo "📋 Installing additional tools..."
install_if_missing "which"         # For dependency detection
install_if_missing "file"          # File type detection

echo
echo "🔧 Setting up Rust environment..."

# Update Rust if already installed
if command -v rustup &> /dev/null; then
    echo "🔄 Updating Rust..."
    rustup update stable
else
    echo "📥 Installing Rust via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi

# Ensure we have the latest Rust
rustup default stable

echo
echo "⚡ Building the converter..."

# Build the main converter
cd "$SCRIPT_DIR"
echo "Building CLI converter..."
cargo build --release --bin epub_audiobook_converter

# Build the GUI
echo "Building GUI..."
cargo build --release --bin epub_converter_gui

# Check if builds succeeded
if [[ -f "$BUILD_DIR/epub_audiobook_converter" && -f "$BUILD_DIR/epub_converter_gui" ]]; then
    echo "✅ Build completed successfully!"
else
    echo "❌ Build failed!"
    exit 1
fi

echo
echo "🧪 Running dependency check..."

# Function to check if command exists
check_command() {
    if command -v "$1" &> /dev/null; then
        echo "✅ $1"
        return 0
    else
        echo "❌ $1 (missing)"
        return 1
    fi
}

# Check TTS engines
echo "🎤 TTS Engines:"
tts_available=false
if check_command "espeak-ng"; then tts_available=true; fi
if check_command "espeak"; then tts_available=true; fi
if check_command "festival"; then tts_available=true; fi

if ! $tts_available; then
    echo "⚠️  No TTS engine found!"
fi

# Check audio encoders
echo
echo "🎵 Audio Encoders:"
check_command "oggenc"
check_command "flac"
check_command "lame"
check_command "ffmpeg"

echo
echo "📁 Creating desktop entries..."

# Create desktop entry for GUI
DESKTOP_FILE="$HOME/.local/share/applications/epub-converter.desktop"
mkdir -p "$(dirname "$DESKTOP_FILE")"

cat > "$DESKTOP_FILE" << EOF
[Desktop Entry]
Version=1.0
Type=Application
Name=EPUB to Audiobook Converter
Comment=Convert EPUB files to audiobooks using TTS
Exec=$BUILD_DIR/epub_converter_gui
Icon=audiobook
Terminal=false
Categories=AudioVideo;Audio;
MimeType=application/epub+zip;
EOF

# Update desktop database
if command -v update-desktop-database &> /dev/null; then
    update-desktop-database "$HOME/.local/share/applications"
fi

echo
echo "🔗 Creating convenient symlinks..."

# Create symlinks in ~/.local/bin (if it exists or create it)
LOCAL_BIN="$HOME/.local/bin"
mkdir -p "$LOCAL_BIN"

ln -sf "$BUILD_DIR/epub_audiobook_converter" "$LOCAL_BIN/epub-to-audiobook"
ln -sf "$BUILD_DIR/epub_converter_gui" "$LOCAL_BIN/epub-to-audiobook-gui"

# Add to PATH if not already there
if [[ ":$PATH:" != *":$LOCAL_BIN:"* ]]; then
    echo "📝 Adding $LOCAL_BIN to PATH..."
    echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$HOME/.bashrc"
    echo "⚠️  Please run 'source ~/.bashrc' or restart your terminal to update PATH"
fi

echo
echo "🧪 Testing installation..."

# Test CLI converter
echo "Testing CLI converter..."
if "$BUILD_DIR/epub_audiobook_converter" --help &> /dev/null; then
    echo "✅ CLI converter working"
else
    echo "❌ CLI converter test failed"
fi

# Test GUI converter (just check if it starts without error)
echo "Testing GUI converter..."
timeout 3s "$BUILD_DIR/epub_converter_gui" &> /dev/null || true
echo "✅ GUI converter appears to be working"

echo
echo "🎉 Installation completed successfully!"
echo
echo "📋 Usage:"
echo "  CLI: epub-to-audiobook -i book.epub -o output_folder"
echo "  GUI: epub-to-audiobook-gui"
echo "  Or find 'EPUB to Audiobook Converter' in your applications menu"
echo
echo "📖 Example usage:"
echo "  epub-to-audiobook -i ~/Documents/book.epub -o ~/Audiobooks/book -f vorbis -q 0.8"
echo
echo "🔧 Advanced options:"
echo "  --help                 Show all available options"
echo "  -f FORMAT             Audio format: vorbis, flac, mp3, wav"
echo "  -q QUALITY            Audio quality: 0.1 to 1.0"
echo "  -s SPEED              Voice speed: 0.5 to 2.0"
echo "  -w WORKERS            Number of worker threads"
echo
echo "🐛 Troubleshooting:"
echo "  - If TTS doesn't work: sudo pacman -S espeak-ng"
echo "  - If audio encoding fails: sudo pacman -S vorbis-tools flac lame ffmpeg"
echo "  - Check dependencies: epub-to-audiobook --check-deps"
echo
echo "Enjoy converting your EPUBs to audiobooks! 📚🎧"
