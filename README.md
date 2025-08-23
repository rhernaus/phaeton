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

🚧 **Work in Progress**: This is a rewrite of the Python [victron-alfen-charger](https://github.com/your-org/victron-alfen-charger) project in Rust. Currently in Phase 1 (Foundation) of development.

## Quick Start

### Prerequisites

- Rust 1.70+ with 2021 edition
- Venus OS or Linux system with D-Bus support
- Alfen EV charger accessible via network

### Installation

```bash
# Clone the repository
git clone https://github.com/your-org/phaeton.git
cd phaeton

# Build the project
cargo build --release

# Run the driver
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

### REST API Endpoints

- `GET /api/status` - Current system status
- `GET /api/config` - Get configuration
- `PUT /api/config` - Update configuration
- `POST /api/mode` - Change charging mode
- `POST /api/startstop` - Start/stop charging
- `POST /api/set_current` - Set charging current

### WebSocket Support

Real-time updates are available via WebSocket connections for live monitoring.

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
FROM rust:1.70-slim as builder
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