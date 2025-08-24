.PHONY: all build clean test lint format release help cross-build-armv7 cross-build-arm64 build-macos install-deps check

# Default target
all: check test build

# Install development dependencies
install-deps:
	@echo "Installing Rust development tools..."
	@cargo install cargo-audit
	@echo "Development tools installed successfully"

# Check code quality
check:
	@echo "Running code quality checks..."
	@cargo check
	@cargo fmt --all -- --check
	@cargo clippy -- -D warnings

# Run tests
test:
	@echo "Running tests..."
	@cargo test --verbose

# Run linter
lint:
	@echo "Running clippy linter..."
	@cargo clippy -- -D warnings

# Format code
format:
	@echo "Formatting code..."
	@cargo fmt --all

# Build for development
build:
	@echo "Building for development..."
	@cargo build --verbose

# Build for release
release:
	@echo "Building for release..."
	@cargo build --release --verbose

# Clean build artifacts
clean:
	@echo "Cleaning build artifacts..."
	@cargo clean

# Cross-compile for Cerbo GX (ARM v7)
cross-build-armv7: install-cross-deps
	@echo "Cross-compiling for Cerbo GX (ARM v7)..."
	@cargo build --target armv7-unknown-linux-gnueabihf --release --verbose
	@echo "Binary available at: target/armv7-unknown-linux-gnueabihf/release/phaeton"

# Cross-compile for Linux ARM64
cross-build-arm64: install-cross-deps
	@echo "Cross-compiling for Linux ARM64..."
	@cargo build --target aarch64-unknown-linux-gnu --release --verbose
	@echo "Binary available at: target/aarch64-unknown-linux-gnu/release/phaeton"

# Build for macOS ARM64 (native)
build-macos:
	@echo "Building for macOS ARM64..."
	@cargo build --release --verbose
	@echo "Binary available at: target/release/phaeton"

# Install cross-compilation dependencies (Linux/macOS)
install-cross-deps:
	@echo "Installing cross-compilation tools..."
	@if command -v apt-get >/dev/null 2>&1; then \
		sudo apt-get update && \
		sudo apt-get install -y gcc-arm-linux-gnueabihf gcc-aarch64-linux-gnu; \
	elif command -v brew >/dev/null 2>&1; then \
		brew tap messense/macos-cross-toolchains && \
		brew install aarch64-unknown-linux-gnu && \
		brew install armv7-unknown-linux-gnueabihf; \
	else \
		echo "Please install cross-compilation tools manually for your system"; \
		exit 1; \
	fi
	@echo "Cross-compilation tools installed successfully"

# Create release artifacts for all targets
package-release: cross-build-armv7 cross-build-arm64 build-macos
	@echo "Creating release packages..."
	@mkdir -p dist
	@VERSION=$${PHAETON_VERSION:-$$(grep -m1 '^version\s*=\s*"' Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/')} ; \\
	if git describe --tags --abbrev=0 >/dev/null 2>&1 ; then \\
	  TAG=$$(git describe --tags --abbrev=0 2>/dev/null || true) ; \\
	  case "$$TAG" in \\
	    v$$VERSION*) VERSION=$${TAG#v} ;; \\
	  esac ; \\
	fi ; \\
	cd target/armv7-unknown-linux-gnueabihf/release && \\
		tar -czf phaeton-v$$VERSION-armv7-unknown-linux-gnueabihf.tar.gz phaeton && \\
		mv phaeton-v$$VERSION-armv7-unknown-linux-gnueabihf.tar.gz ../../../dist/ ; \\
	cd - >/dev/null ; \\
	cd target/aarch64-unknown-linux-gnu/release && \\
		tar -czf phaeton-v$$VERSION-aarch64-unknown-linux-gnu.tar.gz phaeton && \\
		mv phaeton-v$$VERSION-aarch64-unknown-linux-gnu.tar.gz ../../../dist/ ; \\
	cd - >/dev/null ; \\
	cd target/release && \\
		tar -czf phaeton-v$$VERSION-macos-arm64.tar.gz phaeton && \\
		mv phaeton-v$$VERSION-macos-arm64.tar.gz ../../../dist/ ; \\
	cd - >/dev/null ; \\
	echo "Release packages created in dist/ directory (version $$VERSION)"

# Security audit
audit:
	@echo "Running security audit..."
	@cargo audit

# Run all quality checks
quality: format lint test audit
	@echo "All quality checks passed!"

# Help target
help:
	@echo "Available targets:"
	@echo "  all                - Run checks, tests, and build (default)"
	@echo "  check              - Run code quality checks"
	@echo "  test               - Run tests"
	@echo "  lint               - Run clippy linter"
	@echo "  format             - Format code with rustfmt"
	@echo "  build              - Build for development"
	@echo "  release            - Build for release"
	@echo "  clean              - Clean build artifacts"
	@echo "  cross-build-armv7  - Cross-compile for Cerbo GX (ARM v7)"
	@echo "  cross-build-arm64  - Cross-compile for Linux ARM64"
	@echo "  build-macos        - Build for macOS ARM64"
	@echo "  install-cross-deps - Install cross-compilation tools"
	@echo "  package-release    - Create release packages for all targets"
	@echo "  audit              - Run security audit"
	@echo "  quality            - Run all quality checks"
	@echo "  install-deps       - Install development dependencies"
	@echo "  help               - Show this help message"
