#!/bin/bash

# Phaeton Cross-Compilation Script
# Builds binaries for Cerbo GX (ARM v7), Linux ARM64, and Linux AMD64 in parallel

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
rustup target add x86_64-unknown-linux-gnu

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

    if ! command_exists x86_64-unknown-linux-gnu-gcc; then
        print_warning "AMD64 (Linux) cross-compilation tools not found."
        print_status "Installing via Homebrew..."
        if command_exists brew; then
            brew tap messense/macos-cross-toolchains
            brew install x86_64-unknown-linux-gnu
        else
            print_error "Homebrew not found. Please install AMD64 cross-compilation tools manually:"
            echo "  brew tap messense/macos-cross-toolchains"
            echo "  brew install x86_64-unknown-linux-gnu"
            echo ""
            print_warning "Skipping AMD64 cross-compilation for now..."
            SKIP_AMD64=true
        fi
    fi
else
    print_status "Installing cross-compilation tools for Linux..."
    if sudo apt-get update && sudo apt-get install -y gcc-arm-linux-gnueabihf gcc-aarch64-linux-gnu gcc-x86-64-linux-gnu; then
        print_success "Cross-compilation tools installed successfully!"
    else
        print_error "Failed to install cross-compilation tools."
        echo "Please install manually:"
        echo "  sudo apt-get update"
        echo "  sudo apt-get install -y gcc-arm-linux-gnueabihf gcc-aarch64-linux-gnu gcc-x86-64-linux-gnu"
        exit 1
    fi
fi

print_success "Cross-compilation tools installed successfully!"

# Create dist directory and derive version
mkdir -p dist

# Determine version from Cargo.toml or git tag
VERSION=${PHAETON_VERSION:-$(grep -m1 '^version\s*=\s*"' Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/')}
if git describe --tags --abbrev=0 >/dev/null 2>&1; then
    GIT_TAG=$(git describe --tags --abbrev=0 2>/dev/null || true)
    # Prefer tag that matches Cargo version (vX.Y.Z)
    if [[ $GIT_TAG == v$VERSION* ]]; then
        VERSION=${GIT_TAG#v}
    fi
fi
print_status "Using version: $VERSION"

# Function to build and package for a target
build_target() {
    local target=$1
    local name=$2

    print_status "Building for $name ($target)..."

    if cargo build --target $target --release --verbose; then
        print_success "Successfully built for $name"

        # Create tarball
        local binary_path="target/$target/release/phaeton"
        local archive_name="phaeton-v${VERSION}-$target.tar.gz"

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

print_status "Starting parallel builds..."

PIDS=()

# Build for Cerbo GX (ARM v7)
if [[ "$SKIP_ARMV7" != "true" ]]; then
    build_target "armv7-unknown-linux-gnueabihf" "Cerbo GX" &
    PIDS+=("$!")
else
    print_warning "Skipping ARM v7 build (cross-compilation tools not available)"
fi

# Build for Linux ARM64
if [[ "$SKIP_ARM64" != "true" ]]; then
    build_target "aarch64-unknown-linux-gnu" "Linux ARM64" &
    PIDS+=("$!")
else
    print_warning "Skipping ARM64 build (cross-compilation tools not available)"
fi

# Build for Linux AMD64
if [[ "$SKIP_AMD64" != "true" ]]; then
    build_target "x86_64-unknown-linux-gnu" "Linux AMD64" &
    PIDS+=("$!")
else
    print_warning "Skipping AMD64 build (cross-compilation tools not available)"
fi

# Wait for all background builds to complete
if [[ ${#PIDS[@]} -gt 0 ]]; then
    print_status "Waiting for ${#PIDS[@]} parallel build(s) to finish..."
    wait
fi

print_success "Build process completed!"

# Show available binaries
print_status "Checking for available binaries in the dist/ directory:"
if [[ -d "dist" ]]; then
    ls -la dist/

    print_status "Summary of available binaries:"
    if ls dist/phaeton-v${VERSION}-armv7-unknown-linux-gnueabihf.tar.gz >/dev/null 2>&1; then
        echo "  ✅ Cerbo GX (ARM v7): dist/phaeton-v${VERSION}-armv7-unknown-linux-gnueabihf.tar.gz"
    else
        echo "  ❌ Cerbo GX (ARM v7): Not built (cross-compilation tools not available)"
    fi

    if ls dist/phaeton-v${VERSION}-aarch64-unknown-linux-gnu.tar.gz >/dev/null 2>&1; then
        echo "  ✅ Linux ARM64: dist/phaeton-v${VERSION}-aarch64-unknown-linux-gnu.tar.gz"
    else
        echo "  ❌ Linux ARM64: Not built (cross-compilation tools not available)"
    fi

    if ls dist/phaeton-v${VERSION}-x86_64-unknown-linux-gnu.tar.gz >/dev/null 2>&1; then
        echo "  ✅ Linux AMD64: dist/phaeton-v${VERSION}-x86_64-unknown-linux-gnu.tar.gz"
    else
        echo "  ❌ Linux AMD64: Not built (cross-compilation tools not available)"
    fi
else
    print_error "No dist directory found. Build may have failed."
fi

# Quick build for local host (debugging)
print_status "Building for local host (debug) ..."
if cargo build --verbose; then
    print_success "Local host binary ready at: target/debug/phaeton"
    echo "  You can run it with: ./target/debug/phaeton"
else
    print_error "Failed to build local host binary"
fi

# Provide installation instructions
if [[ -d "dist" ]]; then
    print_status "Installation instructions:"
    echo ""
    echo "For Cerbo GX (Venus OS):"
    echo "  1. Copy phaeton-v${VERSION}-armv7-unknown-linux-gnueabihf.tar.gz to your Cerbo GX"
    echo "  2. Extract: tar -xzf phaeton-v${VERSION}-armv7-unknown-linux-gnueabihf.tar.gz"
    echo "  3. Move binary: sudo mv phaeton /usr/local/bin/"
    echo "  4. Make executable: sudo chmod +x /usr/local/bin/phaeton"
    echo ""
    echo "For Linux ARM64 systems:"
    echo "  1. Extract: tar -xzf phaeton-v${VERSION}-aarch64-unknown-linux-gnu.tar.gz"
    echo "  2. Move binary: sudo mv phaeton /usr/local/bin/"
    echo "  3. Make executable: sudo chmod +x /usr/local/bin/phaeton"
    echo ""
    echo "For Linux AMD64 systems:"
    echo "  1. Extract: tar -xzf phaeton-v${VERSION}-x86_64-unknown-linux-gnu.tar.gz"
    echo "  2. Move binary: sudo mv phaeton /usr/local/bin/"
    echo "  3. Make executable: sudo chmod +x /usr/local/bin/phaeton"
fi
