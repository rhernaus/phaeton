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

/// Per-step timings of a single poll cycle in milliseconds
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PollStepDurations {
    /// Modbus read: voltages triplet
    pub read_voltages_ms: Option<u64>,
    /// Modbus read: currents triplet
    pub read_currents_ms: Option<u64>,
    /// Modbus read: powers and total power
    pub read_powers_ms: Option<u64>,
    /// Modbus read: energy counter
    pub read_energy_ms: Option<u64>,
    /// Modbus read: status string
    pub read_status_ms: Option<u64>,
    /// Modbus read: station max current
    pub read_station_max_ms: Option<u64>,
    /// D-Bus: compute PV excess (multiple reads under the hood)
    pub pv_excess_ms: Option<u64>,
    /// Compute effective current including SoC checks and grace logic
    pub compute_effective_ms: Option<u64>,
    /// Modbus write to apply current (only when performed)
    pub write_current_ms: Option<u64>,
    /// Finalize cycle (session update, persist, logs, SSE status)
    pub finalize_cycle_ms: Option<u64>,
    /// Build snapshot and broadcast to watchers
    pub snapshot_build_ms: Option<u64>,
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
    /// Whether Modbus appears connected (if known)
    pub modbus_connected: Option<bool>,
    /// Driver state (Initializing, Running, Error, ShuttingDown)
    pub driver_state: String,
    /// Optional per-step timings of the last poll cycle
    pub poll_steps_ms: Option<PollStepDurations>,
}

/// Commands accepted by the driver from external components (web, etc.)
#[derive(Debug, Clone)]
pub enum DriverCommand {
    SetMode(u8),
    SetStartStop(u8),
    SetCurrent(f32),
    SetPhases(u8),
}
