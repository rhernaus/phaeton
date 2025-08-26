//! Configuration management for Phaeton
//!
//! This module handles loading, validation, and management of the application
//! configuration from YAML files with support for environment variable overrides.

use crate::error::{PhaetonError, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

fn default_true() -> bool {
    true
}

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

    /// Polling interval in milliseconds
    pub poll_interval_ms: u64,

    /// Timezone for schedule operations
    pub timezone: String,

    /// Vehicle integrations (optional) - keep out of schema & serialized output
    #[serde(skip_serializing)]
    #[schemars(skip)]
    pub vehicle: Option<HashMap<String, serde_yaml::Value>>,

    /// Multiple vehicle configurations - keep out of schema & serialized output
    #[serde(skip_serializing)]
    #[schemars(skip)]
    pub vehicles: Option<HashMap<String, serde_yaml::Value>>,
}

/// Modbus TCP connection parameters
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DefaultsConfig {
    /// Default charging current in amperes
    pub intended_set_current: f32,

    /// Default max current if read fails
    pub station_max_current: f32,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LoggingConfig {
    /// Log level (DEBUG, INFO, WARNING, ERROR, CRITICAL)
    pub level: String,

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
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
pub struct ScheduleConfig {
    /// List of schedule items
    pub items: Vec<ScheduleItem>,
}

/// Tibber API configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TibberConfig {
    /// Tibber API access token
    pub access_token: String,

    /// Whether Tibber integration is enabled
    pub enabled: bool,

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
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
}

/// Web server configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WebConfig {
    /// Bind address
    pub host: String,

    /// TCP port
    pub port: u16,
}

/// Pricing configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PricingConfig {
    /// Source (victron or static)
    pub source: String,

    /// Static rate in EUR/kWh
    pub static_rate_eur_per_kwh: f64,

    /// Currency symbol
    pub currency_symbol: String,
}

impl Default for ModbusConfig {
    fn default() -> Self {
        Self {
            ip: "192.168.1.100".to_string(),
            port: 502,
            socket_slave_id: 1,
            station_slave_id: 200,
        }
    }
}

impl Default for RegistersConfig {
    fn default() -> Self {
        Self {
            voltages: 306,
            currents: 320,
            power: 338,
            energy: 374,
            status: 1201,
            amps_config: 1210,
            phases: 1215,
            firmware_version: 123,
            firmware_version_count: 17,
            station_serial: 157,
            station_serial_count: 11,
            manufacturer: 117,
            manufacturer_count: 5,
            platform_type: 140,
            platform_type_count: 17,
            station_max_current: 1100,
            station_status: 1201,
        }
    }
}

impl Default for DefaultsConfig {
    fn default() -> Self {
        Self {
            intended_set_current: 6.0,
            station_max_current: 32.0,
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "INFO".to_string(),
            file: "/tmp/phaeton.log".to_string(),
            format: "structured".to_string(),
            max_file_size_mb: 10,
            backup_count: 5,
            console_output: true,
            json_format: false,
        }
    }
}

impl Default for TibberConfig {
    fn default() -> Self {
        Self {
            access_token: String::new(),
            enabled: false,
            home_id: String::new(),
            charge_on_cheap: true,
            charge_on_very_cheap: true,
            strategy: "level".to_string(),
            max_price_total: 0.0,
            cheap_percentile: 0.3,
        }
    }
}

impl Default for ControlsConfig {
    fn default() -> Self {
        Self {
            current_tolerance: 0.5,
            update_difference_threshold: 0.1,
            verification_delay: 0.1,
            retry_delay: 0.5,
            max_retries: 3,
            watchdog_interval_seconds: 30,
            max_set_current: 64.0,
            min_set_current: 6.0,
            min_charge_duration_seconds: 300,
            current_update_interval: 30000,
            verify_delay: 100,
            ev_reporting_lag_ms: 2000,
            pv_excess_ema_alpha: 0.4,
        }
    }
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8088,
        }
    }
}

impl Default for PricingConfig {
    fn default() -> Self {
        Self {
            source: "static".to_string(),
            static_rate_eur_per_kwh: 0.25,
            currency_symbol: "€".to_string(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            modbus: ModbusConfig::default(),
            device_instance: 0,
            require_dbus: true,
            registers: RegistersConfig::default(),
            defaults: DefaultsConfig::default(),
            logging: LoggingConfig::default(),
            schedule: ScheduleConfig::default(),
            tibber: TibberConfig::default(),
            controls: ControlsConfig::default(),
            poll_interval_ms: 1000,
            timezone: "UTC".to_string(),
            web: WebConfig::default(),
            pricing: PricingConfig::default(),
            vehicle: None,
            vehicles: None,
        }
    }
}

impl Config {
    /// Load configuration from a YAML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&contents)?;
        Ok(config)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.modbus.port, 502);
        assert_eq!(config.device_instance, 0);
        assert_eq!(config.poll_interval_ms, 1000);
        assert!(config.require_dbus);
    }

    #[test]
    fn test_config_validation() {
        let mut config = Config::default();
        assert!(config.validate().is_ok());

        // Test invalid IP
        config.modbus.ip = String::new();
        assert!(config.validate().is_err());

        // Reset and test invalid port
        config = Config::default();
        config.modbus.port = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let yaml = serde_yaml::to_string(&config).unwrap();
        let deserialized: Config = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(config.modbus.port, deserialized.modbus.port);
    }
}
