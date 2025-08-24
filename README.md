# Phaeton - EV Charger Driver

A high-performance EV charger driver for Victron Venus OS, providing seamless integration with Victron's D-Bus system and advanced features like dynamic pricing, vehicle integration, and self-updates.

## Features

### Implemented

- **High Performance**: Async-first design with Tokio runtime
- **Memory Safe**: Rust's ownership system prevents common bugs
- **Modbus TCP**: Async client with reconnect/backoff and decoding utilities
- **Web Interface**: Axum REST API, SSE events, logs endpoints; static UI served under `/ui` and `/app`; OpenAPI at `/openapi.json` and Swagger UI at `/docs`
- **D-Bus Integration (partial)**: Service registration and `com.victronenergy.BusItem` exposure for core paths; writable controls map to driver commands
- **Sessions & Persistence**: Session tracking, stats, and persistence across restarts; optional static pricing for session cost
- **Configuration**: YAML configuration with validation; schema exposed at `/api/config/schema`
- **Logging**: Structured logging with rotation; context-rich tracing

### Planned / In progress

- **Dynamic Pricing (Tibber)**: API client and pricing strategies
- **Vehicle Integration**: Tesla and Kia clients
- **Self-Updates**: Git-based update check/apply
- **D-Bus**: Export full object tree and broader Venus OS parity
- **Security**: Authentication/authorization and rate limiting for the web API

## Quick Start

### Download prebuilt binaries (recommended)

Grab the latest binaries from [Releases](https://github.com/rhernaus/phaeton/releases).
Artifacts are named with the GitHub release tag:

- **Cerbo GX (ARMv7)**: `phaeton-<tag>-armv7-unknown-linux-gnueabihf.tar.gz`
- **Linux ARM64**: `phaeton-<tag>-aarch64-unknown-linux-gnu.tar.gz`
- **Linux AMD64**: `phaeton-<tag>-x86_64-unknown-linux-gnu.tar.gz`

Verify checksums (Linux):

```bash
curl -L -O https://github.com/rhernaus/phaeton/releases/download/<tag>/SHA256SUMS
curl -L -O https://github.com/rhernaus/phaeton/releases/download/<tag>/phaeton-<tag>-<artifact>.tar.gz
sha256sum -c SHA256SUMS
```

Verify on Linux AMD64:

```bash
curl -L -O https://github.com/rhernaus/phaeton/releases/download/<tag>/SHA256SUMS
curl -L -O https://github.com/rhernaus/phaeton/releases/download/<tag>/phaeton-<tag>-x86_64-unknown-linux-gnu.tar.gz
sha256sum phaeton-<tag>-x86_64-unknown-linux-gnu.tar.gz
grep phaeton-<tag>-x86_64-unknown-linux-gnu.tar.gz SHA256SUMS
```

Install:

```bash
tar -xzf phaeton-<tag>-<artifact>.tar.gz
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
- EV charger accessible via network

### Installation

```bash
# Clone the repository
git clone https://github.com/rhernaus/phaeton.git
cd phaeton

# Build the project
cargo build --release

# Run the driver (spawns web server at 127.0.0.1:8088)
cargo run
```

### Configuration

Copy the sample configuration and edit as needed:

```bash
cp phaeton_config.sample.yaml phaeton_config.yaml
# Edit the configuration file with your settings
```

Phaeton will automatically look for a configuration file at the following locations, in order:

- `./phaeton_config.yaml`
- `/data/phaeton_config.yaml`
- `/etc/phaeton/config.yaml`

You can also retrieve the JSON schema via the API at `/api/config/schema`.

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
```

### Cross-Compilation

Phaeton supports cross-compilation for multiple architectures to run on different systems:

#### Local Builds

On developer machines, build only for the current host:

```bash
cargo build --release
```

#### GitHub Actions CI

Tagging a version (e.g., `v0.1.0`) triggers a release build that uploads signed artifacts and checksums to [Releases](https://github.com/rhernaus/phaeton/releases). Pushes to `main` update the rolling `nightly` prerelease. CI workflows live under `.github/workflows/` and cover testing, linting, security audit, cross-compilation, and release publishing.

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

# Build for Linux AMD64 (native or cross)
cargo build --target x86_64-unknown-linux-gnu --release
```



### CI/CD

- Workflows are defined in `.github/workflows/`.
- On pull requests and pushes: run tests, clippy, fmt check, and `cargo audit`.
- On tag (e.g., `v0.x.y`): build cross-compiled artifacts (ARMv7, AArch64, x86_64), generate `SHA256SUMS`, and publish to Releases.
- Nightly prerelease is updated from `main`.

## Architecture

The application follows a modular architecture with clear separation of concerns:

- `config`: Configuration management and validation
- `logging`: Structured logging and tracing
- `modbus`: Modbus TCP client for charger communication
- `driver`: Core driver logic and state management
- `dbus`: D-Bus integration for Venus OS
- `web`: HTTP server and REST API (Axum + OpenAPI)
- `persistence`: State persistence and recovery
- `session`: Charging session management
- `controls`: Charging control algorithms
- `tibber`: Dynamic pricing integration
- `vehicle`: Vehicle API integrations
- `updater`: Self-update functionality

## API Documentation

### REST API Endpoints (available)

- `GET /api/health` - Health check
- `GET /api/status` - Current system status
- `POST /api/mode` - Change charging mode
- `POST /api/startstop` - Start/stop charging
- `POST /api/set_current` - Set charging current
- `GET /api/config` - Get configuration
- `PUT /api/config` - Update configuration
- `GET /api/config/schema` - Configuration schema
- `GET /api/logs/head` - Head of log
- `GET /api/logs/tail` - Tail of log
- `GET /api/logs/download` - Download full log
- `GET /api/sessions` - Sessions snapshot
- `GET /api/dbus` - Cached D‑Bus values
- `GET /api/update/status` - Update status
- `POST /api/update/check` - Check for updates
- `POST /api/update/apply` - Apply updates
- `GET /api/events` - Server-Sent Events (live status)

### OpenAPI / Swagger

- OpenAPI JSON: `/openapi.json`
- Swagger UI: `/docs`

### Static Web UI

- UI assets are served under `/ui` (and `/app` as an alias for compatibility)

### Security

- The HTTP API currently has no authentication and enables permissive CORS for development.
- Deploy behind a trusted network or reverse proxy that enforces authentication.
- Log file path defaults to `/var/log/phaeton.log`; ensure appropriate permissions.

### Known limitations

- Tibber, vehicle integrations, and updater are stubbed (not yet implemented).
- D-Bus export is functional for core paths but not yet complete for all Venus OS paths.
- API is unauthenticated; do not expose directly to untrusted networks.

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

This project is licensed under MIT or Apache-2.0; see the LICENSE files for details.
