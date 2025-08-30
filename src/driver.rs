//! Core driver logic for Phaeton
//!
//! This module contains the main driver state machine and orchestration logic
//! that coordinates all the different components of the system.

use crate::config::Config;
use crate::controls::{ChargingControls, ChargingMode, StartStopState};
use crate::dbus::DbusService;
use crate::error::Result;
// removed unused imports: LogContext, get_logger
// removed direct use; now via trait in runtime
use crate::persistence::PersistenceManager;
use crate::session::ChargingSessionManager;
// serde only used by types; keep driver free of unused imports
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, watch};
// tokio::time only used in runtime modules

mod types;
pub use types::{DriverCommand, DriverSnapshot, DriverState};
// internal worker types moved out; keep type module private
mod commands;
mod dbus_helpers;
pub mod modbus_like;
mod pv;
mod runtime;
mod runtime_arc;
mod runtime_poll;
mod snapshot;

// Measurements and ModbusCommand moved to types.rs

/// Main driver for Phaeton
pub struct AlfenDriver {
    /// Configuration
    config: Config,

    /// Current driver state
    state: watch::Sender<DriverState>,
    /// Keep one receiver alive so state updates always succeed
    #[allow(dead_code)]
    state_rx: watch::Receiver<DriverState>,

    /// Modbus connection manager (trait for testability)
    modbus_manager: Option<Box<dyn modbus_like::ModbusLike>>,

    /// Logger with context
    logger: crate::logging::StructuredLogger,

    /// Shutdown signal
    shutdown_tx: mpsc::UnboundedSender<()>,

    /// Shutdown receiver
    shutdown_rx: mpsc::UnboundedReceiver<()>,

    /// Persistence manager
    persistence: PersistenceManager,

    /// Session manager
    sessions: ChargingSessionManager,

    /// D-Bus service shared across tasks; guard with a mutex to avoid take/restore races
    dbus: Option<Arc<tokio::sync::Mutex<DbusService>>>,

    /// Controls logic
    controls: ChargingControls,

    /// Control state
    current_mode: ChargingMode,
    start_stop: StartStopState,
    intended_set_current: f32,
    station_max_current: f32,
    last_sent_current: f32,
    last_current_set_time: std::time::Instant,
    /// When we last changed the current setpoint (monotonic clock)
    last_set_current_monotonic: std::time::Instant,
    /// Deadline for minimum-charge grace timer when excess < 6A
    min_charge_timer_deadline: Option<std::time::Instant>,
    /// Marker when entering Auto mode; used to suppress grace timer until first Auto charging
    auto_mode_entered_at: Option<std::time::Instant>,
    /// Last observed Victron-esque status (0=Disc,1=Conn,2=Charging)
    last_status: u8,

    /// Command receiver for external control
    commands_rx: mpsc::UnboundedReceiver<DriverCommand>,

    /// Command sender (fan-out to subsystems like D-Bus, web if needed)
    commands_tx: mpsc::UnboundedSender<DriverCommand>,

    /// Broadcast channel for streaming live status updates (SSE)
    status_tx: broadcast::Sender<String>,

    /// Watch channel for full status snapshot consumed by web and other readers
    status_snapshot_tx: watch::Sender<Arc<DriverSnapshot>>,
    status_snapshot_rx: watch::Receiver<Arc<DriverSnapshot>>,

    // Last measured values (from Modbus) for snapshot building
    last_l1_voltage: f64,
    last_l2_voltage: f64,
    last_l3_voltage: f64,
    last_l1_current: f64,
    last_l2_current: f64,
    last_l3_current: f64,
    last_l1_power: f64,
    last_l2_power: f64,
    last_l3_power: f64,
    last_total_power: f64,
    last_energy_kwh: f64,

    // Identity cache (to avoid depending on DBus for UI identity fields)
    product_name: Option<String>,
    firmware_version: Option<String>,
    serial: Option<String>,

    // Poll metrics
    total_polls: u64,
    overrun_count: u64,

    // Last computed PV excess power
    last_excess_pv_power_w: f32,

    /// Per-step timings for the last completed poll cycle
    last_poll_steps: Option<crate::driver::types::PollStepDurations>,

    /// Desired number of phases (1 or 3)
    desired_phases: u8,
    /// Last applied number of phases as known by the driver
    applied_phases: u8,
    /// Time when phases last changed (for grace)
    last_phase_switch: Option<std::time::Instant>,
    /// If currently settling after a phase switch, this is the deadline
    phase_settle_deadline: Option<std::time::Instant>,

    /// If set during a phase switch settle period, indicates the target phase count (1 or 3)
    /// Used to expose Victron D-Bus status 22/23 (switching to 3P/1P)
    phase_switch_to: Option<u8>,
}

impl AlfenDriver {
    // initialize_modbus moved to runtime.rs

    // poll_cycle moved to runtime.rs

    // shutdown moved to runtime.rs

    /// Get current driver state
    pub fn get_state(&self) -> DriverState {
        self.state.borrow().clone()
    }

    /// Request shutdown
    pub fn request_shutdown(&self) {
        self.shutdown_tx.send(()).ok();
    }

    /// Get configuration reference
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Update configuration safely (no hot-restart of subsystems yet)
    pub fn update_config(&mut self, new_config: Config) -> Result<()> {
        // Basic validation already expected by caller
        self.config = new_config;
        Ok(())
    }

    /// Accessors for web/UI
    pub fn current_mode_code(&self) -> u8 {
        self.current_mode as u8
    }

    pub fn start_stop_code(&self) -> u8 {
        self.start_stop as u8
    }

    pub fn get_intended_set_current(&self) -> f32 {
        self.intended_set_current
    }

    pub fn get_station_max_current(&self) -> f32 {
        self.station_max_current
    }

    // get_db_value moved to dbus_helpers.rs

    // /// Snapshot of cached D-Bus paths (subset of known keys)
    // get_dbus_cache_snapshot moved to dbus_helpers.rs

    /// Get sessions data
    pub fn sessions_snapshot(&self) -> serde_json::Value {
        self.sessions.get_state()
    }

    // subscribe_status moved to dbus_helpers.rs
}

// PV computation moved to pv.rs

impl AlfenDriver {
    // refresh_charger_identity moved to dbus_helpers.rs

    /// Run the driver using a shared `Arc<Mutex<AlfenDriver>>` without holding the lock across the entire loop.
    ///
    /// This avoids starving other components (e.g., web server) that need brief access to the driver state.
    pub async fn run_on_arc(driver: Arc<tokio::sync::Mutex<AlfenDriver>>) -> Result<()> {
        self::runtime_arc::run_on_arc_impl(driver).await
    }
}

impl AlfenDriver {
    // try_start_dbus_with_identity moved to dbus_helpers.rs
}

impl AlfenDriver {
    // /// Handle external command
    // handle_command moved to commands.rs

    /// Map Alfen Mode3 status string to Victron-esque numeric status
    /// 0=Disconnected, 1=Connected, 2=Charging
    pub(crate) fn map_alfen_status_to_victron(status_str: &str) -> u8 {
        let s = status_str
            .trim_matches(|c: char| c == char::from(0) || c.is_whitespace())
            .to_uppercase();
        match s.as_str() {
            "C2" | "D2" => 2,
            "B1" | "B2" | "C1" | "D1" => 1,
            "A" | "E" | "F" => 0,
            _ => 0,
        }
    }
}

// Control callbacks for Mode/StartStop/SetCurrent updates (stub: call these from web API later)
impl AlfenDriver {
    pub async fn set_mode(&mut self, mode: u8) {
        let new_mode = match mode {
            1 => ChargingMode::Auto,
            2 => ChargingMode::Scheduled,
            _ => ChargingMode::Manual,
        };
        if new_mode as u8 != self.current_mode as u8 {
            let name = |v: u8| match v {
                0 => "Manual",
                1 => "Auto",
                2 => "Scheduled",
                _ => "Unknown",
            };
            self.logger.info(&format!(
                "Mode changed: {} ({}) -> {} ({})",
                self.current_mode as u8,
                name(self.current_mode as u8),
                new_mode as u8,
                name(new_mode as u8)
            ));
        }
        self.current_mode = new_mode;
        // If entering Auto, clear any existing grace timer and mark entry time.
        if matches!(self.current_mode, ChargingMode::Auto) {
            self.min_charge_timer_deadline = None;
            self.auto_mode_entered_at = Some(std::time::Instant::now());
        }
        if let Some(dbus) = &self.dbus {
            let _ = dbus
                .lock()
                .await
                .update_path("/Mode", serde_json::json!(mode))
                .await;
        }
        self.persistence.set_mode(self.current_mode as u32);
        let _ = self.persistence.save();
    }

    pub async fn set_start_stop(&mut self, value: u8) {
        self.start_stop = if value == 1 {
            StartStopState::Enabled
        } else {
            StartStopState::Stopped
        };
        self.logger.info(&format!(
            "StartStop changed: {}",
            match self.start_stop {
                StartStopState::Enabled => "enabled",
                StartStopState::Stopped => "stopped",
            }
        ));
        if let Some(dbus) = &self.dbus {
            let _ = dbus
                .lock()
                .await
                .update_path("/StartStop", serde_json::json!(value))
                .await;
        }
        self.persistence.set_start_stop(self.start_stop as u32);
        let _ = self.persistence.save();
    }

    pub async fn set_intended_current(&mut self, amps: f32) {
        let clamped = amps.max(0.0).min(self.config.controls.max_set_current);
        self.intended_set_current = clamped;
        if let Some(dbus) = &self.dbus {
            let _ = dbus
                .lock()
                .await
                .update_path("/SetCurrent", serde_json::json!(clamped))
                .await;
        }
        self.persistence.set_set_current(self.intended_set_current);
        let _ = self.persistence.save();
        // Record the moment we changed the intended current to enable lag compensation
        self.last_set_current_monotonic = std::time::Instant::now();
    }

    /// Set desired number of phases (1 or 3). Applies immediately in Manual/Scheduled; in Auto it may be overridden.
    pub async fn set_phases(&mut self, phases: u8) {
        let p = if phases >= 3 { 3 } else { 1 };
        if p != self.desired_phases {
            self.logger.info(&format!(
                "Desired phases changed: {} -> {}",
                self.desired_phases, p
            ));
        }
        self.desired_phases = p;
        // In Manual or Scheduled, apply immediately
        if !matches!(self.current_mode, ChargingMode::Auto) {
            let _ = self.apply_phases_now(p).await;
        }
        if let Some(dbus) = &self.dbus {
            let _ = dbus
                .lock()
                .await
                .update_path("/Ac/PhaseCount", serde_json::json!(p))
                .await;
        }
    }

    async fn apply_phases_now(&mut self, p: u8) -> bool {
        let target = if p >= 3 { 3 } else { 1 };
        if target == self.applied_phases && self.phase_settle_deadline.is_none() {
            return true;
        }
        // Stop charging during switch
        let prev_current = self.last_sent_current;
        // Write 0.0 A to amps register directly (avoid cross-module private call)
        if let Some(mgr) = self.modbus_manager.as_mut() {
            let socket_id = self.config.modbus.socket_slave_id;
            let addr_amps = self.config.registers.amps_config;
            let regs = crate::modbus::encode_32bit_float(0.0);
            let _ = mgr
                .write_multiple_registers(socket_id, addr_amps, &regs)
                .await
                .ok();
        }
        self.last_sent_current = 0.0;
        self.last_current_set_time = std::time::Instant::now();

        // Write the phases register
        let station_id = self.config.modbus.station_slave_id;
        let addr_phases = self.config.registers.phases;
        let value: u16 = if target == 3 { 3 } else { 1 };
        let write_ok = if let Some(mgr) = self.modbus_manager.as_mut() {
            mgr.write_multiple_registers(station_id, addr_phases, &[value])
                .await
                .is_ok()
        } else {
            false
        };

        if write_ok {
            self.applied_phases = target;
            self.last_phase_switch = Some(std::time::Instant::now());
            let settle = self.config.controls.phase_switch_settle_seconds as u64;
            self.phase_settle_deadline =
                Some(std::time::Instant::now() + std::time::Duration::from_secs(settle));
            self.phase_switch_to = Some(target);
            self.logger.info(&format!(
                "Switched phases to {}P; settling for {}s (prev current {:.1} A)",
                target, settle, prev_current
            ));
            // Update D-Bus to reflect switching status immediately (22/23)
            if let Some(dbus) = &self.dbus {
                let status_code: u8 = if target == 3 { 22 } else { 23 };
                let _ = dbus
                    .lock()
                    .await
                    .update_paths([
                        ("/Status".to_string(), serde_json::json!(status_code)),
                        ("/Ac/PhaseCount".to_string(), serde_json::json!(target)),
                    ])
                    .await;
            }
            true
        } else {
            self.logger
                .warn("Failed to write phase configuration via Modbus");
            false
        }
    }
}

impl AlfenDriver {
    // last_poll_duration_ms moved to runtime.rs
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[test]
    fn map_status_variants() {
        assert_eq!(AlfenDriver::map_alfen_status_to_victron("C2"), 2);
        assert_eq!(AlfenDriver::map_alfen_status_to_victron("D2"), 2);
        assert_eq!(AlfenDriver::map_alfen_status_to_victron("B2"), 1);
        assert_eq!(AlfenDriver::map_alfen_status_to_victron("c1"), 1);
        assert_eq!(AlfenDriver::map_alfen_status_to_victron("A"), 0);
        assert_eq!(AlfenDriver::map_alfen_status_to_victron("unknown"), 0);
        // Ensure trimming of NUL and spaces
        assert_eq!(AlfenDriver::map_alfen_status_to_victron(" B1\0\0 "), 1);
    }

    #[tokio::test]
    async fn set_mode_updates_state() {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut driver = AlfenDriver::new(rx, tx.clone()).await.unwrap();
        assert_eq!(driver.current_mode_code(), 0);
        driver.set_mode(1).await; // Auto
        assert_eq!(driver.current_mode_code(), 1);
        driver.set_mode(2).await; // Scheduled
        assert_eq!(driver.current_mode_code(), 2);
    }

    #[tokio::test]
    async fn start_stop_updates_state() {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut driver = AlfenDriver::new(rx, tx.clone()).await.unwrap();
        assert_eq!(driver.start_stop_code(), 0);
        driver.set_start_stop(1).await; // Enabled
        assert_eq!(driver.start_stop_code(), 1);
        driver.set_start_stop(0).await; // Stopped
        assert_eq!(driver.start_stop_code(), 0);
    }

    #[tokio::test]
    async fn intended_current_is_clamped_and_recorded() {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut driver = AlfenDriver::new(rx, tx.clone()).await.unwrap();
        // Over the max_set_current should clamp
        driver.set_intended_current(100.0).await;
        let max_allowed = driver.config().controls.max_set_current;
        assert!((driver.get_intended_set_current() - max_allowed).abs() < f32::EPSILON);
        // Negative clamps to 0
        driver.set_intended_current(-5.0).await;
        assert!((driver.get_intended_set_current() - 0.0).abs() < f32::EPSILON);
    }

    #[tokio::test]
    async fn subscribe_snapshot_has_initial_value() {
        let (tx, rx) = mpsc::unbounded_channel();
        let driver = AlfenDriver::new(rx, tx.clone()).await.unwrap();
        let snapshot = driver.subscribe_snapshot().borrow().clone();
        let snap = (*snapshot).clone();
        assert_eq!(snap.device_instance, driver.config().device_instance);
        assert_eq!(snap.mode, driver.current_mode_code());
    }
}
