#!/bin/bash

# Phaeton Cross-Compilation Script
# Builds binaries for Cerbo GX (ARM v7) and Linux ARM64

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Function to print colored output
print_status() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Function to check if command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Check if Rust is installed
if ! command_exists cargo; then
    print_error "Rust is not installed. Please install Rust first:"
    echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi

# Check if we're on macOS
if [[ "$OSTYPE" != "darwin"* ]]; then
    print_warning "This script is optimized for macOS. Some features may not work on other systems."
fi

# Install cross-compilation targets
print_status "Installing Rust cross-compilation targets..."
rustup target add armv7-unknown-linux-gnueabihf
rustup target add aarch64-unknown-linux-gnu

# Install cross-compilation tools on macOS
if [[ "$OSTYPE" == "darwin"* ]]; then
    print_status "Checking for cross-compilation tools..."

    if ! command_exists aarch64-unknown-linux-gnu-gcc; then
        print_warning "ARM64 cross-compilation tools not found."
        print_status "Installing via Homebrew..."
        if command_exists brew; then
            brew tap messense/macos-cross-toolchains
            brew install aarch64-unknown-linux-gnu
        else
            print_error "Homebrew not found. Please install ARM64 cross-compilation tools manually:"
            echo "  brew tap messense/macos-cross-toolchains"
            echo "  brew install aarch64-unknown-linux-gnu"
            echo ""
            print_warning "Skipping ARM64 cross-compilation for now..."
            SKIP_ARM64=true
        fi
    fi

    if ! command_exists armv7-unknown-linux-gnueabihf-gcc; then
        print_warning "ARM v7 cross-compilation tools not found."
        print_status "Installing via Homebrew..."
        if command_exists brew; then
            brew tap messense/macos-cross-toolchains
            brew install armv7-unknown-linux-gnueabihf
        else
            print_error "Homebrew not found. Please install ARM v7 cross-compilation tools manually:"
            echo "  brew tap messense/macos-cross-toolchains"
            echo "  brew install armv7-unknown-linux-gnueabihf"
            echo ""
            print_warning "Skipping ARM v7 cross-compilation for now..."
            SKIP_ARMV7=true
        fi
    fi
else
    print_status "Installing cross-compilation tools for Linux..."
    if sudo apt-get update && sudo apt-get install -y gcc-arm-linux-gnueabihf gcc-aarch64-linux-gnu; then
        print_success "Cross-compilation tools installed successfully!"
    else
        print_error "Failed to install cross-compilation tools."
        echo "Please install manually:"
        echo "  sudo apt-get update"
        echo "  sudo apt-get install -y gcc-arm-linux-gnueabihf gcc-aarch64-linux-gnu"
        exit 1
    fi
fi

print_success "Cross-compilation tools installed successfully!"

# Create dist directory
mkdir -p dist

# Function to build and package for a target
build_target() {
    local target=$1
    local name=$2

    print_status "Building for $name ($target)..."

    if cargo build --target $target --release --verbose; then
        print_success "Successfully built for $name"

        # Create tarball
        local binary_path="target/$target/release/phaeton"
        local archive_name="phaeton-$target.tar.gz"

        if [[ -f "$binary_path" ]]; then
            cd "target/$target/release"
            tar -czf "../../../dist/$archive_name" phaeton
            cd ../../../
            print_success "Created archive: dist/$archive_name"
        else
            print_error "Binary not found at $binary_path"
        fi
    else
        print_error "Failed to build for $name"
        return 1
    fi
}

# Build for all targets
print_status "Starting builds..."

# Build for Cerbo GX (ARM v7)
if [[ "$SKIP_ARMV7" != "true" ]]; then
    build_target "armv7-unknown-linux-gnueabihf" "Cerbo GX"
else
    print_warning "Skipping ARM v7 build (cross-compilation tools not available)"
fi

# Build for Linux ARM64
if [[ "$SKIP_ARM64" != "true" ]]; then
    build_target "aarch64-unknown-linux-gnu" "Linux ARM64"
else
    print_warning "Skipping ARM64 build (cross-compilation tools not available)"
fi

# Build for macOS (native) - always try this
print_status "Building for macOS ARM64 (native)..."
if cargo build --release --verbose; then
    print_success "Successfully built for macOS ARM64"

    cd target/release
    tar -czf "../../dist/phaeton-macos-arm64.tar.gz" phaeton
    cd ../..
    print_success "Created archive: dist/phaeton-macos-arm64.tar.gz"
else
    print_error "Failed to build for macOS ARM64"
fi

print_success "Build process completed!"

# Show available binaries
print_status "Checking for available binaries in the dist/ directory:"
if [[ -d "dist" ]]; then
    ls -la dist/

    print_status "Summary of available binaries:"
    if [[ -f "dist/phaeton-armv7-unknown-linux-gnueabihf.tar.gz" ]]; then
        echo "  ✅ Cerbo GX (ARM v7): dist/phaeton-armv7-unknown-linux-gnueabihf.tar.gz"
    else
        echo "  ❌ Cerbo GX (ARM v7): Not built (cross-compilation tools not available)"
    fi

    if [[ -f "dist/phaeton-aarch64-unknown-linux-gnu.tar.gz" ]]; then
        echo "  ✅ Linux ARM64: dist/phaeton-aarch64-unknown-linux-gnu.tar.gz"
    else
        echo "  ❌ Linux ARM64: Not built (cross-compilation tools not available)"
    fi

    if [[ -f "dist/phaeton-macos-arm64.tar.gz" ]]; then
        echo "  ✅ macOS ARM64: dist/phaeton-macos-arm64.tar.gz"
    else
        echo "  ❌ macOS ARM64: Build failed"
    fi
else
    print_error "No dist directory found. Build may have failed."
fi

# Quick build for local use (macOS ARM64 only)
print_status "Building for local macOS use..."
if cargo build --release --verbose; then
    print_success "Local macOS binary ready at: target/release/phaeton"
    echo "  You can run it with: ./target/release/phaeton"
else
    print_error "Failed to build local binary"
fi

# Provide installation instructions
if [[ -d "dist" ]]; then
    print_status "Installation instructions:"
    echo ""
    echo "For Cerbo GX (Venus OS):"
    echo "  1. Copy phaeton-armv7-unknown-linux-gnueabihf.tar.gz to your Cerbo GX"
    echo "  2. Extract: tar -xzf phaeton-armv7-unknown-linux-gnueabihf.tar.gz"
    echo "  3. Move binary: sudo mv phaeton /usr/local/bin/"
    echo "  4. Make executable: sudo chmod +x /usr/local/bin/phaeton"
    echo ""
    echo "For Linux ARM64 systems:"
    echo "  1. Extract: tar -xzf phaeton-aarch64-unknown-linux-gnu.tar.gz"
    echo "  2. Move binary: sudo mv phaeton /usr/local/bin/"
    echo "  3. Make executable: sudo chmod +x /usr/local/bin/phaeton"
    echo ""
    echo "For macOS (local):"
    echo "  1. Extract: tar -xzf phaeton-macos-arm64.tar.gz"
    echo "  2. The binary can be run directly: ./phaeton"
fi
