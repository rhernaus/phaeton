use serde::{Deserialize, Serialize};

/// Main driver state
#[derive(Debug, Clone, PartialEq)]
pub enum DriverState {
    /// Driver is initializing
    Initializing,
    /// Driver is running normally
    Running,
    /// Driver is in error state
    Error(String),
    /// Driver is shutting down
    ShuttingDown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverSnapshot {
    pub timestamp: String,
    pub mode: u8,
    pub start_stop: u8,
    pub set_current: f32,
    pub applied_current: f32,
    pub station_max_current: f32,
    pub device_instance: u32,
    pub product_name: Option<String>,
    pub firmware: Option<String>,
    pub serial: Option<String>,
    pub status: u32,
    pub active_phases: u8,
    pub ac_power: f64,
    pub ac_current: f64,
    pub l1_voltage: f64,
    pub l2_voltage: f64,
    pub l3_voltage: f64,
    pub l1_current: f64,
    pub l2_current: f64,
    pub l3_current: f64,
    pub l1_power: f64,
    pub l2_power: f64,
    pub l3_power: f64,
    pub total_energy_kwh: f64,
    pub pricing_currency: Option<String>,
    pub energy_rate: Option<f64>,
    pub session: serde_json::Value,
    pub poll_duration_ms: Option<u64>,
    pub total_polls: u64,
    pub overrun_count: u64,
    pub poll_interval_ms: u64,
    pub excess_pv_power_w: f32,
}

/// Commands accepted by the driver from external components (web, etc.)
#[derive(Debug, Clone)]
pub enum DriverCommand {
    SetMode(u8),
    SetStartStop(u8),
    SetCurrent(f32),
}
