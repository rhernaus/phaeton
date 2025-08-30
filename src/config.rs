//! Configuration management for Phaeton
//!
//! This module handles loading, validation, and management of the application
//! configuration from YAML files with support for environment variable overrides.

use crate::error::{PhaetonError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

mod defaults;

fn default_true() -> bool {
    true
}

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct Config {
    /// Modbus TCP connection configuration
    pub modbus: ModbusConfig,

    /// Device instance for D-Bus service naming
    pub device_instance: u32,

    /// Require D-Bus to be available; fail fast on startup if unavailable
    #[serde(default = "default_true")]
    pub require_dbus: bool,

    /// Modbus register address mappings
    pub registers: RegistersConfig,

    /// Default operational values
    pub defaults: DefaultsConfig,

    /// Logging configuration
    pub logging: LoggingConfig,

    /// Charging schedule configuration
    pub schedule: ScheduleConfig,

    /// Tibber API configuration for dynamic pricing
    pub tibber: TibberConfig,

    /// Control and safety limit configuration
    pub controls: ControlsConfig,

    /// Web server binding configuration
    pub web: WebConfig,

    /// Pricing configuration for session cost calculation
    pub pricing: PricingConfig,

    /// Updater configuration
    #[serde(default)]
    pub updates: UpdaterConfig,

    /// Polling interval in milliseconds
    pub poll_interval_ms: u64,

    /// Timezone for schedule operations
    pub timezone: String,

    /// Multiple vehicle configurations - kept out of schema & serialized output
    #[serde(skip_serializing)]
    #[cfg_attr(feature = "openapi", schemars(skip))]
    pub vehicles: Option<HashMap<String, serde_yaml::Value>>,
}

/// Modbus TCP connection parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct ModbusConfig {
    /// IP address of the EV charger
    pub ip: String,

    /// TCP port (typically 502)
    pub port: u16,

    /// Slave ID for socket-related registers
    pub socket_slave_id: u8,

    /// Slave ID for station configuration
    pub station_slave_id: u8,
}

/// Modbus register address mappings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct RegistersConfig {
    /// Voltage register addresses (L1, L2, L3)
    pub voltages: u16,

    /// Current register addresses (L1, L2, L3)
    pub currents: u16,

    /// Power register addresses
    pub power: u16,

    /// Energy counter register address
    pub energy: u16,

    /// Status string register address
    pub status: u16,

    /// Current setting register address
    pub amps_config: u16,

    /// Phase configuration register address
    pub phases: u16,

    /// Firmware version register addresses
    pub firmware_version: u16,
    pub firmware_version_count: u16,

    /// Serial number register addresses
    pub station_serial: u16,
    pub station_serial_count: u16,

    /// Manufacturer register addresses
    pub manufacturer: u16,
    pub manufacturer_count: u16,

    /// Platform type register addresses
    pub platform_type: u16,
    pub platform_type_count: u16,

    /// Station max current register address
    pub station_max_current: u16,

    /// Station status register address
    pub station_status: u16,
}

/// Default operational values
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct DefaultsConfig {
    /// Default charging current in amperes
    pub intended_set_current: f32,

    /// Default max current if read fails
    pub station_max_current: f32,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct LoggingConfig {
    /// Log level (DEBUG, INFO, WARNING, ERROR, CRITICAL)
    pub level: String,

    /// Optional override for console output level (defaults to `level`)
    #[serde(default)]
    pub console_level: Option<String>,

    /// Optional override for file output level (defaults to `level`)
    #[serde(default)]
    pub file_level: Option<String>,

    /// Optional override for web/SSE output level (defaults to `level`)
    #[serde(default)]
    pub web_level: Option<String>,

    /// Path to log file
    pub file: String,

    /// Log format (structured or simple)
    pub format: String,

    /// Max log file size in MB
    pub max_file_size_mb: u32,

    /// Number of backup files to keep
    pub backup_count: u32,

    /// Whether to log to console
    pub console_output: bool,

    /// Whether to use JSON format
    pub json_format: bool,
}

/// Individual schedule configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct ScheduleItem {
    /// Whether this schedule is active
    pub active: bool,

    /// List of days (0=Mon, 6=Sun)
    pub days: Vec<u8>,

    /// Start time in HH:MM format
    pub start_time: String,

    /// End time in HH:MM format
    pub end_time: String,

    // Legacy fields for compatibility
    pub enabled: u8,
    pub days_mask: u32,
    pub start: String,
    pub end: String,
}

/// Schedule configuration container
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct ScheduleConfig {
    /// Scheduling source: "time" (time-based windows) or "tibber" (price-based)
    #[serde(default = "default_schedule_mode")]
    pub mode: String,

    /// List of schedule items
    pub items: Vec<ScheduleItem>,
}

fn default_schedule_mode() -> String {
    "time".to_string()
}

impl Default for ScheduleConfig {
    fn default() -> Self {
        Self {
            mode: default_schedule_mode(),
            items: Vec::new(),
        }
    }
}

/// Tibber API configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct TibberConfig {
    /// Tibber API access token
    pub access_token: String,

    /// Optional specific home ID
    pub home_id: String,

    /// Charge when price level is CHEAP
    pub charge_on_cheap: bool,

    /// Charge when price level is VERY_CHEAP
    pub charge_on_very_cheap: bool,

    /// Selection strategy (level, threshold, percentile)
    pub strategy: String,

    /// Absolute price threshold for threshold strategy
    pub max_price_total: f64,

    /// Fraction of cheapest prices for percentile strategy
    pub cheap_percentile: f64,
}

/// Control and safety limits
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
#[serde(default)]
pub struct ControlsConfig {
    /// Tolerance for current verification
    pub current_tolerance: f32,

    /// Min difference to trigger update
    pub update_difference_threshold: f32,

    /// Delay before verifying settings
    pub verification_delay: f64,

    /// Delay between retries
    pub retry_delay: f64,

    /// Max retry attempts
    pub max_retries: u32,

    /// Watchdog interval in seconds
    pub watchdog_interval_seconds: u32,

    /// Max settable current
    pub max_set_current: f32,

    /// Minimum non-zero current to apply in automatic mode. If the computed
    /// current is below this threshold, we set 0 A to avoid oscillating with
    /// sub-minimum setpoints. Typical EVSE minimum is 6 A.
    pub min_set_current: f32,

    /// Min charge duration in seconds
    pub min_charge_duration_seconds: u32,

    /// Interval for refreshing current settings
    pub current_update_interval: u32,

    /// Verification delay in milliseconds
    pub verify_delay: u32,

    /// Time window to compensate measurement lag between Victron house loads
    /// and charger Modbus readings (milliseconds). During this window after a
    /// set-current change we subtract the expected EV power (derived from the
    /// last sent current) from house consumption instead of the charger-reported
    /// power to avoid double-counting.
    pub ev_reporting_lag_ms: u32,

    /// Exponential moving average smoothing factor for PV excess (0..1)
    /// Lower values increase smoothing; 0 disables and uses raw values.
    pub pv_excess_ema_alpha: f32,

    /// Minimum time between phase switches (seconds) to avoid oscillations
    pub phase_switch_grace_seconds: u32,

    /// Settling time to keep charging stopped after switching phases (seconds)
    pub phase_switch_settle_seconds: u32,

    /// Enable automatic 1P/3P switching in Auto mode
    pub auto_phase_switch: bool,

    /// Hysteresis margin in watts for auto phase switching decisions
    pub auto_phase_hysteresis_watts: f32,
}

/// Web server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct WebConfig {
    /// Bind address
    pub host: String,

    /// TCP port
    pub port: u16,
}

/// Pricing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct PricingConfig {
    /// Source (victron or static)
    pub source: String,

    /// Static rate in EUR/kWh
    pub static_rate_eur_per_kwh: f64,

    /// Currency symbol
    pub currency_symbol: String,
}

/// Updater configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct UpdaterConfig {
    /// Enable updater background tasks
    pub enabled: bool,
    /// Periodically check for updates
    pub auto_check: bool,
    /// Automatically apply updates when available
    pub auto_update: bool,
    /// Include prerelease versions
    pub include_prereleases: bool,
    /// Check interval in hours
    pub check_interval_hours: u32,
    /// Override repository URL (defaults to Cargo package repository)
    pub repository: String,
}

impl Config {
    /// Load configuration from a YAML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&contents)?;
        Ok(config)
    }

    /// Load configuration with an optional explicit override path.
    ///
    /// When `override_path` is provided, the configuration is loaded strictly
    /// from that path and any error (including file-not-found) is returned
    /// without falling back to default search locations. When `override_path`
    /// is `None`, this behaves like `load()` and searches default locations.
    pub fn load_with_override<P: AsRef<Path>>(override_path: Option<P>) -> Result<Self> {
        if let Some(p) = override_path {
            return Self::from_file(p);
        }
        Self::load()
    }

    /// Load configuration with validation
    pub fn load() -> Result<Self> {
        // Try to load from default locations
        let default_paths = [
            "phaeton_config.yaml",
            "/data/phaeton_config.yaml",
            "/etc/phaeton/config.yaml",
        ];

        for path in &default_paths {
            if Path::new(path).exists() {
                return Self::from_file(path);
            }
        }

        // Fall back to default configuration
        Ok(Config::default())
    }

    /// Save configuration to a YAML file
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let yaml = serde_yaml::to_string(self)?;
        std::fs::write(path, yaml)?;
        Ok(())
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        // Validate Modbus configuration
        if self.modbus.ip.is_empty() {
            return Err(PhaetonError::validation(
                "modbus.ip",
                "IP address cannot be empty",
            ));
        }

        if self.modbus.port == 0 {
            return Err(PhaetonError::validation(
                "modbus.port",
                "Port must be greater than 0",
            ));
        }

        // Validate current limits
        if self.defaults.intended_set_current <= 0.0 {
            return Err(PhaetonError::validation(
                "defaults.intended_set_current",
                "Must be positive",
            ));
        }

        if self.defaults.station_max_current <= 0.0 {
            return Err(PhaetonError::validation(
                "defaults.station_max_current",
                "Must be positive",
            ));
        }

        // Validate polling interval
        if self.poll_interval_ms == 0 {
            return Err(PhaetonError::validation(
                "poll_interval_ms",
                "Must be greater than 0",
            ));
        }

        Ok(())
    }
}

// Tests moved to `src/config_tests.rs` to keep file size within budget
