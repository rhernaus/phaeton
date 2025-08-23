# Phaeton - Alfen EV Charger Driver (Rust)

A high-performance Rust implementation of the Alfen EV charger driver for Victron Venus OS, providing seamless integration with Victron's D-Bus system and advanced features like dynamic pricing, vehicle integration, and self-updates.

## Features

- **High Performance**: Async-first design with Tokio runtime
- **Memory Safe**: Rust's ownership system prevents common bugs
- **Modbus TCP**: Direct communication with Alfen EV chargers
- **D-Bus Integration**: Full Venus OS compatibility
- **Web Interface**: REST API and static file serving
- **Dynamic Pricing**: Tibber API integration for smart charging
- **Vehicle Integration**: Tesla and Kia API support
- **Self-Updates**: Git-based automatic updates
- **Configuration**: YAML-based configuration with validation

## Status

🚧 **Work in Progress**: This is a rewrite of the Python [victron-alfen-charger](https://github.com/your-org/victron-alfen-charger) project in Rust. Currently in Phase 2 (Core communication & control) – polling, MANUAL/AUTO/SCHEDULED control logic, session tracking + persistence, and D‑Bus via `zbus` (service registered; cached paths for now) are implemented. A basic web API is available (status + control + config get/update), and the driver spawns the web server. CI builds and cross-compiles are configured. Modbus stack upgraded to `tokio-modbus` 0.16.1.

## Quick Start

### Download prebuilt binaries (recommended)

Grab the latest binaries from [Releases](https://github.com/your-org/phaeton/releases):

- **Cerbo GX (ARMv7)**: `phaeton-armv7-unknown-linux-gnueabihf.tar.gz`
- **Linux ARM64**: `phaeton-aarch64-unknown-linux-gnu.tar.gz`
- **macOS ARM64**: `phaeton-macos-arm64.tar.gz`

Verify checksums (Linux):

```bash
curl -L -O https://github.com/your-org/phaeton/releases/download/<tag>/SHA256SUMS
curl -L -O https://github.com/your-org/phaeton/releases/download/<tag>/phaeton-<artifact>.tar.gz
sha256sum -c SHA256SUMS
```

Verify on macOS:

```bash
curl -L -O https://github.com/your-org/phaeton/releases/download/<tag>/SHA256SUMS
curl -L -O https://github.com/your-org/phaeton/releases/download/<tag>/phaeton-macos-arm64.tar.gz
shasum -a 256 phaeton-macos-arm64.tar.gz
grep phaeton-macos-arm64.tar.gz SHA256SUMS
```

Install:

```bash
tar -xzf phaeton-<artifact>.tar.gz
sudo install -m 0755 phaeton /usr/local/bin/phaeton
```

Run:

```bash
phaeton
```

Nightly builds are published to the rolling `nightly` prerelease for early testing.

### Prerequisites

- Rust (stable) with 2024 edition
- Venus OS or Linux system with D-Bus support
- Alfen EV charger accessible via network

### Installation

```bash
# Clone the repository
git clone https://github.com/your-org/phaeton.git
cd phaeton

# Build the project
cargo build --release

# Run the driver (spawns web server at 127.0.0.1:8088)
cargo run
```

### Configuration

Copy the sample configuration and edit as needed:

```bash
cp alfen_driver_config.sample.yaml alfen_driver_config.yaml
# Edit the configuration file with your settings
```

### Development

```bash
# Run tests
cargo test

# Run linter
cargo clippy

# Check formatting
cargo fmt --check

# Run security audit
cargo audit

# Make build script executable
chmod +x build.sh
```

### Cross-Compilation

Phaeton supports cross-compilation for multiple architectures to run on different systems:

#### Quick Build Script (Recommended)

```bash
# Build for all supported platforms
./build.sh
```

This will create release binaries in the `dist/` directory for:
- **Cerbo GX (ARM v7)**: `phaeton-armv7-unknown-linux-gnueabihf.tar.gz`
- **Linux ARM64**: `phaeton-aarch64-unknown-linux-gnu.tar.gz`
- **macOS ARM64**: `phaeton-macos-arm64.tar.gz`

#### GitHub Actions CI

Tagging a version (e.g., `v0.1.0`) triggers a release build that uploads signed artifacts and checksums to [Releases](https://github.com/your-org/phaeton/releases). Pushes to `main` update the rolling `nightly` prerelease.

#### Manual Cross-Compilation

```bash
# Install cross-compilation targets
rustup target add armv7-unknown-linux-gnueabihf
rustup target add aarch64-unknown-linux-gnu

# Install cross-compilation tools (macOS)
brew tap messense/macos-cross-toolchains
brew install aarch64-unknown-linux-gnu armv7-unknown-linux-gnueabihf

# For Linux (Ubuntu 24.04+), install cross toolchains and headers:
sudo apt-get update
sudo apt-get install -y --no-install-recommends \
  gcc-arm-linux-gnueabihf gcc-aarch64-linux-gnu \
  libc6-dev-armhf-cross libc6-dev-arm64-cross \
  pkg-config cmake make perl build-essential ca-certificates

# Build for Cerbo GX (ARMv7)
export CC_armv7_unknown_linux_gnueabihf=arm-linux-gnueabihf-gcc
export AR_armv7_unknown_linux_gnueabihf=arm-linux-gnueabihf-ar
export RANLIB_armv7_unknown_linux_gnueabihf=arm-linux-gnueabihf-ranlib
export CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER=arm-linux-gnueabihf-gcc
export PKG_CONFIG_ALLOW_CROSS=1
cargo build --target armv7-unknown-linux-gnueabihf --release

# Build for Linux ARM64
export CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc
export AR_aarch64_unknown_linux_gnu=aarch64-linux-gnu-ar
export RANLIB_aarch64_unknown_linux_gnu=aarch64-linux-gnu-ranlib
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
export PKG_CONFIG_ALLOW_CROSS=1
cargo build --target aarch64-unknown-linux-gnu --release

# Build for macOS (native)
cargo build --release
```

#### Using Make

```bash
# Show all available targets
make help

# Cross-compile for specific targets
make cross-build-armv7    # Cerbo GX (ARM v7)
make cross-build-arm64    # Linux ARM64
make build-macos          # macOS ARM64

# Build all targets and create packages
make package-release

# Run all quality checks
make quality
```

## Architecture

The application follows a modular architecture with clear separation of concerns:

- `config`: Configuration management and validation
- `logging`: Structured logging and tracing
- `modbus`: Modbus TCP client for charger communication
- `driver`: Core driver logic and state management
- `dbus`: D-Bus integration for Venus OS
- `web`: HTTP server and REST API
- `persistence`: State persistence and recovery
- `session`: Charging session management
- `controls`: Charging control algorithms
- `tibber`: Dynamic pricing integration
- `vehicle`: Vehicle API integrations
- `updater`: Self-update functionality

## API Documentation

### REST API Endpoints (available)

- `GET /api/status` - Current system status
- `POST /api/mode` - Change charging mode
- `POST /api/startstop` - Start/stop charging
- `POST /api/set_current` - Set charging current
 - `GET /api/config` - Get configuration
 - `PUT /api/config` - Update configuration

### Planned Endpoints

- `GET /api/config` - Get configuration
- `PUT /api/config` - Update configuration
- `GET /api/config/schema` - Configuration schema
- Update management endpoints (`/api/update/*`)
- Logs endpoints (`/api/logs/*`)

### WebSocket/SSE Support (planned)

Real-time updates via WebSocket or Server-Sent Events for live monitoring.

## Configuration

The application uses YAML configuration with the following main sections:

```yaml
modbus:
  ip: "192.168.1.100"
  port: 502
  socket_slave_id: 1
  station_slave_id: 200

device_instance: 0

logging:
  level: INFO
  file: "/var/log/phaeton.log"
  format: structured

tibber:
  enabled: false
  access_token: ""
  strategy: level

web:
  host: "127.0.0.1"
  port: 8088
```

## Deployment

### Venus OS

1. Build for ARM target:
   ```bash
   cargo build --target armv7-unknown-linux-gnueabihf --release
   ```

2. Copy binary to Venus OS:
   ```bash
   scp target/armv7-unknown-linux-gnueabihf/release/phaeton root@venus:/data/phaeton
   ```

3. Set up as service (similar to Python version)

### Docker

```dockerfile
FROM rust:1.83-slim as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bullseye-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/phaeton /usr/local/bin/phaeton
CMD ["phaeton"]
```

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests for new functionality
5. Run `cargo test` and `cargo clippy`
6. Submit a pull request

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Related Projects

- [victron-alfen-charger](https://github.com/your-org/victron-alfen-charger) - Original Python implementation