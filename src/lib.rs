//! # Phaeton - Alfen EV Charger Driver for Victron Venus OS
//!
//! A high-performance Rust implementation of the Alfen EV charger driver,
//! providing seamless integration with Victron Venus OS through D-Bus and
//! offering advanced features like dynamic pricing, vehicle integration, and
//! self-updates.
//!
//! ## Features
//!
//! - **High Performance**: Async-first design with Tokio runtime
//! - **Memory Safe**: Rust's ownership system prevents common bugs
//! - **Modbus TCP**: Direct communication with Alfen EV chargers
//! - **D-Bus Integration**: Full Venus OS compatibility
//! - **Web Interface**: REST API and static file serving
//! - **Dynamic Pricing**: Tibber API integration for smart charging
//! - **Vehicle Integration**: Tesla and Kia API support
//! - **Self-Updates**: Git-based automatic updates
//! - **Configuration**: YAML-based configuration with validation
//!
//! ## Architecture
//!
//! The application follows a modular architecture with clear separation of concerns:
//!
//! - `config`: Configuration management and validation
//! - `logging`: Structured logging and tracing
//! - `modbus`: Modbus TCP client for charger communication
//! - `driver`: Core driver logic and state management
//! - `dbus`: D-Bus integration for Venus OS
//! - `web`: HTTP server and REST API
//! - `persistence`: State persistence and recovery
//! - `session`: Charging session management
//! - `controls`: Charging control algorithms
//! - `tibber`: Dynamic pricing integration
//! - `vehicle`: Vehicle API integrations
//! - `updater`: Self-update functionality

pub mod config;
pub mod dbus;
pub mod driver;
pub mod error;
pub mod logging;
pub mod modbus;
// pub mod web; // Commented out due to warp compatibility issues
pub mod controls;
pub mod persistence;
pub mod session;
pub mod tibber;
pub mod updater;
pub mod vehicle;

// Re-export commonly used types
pub use config::Config;
pub use driver::AlfenDriver;
pub use error::{PhaetonError, Result};
