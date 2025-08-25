# Phaeton - EV Charger Driver

## Overview
This project is an EV charger driver, providing a performant, memory-safe, and maintainable implementation for integration with Victron Venus OS.

## Current Status
- **Project initialized**: ✅ Basic Rust project structure created
- **Phase**: Foundation setup (Phase 1) - **COMPLETED** ✅
- **Edition**: Migrated to Rust 2024 ✅
- **Code Quality**: Clippy clean; unit tests present for core modules ✅
- **Build/cross**: Cross-compilation scaffolding present for ARMv7 and AArch64 ✅
- **CI/CD**: GitHub Actions implemented (see `.github/workflows/`) ✅
- **Dependencies**: `tokio-modbus` at 0.16.1; nested Result handling adopted ✅

## Project Structure
```
phaeton/
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── config.rs          # Configuration management
│   ├── logging.rs         # Structured logging
│   ├── modbus.rs          # Modbus TCP client
│   ├── driver.rs          # Core driver logic
│   ├── dbus.rs            # D-Bus integration
│   ├── web.rs             # HTTP server and API (Axum + OpenAPI)
│   ├── persistence.rs     # State persistence
│   ├── session.rs         # Charging session management
│   ├── controls.rs        # Charging control algorithms
│   ├── tibber.rs          # Dynamic pricing integration
│   ├── vehicle.rs         # Vehicle API integrations
│   ├── updater.rs         # Self-update functionality
│   └── error.rs           # Error types and handling
├── tests/                 # Test modules
├── .github/workflows/     # CI workflows (tests, audit, cross-builds, releases)
├── docs/                  # Documentation
├── Cargo.toml
└── README.md
```

---

# Phase 1: Foundation (Priority: High) ✅ COMPLETED

## 1.1 Project Setup & Dependencies
- [x] **Initialize Cargo workspace** with proper package structure
- [x] **Configure Cargo.toml** with all necessary dependencies:
  - Async runtime (`tokio`, `tokio-util`, `tokio-stream`) ✅
  - Modbus client (`tokio-modbus`) ✅
  - Web framework (`axum`) ✅
  - Serialization (`serde`, `serde_yaml`, `serde_json`) ✅
  - Logging (`tracing`, `tracing-subscriber`, `tracing-appender`) ✅
  - Configuration (`config`) ✅
  - Error handling (`thiserror`, `anyhow`) ✅
  - HTTP client (`reqwest`) ✅
  - Date/time handling (`chrono`, `chrono-tz`) ✅
  - Git operations (`git2`) ✅
  - Utilities (`uuid`, `regex`, `futures`) ✅
- [x] **Set up cross-compilation** for ARM targets (Venus OS) - Configured in Cargo.toml
- [x] **Configure build profiles** (debug, release, cross-compile) - Optimized profiles added

## 1.2 Configuration System
- [x] **Implement YAML configuration parsing** using `serde_yaml`
- [x] **Create configuration structs** matching Python dataclasses:
  - `ModbusConfig` ✅
  - `RegistersConfig` ✅
  - `DefaultsConfig` ✅
  - `LoggingConfig` ✅
  - `ScheduleConfig` & `ScheduleItem` ✅
  - `ControlsConfig` ✅
  - `WebConfig` ✅
  - `PricingConfig` ✅
  - `TibberConfig` ✅
  - Main `Config` struct ✅
- [x] **Implement configuration validation** with custom validation rules
- [ ] **Add configuration hot-reloading** capability
- [ ] **Support environment variable overrides** (e.g., for containers)
- [ ] **Create configuration migration** logic for backward compatibility

## 1.3 Logging System
- [x] **Implement structured logging** using `tracing`
- [x] **Configure multiple output formats** (JSON, human-readable)
- [x] **Set up log rotation** and file management - Using tracing-appender
- [x] **Implement log streaming** for web UI integration via SSE (`/api/events`)
- [x] **Add performance tracing** for Modbus operations and control loops
- [x] **Create log context** system for request tracing

---

# Phase 2: Core Communication & Control (Priority: High) ✅ MOSTLY COMPLETE

## 2.1 Modbus TCP Client ✅ COMPLETED
- [x] **Implement async Modbus TCP client** using `tokio-modbus`
- [x] **Create register reading utilities**:
  - `read_holding_registers()` ✅
  - `read_modbus_string()` - Framework ready
  - `decode_32bit_float()` ✅
  - `decode_64bit_float()` ✅
- [x] **Implement connection management** with automatic reconnection
- [x] **Add retry logic** with exponential backoff - Basic retry logic implemented
- [x] **Create error handling** for Modbus-specific errors - Comprehensive error handling
- [x] **Implement timeout handling** for all operations
- [x] **Add connection pooling** for performance optimization - Connection manager ready
- [x] **Upgrade tokio-modbus to 0.16.1** and adopt nested `Result` handling

## 2.2 Core Driver Logic
- [x] **Implement main driver state machine** with `tokio::sync::mpsc` (command channel scaffold)
- [x] **Create polling loop** for periodic data collection (voltages, currents, power, energy, status)
- [x] **Implement charging control algorithms (initial)**:
  - Manual mode ✅
  - Auto mode (solar-based heuristic) ✅
  - Scheduled mode (timezone-aware windows) ✅
- [x] **Add state persistence** for mode/start/stop/set_current + session snapshot
- [ ] **Implement watchdog mechanisms** for fault tolerance
- [ ] **Create event system** for status changes and notifications (beyond SSE)
- [x] **Wire persistence and sessions** into the driver run loop
- [ ] **Implement status mapping parity** (e.g., LOW_SOC, WAIT_* states)
  - Current: Disconnected/Connected/Charging/Wait Sun/Wait Start/Low SOC
  - Next: error states parity (ground fault, overtemp, etc.)

## 2.3 Session Management
- [x] **Implement charging session tracking** with start/end detection
- [x] **Add session statistics** (duration, energy delivered, costs)
- [x] **Create session persistence** across restarts
- [x] **Implement session history** with configurable retention
- [ ] **Add session export** functionality for analysis

---

# Phase 3: System Integration (Priority: High) 🚧

## 3.1 D‑Bus Integration
- [x] **Implement D‑Bus service** using `zbus` and acquire name `com.victronenergy.evcharger.phaeton_<instance>`
- [x] **Expose `com.victronenergy.BusItem`** for core/writable paths mapped to driver commands
- [x] **Cache and update common paths** via internal store with `BusItem` getters
- [x] **Emit PropertiesChanged + ItemsChanged signals on updates** (VeDbus compatible)
- [x] **Normalize writes**: `/StartStop` (bool/num/string→0/1), `/Mode` (→0/1/2)
- [x] **Eliminate D‑Bus handle races** with shared Arc<Mutex<DbusService>>
- [ ] **Export complete object tree** with properties for all Venus OS paths (beyond cached reflection)
- [ ] **Implement Victron energy rate detection** from system D‑Bus

## 3.2 Web Server & API
- [x] **Migrate web framework**: fully migrated from `warp` to `axum` (no legacy code)
- [x] **Create REST API**: status, mode, start/stop, set current, config get/put, config schema
- [x] **Logs endpoints**: head, tail, download
- [x] **Updates endpoints**: status/check/apply (backed by stubs)
- [x] **SSE** at `/api/events` for live status
- [x] **Static UI** under `/ui` and alias `/app`
- [x] **OpenAPI/Swagger** at `/openapi.json` and `/docs`

---

# Phase 4: Advanced Features (Priority: Medium) 🚧

## 4.1 Tibber Integration
- [ ] **Implement Tibber API client** using `reqwest`
- [ ] **Create price data fetching** with proper authentication
- [ ] **Implement pricing strategies**: level, threshold, percentile
- [ ] **Add hourly price overview** generation
- [ ] **Implement price caching** and offline fallbacks

## 4.2 Vehicle Integration
- [ ] **Create Tesla API client** with OAuth2 authentication
- [ ] **Implement Kia/Hyundai API client** with login flow
- [ ] **Add vehicle status polling** with configurable intervals
- [ ] **Implement vehicle wake-up** logic when needed
- [ ] **Create vehicle data sanitization** and normalization
- [ ] **Add multiple vehicle support** with individual configurations

## 4.3 Update Management
- [ ] **Implement Git-based update checking** using `git2`
- [ ] **Create update download** and verification system
- [ ] **Add update scheduling** and automatic restarts
- [ ] **Implement branch management** for different update channels
- [ ] **Add update rollback** capability for failed deployments

---

# Phase 5: Testing & Quality Assurance (Priority: Medium) 🚧

## 5.1 Unit Testing
- [x] **Core unit tests**: controls, modbus utils, persistence, error types, sessions, tibber stub, updater stub
- [ ] **Add mock implementations** for external dependencies:
  - Modbus client mocks
  - D-Bus service mocks
  - HTTP client mocks
- [ ] **Implement property-based testing** for critical algorithms
- [ ] **Add integration tests** for module interactions

## 5.2 System Testing
- [ ] **Create end-to-end tests** for complete workflows
- [ ] **Add performance benchmarks** for critical paths
- [ ] **Implement load testing** for concurrent operations
- [ ] **Create hardware-in-the-loop tests** for Modbus communication

## 5.3 Documentation & Validation
- [x] **OpenAPI generated** and Swagger UI served
- [ ] **Write narrative API docs** with examples
- [ ] **Create deployment guides** for Venus OS
- [ ] **Add configuration reference** with all options
- [ ] **Create troubleshooting guide** for common issues

---

# Phase 6: Deployment & Operations (Priority: Low) 🚧

## 6.1 Build System
- [x] **Configure cross-compilation** scaffolding for ARM architecture
- [ ] **Create Docker build** environment for consistent builds
- [x] **Implement CI/CD pipeline** with automated testing and cross artifacts (GitHub Actions)
- [ ] **Add binary packaging** for different target platforms

## 6.2 Monitoring & Observability
- [ ] **Add Prometheus metrics** for system monitoring
- [x] **Health check endpoint** (`GET /api/health`)
- [ ] **Create performance profiling** capabilities
- [ ] **Add structured logging** to external systems

## 6.3 Security & Hardening
- [ ] **Implement input validation** for all user inputs
- [ ] **Add authentication/authorization** for sensitive operations
- [ ] **Create security audit** of dependencies
- [ ] **Add rate limiting** for API endpoints

---

# Implementation Notes

## Architecture Decisions
- **Async-first design** using Tokio for all I/O operations
- **Actor model** for state management and concurrent access
- **Repository pattern** for data persistence and configuration
- **Strategy pattern** for different charging modes and pricing strategies

## Development Workflow
1. Implement core modules (config, logging, modbus) first
2. Build minimal working driver with basic polling
3. Add D-Bus integration for Venus OS compatibility
4. Implement web interface and advanced features
5. Comprehensive testing and performance optimization

## Success Criteria
- [ ] All existing Python functionality ported to Rust
- [ ] Performance improvements (2-5x better resource usage)
- [ ] Memory safety verified with comprehensive testing
- [ ] Deployment on Venus OS with full D‑Bus integration
- [ ] Web UI functional with all features working

---

# Current Priority Tasks
1. ✅ Initialize Rust project structure
2. ✅ Configure Cargo.toml with dependencies
3. ✅ Implement configuration system
4. ✅ Implement logging system
5. ✅ Implement Modbus TCP client
6. ✅ Build core driver logic (polling + controls + sessions + persistence)
7. 🚧 Expose full D‑Bus paths and enrich Web API
8. ✅ Fix Start/Stop & Mode normalization; add human‑readable logs for `/Status` & `/Mode`
9. ✅ Auto‑mode grace logic: wait for sun on entry; dips only start timer when already charging

## Additional Completed Infrastructure
7. ✅ GitHub Actions CI/CD workflow present (tests, audit, macOS/Linux builds, cross)
8. ✅ Dependabot configured (cargo and actions)
9. ✅ Resolve all clippy warnings and errors (zero warnings)
10. ✅ Implement comprehensive error handling system
11. ✅ Create all core modules with proper architecture
12. ✅ Set up cross-compilation for Venus OS (ARM targets)
13. ✅ Migrate project to Rust 2024 edition
14. ✅ Vendor `git2`/OpenSSL to simplify cross-compilation

## Next Phase Ready
**Phase 3: System Integration**
- Expand D‑Bus property export to full object tree
- Keep enriching web API (auth, rate limiting) and wire real updater/Tibber backends
