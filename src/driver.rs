//! Core driver logic for Phaeton
//!
//! This module contains the main driver state machine and orchestration logic
//! that coordinates all the different components of the system.

use crate::config::Config;
use crate::controls::{ChargingControls, ChargingMode, StartStopState};
use crate::dbus::DbusService;
use crate::error::Result;
use crate::logging::{LogContext, get_logger};
use crate::modbus::{
    ModbusConnectionManager, decode_32bit_float, decode_64bit_float, decode_string,
};
use crate::persistence::PersistenceManager;
use crate::session::ChargingSessionManager;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, watch};
use tokio::time::{Duration, interval};

/// Main driver state
#[derive(Debug, Clone)]
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

/// Measurements sampled from Modbus by the worker
struct Measurements {
    l1_v: f64,
    l2_v: f64,
    l3_v: f64,
    l1_i: f64,
    l2_i: f64,
    l3_i: f64,
    l1_p: f64,
    l2_p: f64,
    l3_p: f64,
    p_total: f64,
    energy_kwh: f64,
    status_base: i32,
    duration_ms: u64,
    overran: bool,
}

/// Commands to the Modbus worker
enum ModbusCommand {
    WriteSetCurrent(f32),
}

/// Main driver for Phaeton
pub struct AlfenDriver {
    /// Configuration
    config: Config,

    /// Current driver state
    state: watch::Sender<DriverState>,

    /// Modbus connection manager
    modbus_manager: Option<ModbusConnectionManager>,

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

    /// D-Bus service (owned here; avoid holding driver lock across awaits by temporarily taking it)
    dbus: Option<DbusService>,

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
    /// Smoothed PV excess power (EMA) for stability
    pv_excess_ema_w: f32,
    /// Timestamp when PV excess was last clearly non-zero (for zero-grace)
    last_nonzero_excess_at: std::time::Instant,
    /// Deadline for minimum-charge grace timer when excess < 6A
    min_charge_timer_deadline: Option<std::time::Instant>,
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
}

impl AlfenDriver {
    /// Create a new driver instance
    pub async fn new(
        commands_rx: mpsc::UnboundedReceiver<DriverCommand>,
        commands_tx: mpsc::UnboundedSender<DriverCommand>,
    ) -> Result<Self> {
        let config = Config::load().map_err(|e| {
            eprintln!("Failed to load configuration: {}", e);
            e
        })?;

        // Initialize logging
        crate::logging::init_logging(&config.logging)?;

        let _context = LogContext::new("driver").with_device_instance(config.device_instance);

        let logger = get_logger("driver");

        let (shutdown_tx, shutdown_rx) = mpsc::unbounded_channel();
        let (state_tx, _) = watch::channel(DriverState::Initializing);

        logger.info("Initializing EV charger driver");

        // Initialize persistence and load any saved state (best-effort)
        let mut persistence = PersistenceManager::new("/data/phaeton_state.json");
        let _ = persistence.load();

        // Initialize session manager and restore previous session state if available
        let mut sessions = ChargingSessionManager::default();
        if let Some(sess_state) = persistence.get_section("session") {
            let _ = sessions.restore_state(sess_state);
        }

        // Restore control states from persistence
        let mut current_mode = ChargingMode::Manual;
        if let Some(mode_val) = persistence.get::<u32>("mode") {
            current_mode = match mode_val {
                1 => ChargingMode::Auto,
                2 => ChargingMode::Scheduled,
                _ => ChargingMode::Manual,
            };
        }

        let mut start_stop = StartStopState::Stopped;
        if let Some(ss) = persistence.get::<u32>("start_stop") {
            start_stop = if ss == 1 {
                StartStopState::Enabled
            } else {
                StartStopState::Stopped
            };
        }

        let mut intended_set_current = 0.0f32;
        if let Some(cur) = persistence.get::<f32>("set_current") {
            intended_set_current = cur.max(0.0).min(config.controls.max_set_current);
        }

        // Create status broadcast channel
        let (status_tx, _status_rx) = broadcast::channel::<String>(100);

        // Create status snapshot channel (initialized with empty object)
        let initial_snapshot = Arc::new(DriverSnapshot {
            timestamp: chrono::Utc::now().to_rfc3339(),
            mode: 0,
            start_stop: 0,
            set_current: 0.0,
            applied_current: 0.0,
            station_max_current: 0.0,
            device_instance: config.device_instance,
            product_name: None,
            firmware: None,
            serial: None,
            status: 0,
            active_phases: 0,
            ac_power: 0.0,
            ac_current: 0.0,
            l1_voltage: 0.0,
            l2_voltage: 0.0,
            l3_voltage: 0.0,
            l1_current: 0.0,
            l2_current: 0.0,
            l3_current: 0.0,
            l1_power: 0.0,
            l2_power: 0.0,
            l3_power: 0.0,
            total_energy_kwh: 0.0,
            pricing_currency: None,
            energy_rate: None,
            session: serde_json::json!({}),
            poll_duration_ms: None,
            total_polls: 0,
            overrun_count: 0,
            poll_interval_ms: config.poll_interval_ms,
            excess_pv_power_w: 0.0,
        });
        let (status_snapshot_tx, status_snapshot_rx) =
            watch::channel::<Arc<DriverSnapshot>>(initial_snapshot);

        Ok(Self {
            config,
            state: state_tx,
            modbus_manager: None,
            logger,
            shutdown_tx,
            shutdown_rx,
            persistence,
            sessions,
            dbus: None,
            controls: ChargingControls::new(),
            current_mode,
            start_stop,
            intended_set_current,
            station_max_current: 32.0,
            last_sent_current: 0.0,
            last_current_set_time: std::time::Instant::now(),
            last_set_current_monotonic: std::time::Instant::now(),
            last_status: 0,
            pv_excess_ema_w: 0.0,
            last_nonzero_excess_at: std::time::Instant::now(),
            min_charge_timer_deadline: None,
            commands_rx,
            commands_tx,
            status_tx,
            status_snapshot_tx,
            status_snapshot_rx,
            last_l1_voltage: 0.0,
            last_l2_voltage: 0.0,
            last_l3_voltage: 0.0,
            last_l1_current: 0.0,
            last_l2_current: 0.0,
            last_l3_current: 0.0,
            last_l1_power: 0.0,
            last_l2_power: 0.0,
            last_l3_power: 0.0,
            last_total_power: 0.0,
            last_energy_kwh: 0.0,
            product_name: None,
            firmware_version: None,
            serial: None,
            total_polls: 0,
            overrun_count: 0,
            last_excess_pv_power_w: 0.0,
        })
    }

    /// Run the driver main loop
    pub async fn run(&mut self) -> Result<()> {
        self.logger.info("Starting EV charger driver main loop");

        // Initialize Modbus connection
        self.initialize_modbus().await?;

        // Update state to running
        self.state.send(DriverState::Running).ok();

        // Initialize control state from config defaults
        self.intended_set_current = self.config.defaults.intended_set_current;
        self.station_max_current = self.config.defaults.station_max_current;
        // Attempt to start D-Bus service only after we have identity values
        match self.try_start_dbus_with_identity().await {
            Ok(_) => {}
            Err(e) => {
                if self.config.require_dbus {
                    self.logger.error(&format!(
                        "Failed to initialize D-Bus and require_dbus=true: {}",
                        e
                    ));
                    return Err(e);
                } else {
                    self.logger.warn(&format!(
                        "D-Bus initialization failed but require_dbus=false, continuing without D-Bus: {}",
                        e
                    ));
                }
            }
        }

        // Main polling loop
        let mut poll_interval = interval(Duration::from_millis(self.config.poll_interval_ms));

        loop {
            tokio::select! {
                _ = poll_interval.tick() => {
                    let poll_started = std::time::Instant::now();
                    if let Err(e) = self.poll_cycle().await {
                        self.logger.error(&format!("Poll cycle failed: {}", e));
                        // Continue polling even on errors
                    }
                    let dur_ms = poll_started.elapsed().as_millis() as u64;
                    self.total_polls = self.total_polls.saturating_add(1);
                    if dur_ms > self.config.poll_interval_ms {
                        self.overrun_count = self.overrun_count.saturating_add(1);
                    }
                }
                Some(cmd) = self.commands_rx.recv() => {
                    self.handle_command(cmd).await;
                }
                _ = self.shutdown_rx.recv() => {
                    self.logger.info("Shutdown signal received");
                    break;
                }
            }
        }

        // Shutdown sequence
        self.state.send(DriverState::ShuttingDown).ok();
        self.shutdown().await?;

        Ok(())
    }

    /// Initialize Modbus connection
    async fn initialize_modbus(&mut self) -> Result<()> {
        let manager = ModbusConnectionManager::new(
            &self.config.modbus,
            self.config.controls.max_retries,
            Duration::from_secs_f64(self.config.controls.retry_delay),
        );

        self.modbus_manager = Some(manager);
        self.logger.info("Modbus connection manager initialized");
        Ok(())
    }

    /// Single polling cycle
    async fn poll_cycle(&mut self) -> Result<()> {
        self.logger.debug("Starting poll cycle");
        // Read measurements from Modbus
        if self.modbus_manager.is_some() {
            let socket_id = self.config.modbus.socket_slave_id;
            let addr_voltages = self.config.registers.voltages;
            let addr_currents = self.config.registers.currents;
            let addr_power = self.config.registers.power;
            let addr_energy = self.config.registers.energy;
            let addr_status = self.config.registers.status;
            let addr_amps = self.config.registers.amps_config;
            let station_id = self.config.modbus.station_slave_id;
            let addr_station_max = self.config.registers.station_max_current;

            // Read all required Modbus blocks within a limited mutable borrow scope
            let (l1_v, l2_v, l3_v, l1_i, l2_i, l3_i, l1_p, l2_p, l3_p, p_total, energy_kwh, status): (
                f64,
                f64,
                f64,
                f64,
                f64,
                f64,
                f64,
                f64,
                f64,
                f64,
                f64,
                i32,
            ) = {
                let manager = self.modbus_manager.as_mut().unwrap();

                // Voltages L1..L3 (6 registers -> 3 floats)
                let voltages = manager
                    .execute_with_reconnect(|client| {
                        Box::pin(async move {
                            client
                                .read_holding_registers(socket_id, addr_voltages, 6)
                                .await
                        })
                    })
                    .await
                    .ok();

                // Currents L1..L3 (6 registers -> 3 floats)
                let currents = manager
                    .execute_with_reconnect(|client| {
                        Box::pin(async move {
                            client
                                .read_holding_registers(socket_id, addr_currents, 6)
                                .await
                        })
                    })
                    .await
                    .ok();

                // Power block (8 registers -> 3 phases + total)
                let power_regs = manager
                    .execute_with_reconnect(|client| {
                        Box::pin(async move {
                            client
                                .read_holding_registers(socket_id, addr_power, 8)
                                .await
                        })
                    })
                    .await
                    .ok();

                // Energy (4 registers -> f64 Wh)
                let energy_regs = manager
                    .execute_with_reconnect(|client| {
                        Box::pin(async move {
                            client
                                .read_holding_registers(socket_id, addr_energy, 4)
                                .await
                        })
                    })
                    .await
                    .ok();

                // Socket status string (5 registers)
                let status_regs = manager
                    .execute_with_reconnect(|client| {
                        Box::pin(async move {
                            client
                                .read_holding_registers(socket_id, addr_status, 5)
                                .await
                        })
                    })
                    .await
                    .ok();

                // Station max current (optional refresh)
                if let Ok(max_regs) = manager
                    .execute_with_reconnect(|client| {
                        Box::pin(async move {
                            client
                                .read_holding_registers(station_id, addr_station_max, 2)
                                .await
                        })
                    })
                    .await
                    && max_regs.len() >= 2
                    && let Ok(max_c) = decode_32bit_float(&max_regs[0..2])
                    && max_c.is_finite()
                    && max_c > 0.0
                {
                    self.station_max_current = max_c;
                }

                // Decode values with safe fallbacks
                let (l1_v, l2_v, l3_v) = match voltages {
                    Some(v) if v.len() >= 6 => (
                        decode_32bit_float(&v[0..2]).unwrap_or(0.0) as f64,
                        decode_32bit_float(&v[2..4]).unwrap_or(0.0) as f64,
                        decode_32bit_float(&v[4..6]).unwrap_or(0.0) as f64,
                    ),
                    _ => (0.0, 0.0, 0.0),
                };

                let (l1_i, l2_i, l3_i) = match currents {
                    Some(v) if v.len() >= 6 => (
                        decode_32bit_float(&v[0..2]).unwrap_or(0.0) as f64,
                        decode_32bit_float(&v[2..4]).unwrap_or(0.0) as f64,
                        decode_32bit_float(&v[4..6]).unwrap_or(0.0) as f64,
                    ),
                    _ => (0.0, 0.0, 0.0),
                };

                let (mut l1_p, mut l2_p, mut l3_p, mut p_total) = match power_regs {
                    Some(v) if v.len() >= 8 => {
                        let p1 = decode_32bit_float(&v[0..2]).unwrap_or(0.0) as f64;
                        let p2 = decode_32bit_float(&v[2..4]).unwrap_or(0.0) as f64;
                        let p3 = decode_32bit_float(&v[4..6]).unwrap_or(0.0) as f64;
                        let pt = decode_32bit_float(&v[6..8]).unwrap_or(0.0) as f64;
                        let sanitize = |x: f64| if x.is_finite() { x } else { 0.0 };
                        (sanitize(p1), sanitize(p2), sanitize(p3), sanitize(pt))
                    }
                    _ => (0.0, 0.0, 0.0, 0.0),
                };

                // Fallback for chargers that report 0 for per-phase or total power: approximate using V*I
                let approx = |v: f64, i: f64| (v * i).round();
                if l1_p.abs() < 1.0 { l1_p = approx(l1_v, l1_i); }
                if l2_p.abs() < 1.0 { l2_p = approx(l2_v, l2_i); }
                if l3_p.abs() < 1.0 { l3_p = approx(l3_v, l3_i); }
                if p_total.abs() < 1.0 {
                    p_total = l1_p + l2_p + l3_p;
                }

                let energy_wh = match energy_regs {
                    Some(v) if v.len() >= 4 => decode_64bit_float(&v[0..4]).unwrap_or(0.0),
                    _ => 0.0,
                };
                let energy_kwh = energy_wh / 1000.0;

                let status_base = match status_regs {
                    Some(v) if v.len() >= 5 => {
                        let s = decode_string(&v[0..5], None).unwrap_or_default();
                        Self::map_alfen_status_to_victron(&s) as i32
                    }
                    _ => 0,
                };
                // Derive extended status: 4=WAIT_SUN, 6=WAIT_START, 7=LOW_SOC (approximate)
                let mut status = status_base;
                let connected = status_base == 1 || status_base == 2;
                if connected {
                    if matches!(self.start_stop, StartStopState::Stopped) {
                        status = 6; // WAIT_START
                    } else if matches!(self.current_mode, ChargingMode::Auto) {
                        // If Auto mode and very low effective/last current, WAIT_SUN
                        if self.last_sent_current < 0.1 {
                            status = 4; // WAIT_SUN
                        }
                    } else if matches!(self.current_mode, ChargingMode::Scheduled) {
                        // If not within schedule, WAIT_START
                        if !crate::controls::ChargingControls::is_schedule_active(&self.config) {
                            status = 6;
                        }
                    }
                }

                (l1_v, l2_v, l3_v, l1_i, l2_i, l3_i, l1_p, l2_p, l3_p, p_total, energy_kwh, status)
            };

            // Control logic: compute effective current and write via Modbus if needed
            let now_secs = (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default())
            .as_secs_f64();
            let requested = self.intended_set_current;

            // Calculate PV excess power from Victron system (AC+DC PV minus AC loads excluding EV charger itself)
            // Compensate for Victron vs charger measurement lag right after a set-current change
            // by estimating EV power from the last sent current for a brief window.
            let ev_power_for_subtract = {
                let lag_ms = self.config.controls.ev_reporting_lag_ms as u128;
                if self.last_set_current_monotonic.elapsed().as_millis() < lag_ms {
                    // Estimate: P = I_phase_max * V_phase * phases (use max of phase currents if available later)
                    // We only have the commanded current; assume 3 phases and 230V nominal.
                    let phases = 3.0f64; // TODO: detect
                    (self.last_sent_current as f64 * 230.0f64 * phases).max(0.0)
                } else {
                    p_total
                }
            };
            let excess_pv_power_w: f32 = self
                .calculate_excess_pv_power(ev_power_for_subtract)
                .await
                .unwrap_or(0.0);
            let effective: f32 = self
                .controls
                .compute_effective_current(
                    self.current_mode,
                    self.start_stop,
                    requested,
                    self.station_max_current,
                    now_secs,
                    Some(excess_pv_power_w),
                    &self.config,
                )
                .await
                .unwrap_or(0.0);

            // Enforce updating at least every current_update_interval ms
            let watchdog_satisfied = self.last_current_set_time.elapsed().as_millis()
                >= self.config.controls.current_update_interval as u128;
            let need_watchdog = watchdog_satisfied
                || self.last_current_set_time.elapsed().as_secs()
                    >= self.config.controls.watchdog_interval_seconds as u64;
            let need_change = (effective - self.last_sent_current).abs()
                > self.config.controls.update_difference_threshold;
            if need_watchdog || need_change {
                if need_change {
                    let reason = match self.current_mode {
                        ChargingMode::Manual => "manual",
                        ChargingMode::Auto => "pv_auto",
                        ChargingMode::Scheduled => "scheduled",
                    };
                    self.logger.info(&format!(
                        "Adjusting available current: {:.2} A -> {:.2} A (reason={}, pv_excess={:.0} W, station_max={:.1} A)",
                        self.last_sent_current, effective, reason, excess_pv_power_w, self.station_max_current
                    ));
                }
                let regs = crate::modbus::encode_32bit_float(effective);
                // Borrow modbus manager only for the write
                let write_res = self
                    .modbus_manager
                    .as_mut()
                    .unwrap()
                    .execute_with_reconnect(|client| {
                        let regs_vec = vec![regs[0], regs[1]];
                        Box::pin(async move {
                            client
                                .write_multiple_registers(socket_id, addr_amps, &regs_vec)
                                .await
                        })
                    })
                    .await;
                if write_res.is_ok() {
                    self.last_sent_current = effective;
                    self.last_current_set_time = std::time::Instant::now();
                    self.last_set_current_monotonic = std::time::Instant::now();
                } else {
                    self.logger.warn("Failed to write set current via Modbus");
                }
            }

            // Session start/end detection based on status transitions
            let prev_status = self.last_status;
            let cur_status = status as u8;
            if cur_status == 2 && prev_status != 2 && self.sessions.current_session.is_none() {
                let _ = self.sessions.start_session(energy_kwh);
            } else if cur_status != 2 && self.sessions.current_session.is_some() {
                // End current session
                if self.sessions.end_session(energy_kwh).is_ok() {
                    // Apply simple static pricing if configured
                    if self.config.pricing.source.to_lowercase() == "static"
                        && let Some(ref last) = self.sessions.last_session
                    {
                        let cost =
                            last.energy_delivered_kwh * self.config.pricing.static_rate_eur_per_kwh;
                        self.sessions.set_cost_on_last_session(cost);
                    }
                }
            }
            self.last_status = cur_status;

            // Update session metrics on each poll
            self.sessions.update(p_total, energy_kwh)?;

            // Persist minimal state snapshot (best-effort)
            self.persistence.set_mode(self.current_mode as u32);
            self.persistence.set_start_stop(self.start_stop as u32);
            self.persistence.set_set_current(self.intended_set_current);
            // store full session state snapshot
            let _ = self
                .persistence
                .set_section("session", self.sessions.get_state());
            let _ = self.persistence.save();

            // D-Bus export moved to a dedicated task (exporter) that consumes snapshots

            // Store last measured values for snapshot
            self.last_l1_voltage = l1_v;
            self.last_l2_voltage = l2_v;
            self.last_l3_voltage = l3_v;
            self.last_l1_current = l1_i;
            self.last_l2_current = l2_i;
            self.last_l3_current = l3_i;
            self.last_l1_power = l1_p;
            self.last_l2_power = l2_p;
            self.last_l3_power = l3_p;
            self.last_total_power = p_total;
            self.last_energy_kwh = energy_kwh;

            // Log a concise summary
            self.logger.debug(&format!(
                "V=({:.1},{:.1},{:.1})V I=({:.2},{:.2},{:.2})A P=({:.0},{:.0},{:.0})W total={:.0}W E={:.3}kWh status={} lag_ms={} last_sent_A={:.2}",
                l1_v, l2_v, l3_v, l1_i, l2_i, l3_i, l1_p, l2_p, l3_p, p_total, energy_kwh, status,
                self.last_set_current_monotonic.elapsed().as_millis(), self.last_sent_current
            ));

            // Publish status snapshot for SSE consumers
            let mut status_obj = serde_json::json!({
                "mode": self.current_mode_code(),
                "start_stop": self.start_stop_code(),
                "set_current": self.get_intended_set_current(),
                "applied_current": effective,
                "station_max_current": self.get_station_max_current(),
                "ac_power": p_total,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            });
            // Include session energy if available
            if let Some(v) = self
                .sessions
                .get_session_stats()
                .get("energy_delivered_kwh")
                .and_then(|v| v.as_f64())
            {
                status_obj["energy_forward_kwh"] = serde_json::json!(v);
            }
            let _ = self.status_tx.send(status_obj.to_string());
        }

        self.logger.debug("Poll cycle completed");

        // Publish a status snapshot for consumers (web, etc.)
        let snapshot = Arc::new(self.build_typed_snapshot(Some(self.last_poll_duration_ms())));
        let _ = self.status_snapshot_tx.send(snapshot);
        Ok(())
    }

    /// Shutdown the driver
    async fn shutdown(&mut self) -> Result<()> {
        self.logger.info("Shutting down driver");

        if let Some(_manager) = self.modbus_manager.take() {
            // Disconnect Modbus
            // TODO: Implement proper disconnect
        }

        self.logger.info("Driver shutdown complete");
        Ok(())
    }

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

    pub fn get_db_value(&self, path: &str) -> Option<serde_json::Value> {
        self.dbus.as_ref().and_then(|d| d.get(path))
    }

    /// Snapshot of cached D-Bus paths (subset of known keys)
    pub fn get_dbus_cache_snapshot(&self) -> serde_json::Value {
        let mut root = serde_json::Map::new();
        for key in [
            "/DeviceInstance",
            "/ProductName",
            "/FirmwareVersion",
            "/Serial",
            "/Ac/Power",
            "/Ac/Energy/Forward",
            "/Ac/Current",
            "/Ac/PhaseCount",
            "/Status",
            "/Mode",
            "/StartStop",
            "/SetCurrent",
        ] {
            if let Some(v) = self.get_db_value(key) {
                root.insert(key.to_string(), v);
            }
        }
        serde_json::Value::Object(root)
    }

    /// Get sessions data
    pub fn sessions_snapshot(&self) -> serde_json::Value {
        self.sessions.get_state()
    }

    /// Subscribe to status updates (for SSE)
    pub fn subscribe_status(&self) -> broadcast::Receiver<String> {
        self.status_tx.subscribe()
    }
}

impl AlfenDriver {
    /// Compute excess PV power (W) using Victron D-Bus system values:
    /// total_pv = Dc/Pv/Power + sum(Ac/PvOnOutput/L{1,2,3}/Power)
    /// consumption = sum(Ac/Consumption/L{1,2,3}/Power)
    /// excess = max(0, total_pv - (consumption - ev_power))
    async fn calculate_excess_pv_power(&self, ev_power_w: f64) -> Option<f32> {
        let dbus = self.dbus.as_ref()?;
        // Helper to read f64, defaulting to 0
        async fn get_f64(svc: &crate::dbus::DbusService, path: &str) -> f64 {
            match svc
                .read_remote_value("com.victronenergy.system", path)
                .await
            {
                Ok(v) => v
                    .as_f64()
                    .or_else(|| v.as_i64().map(|x| x as f64))
                    .or_else(|| v.as_u64().map(|x| x as f64))
                    .unwrap_or(0.0),
                Err(_) => 0.0,
            }
        }

        let dc_pv = get_f64(dbus, "/Dc/Pv/Power").await;
        let ac_pv_l1 = get_f64(dbus, "/Ac/PvOnOutput/L1/Power").await;
        let ac_pv_l2 = get_f64(dbus, "/Ac/PvOnOutput/L2/Power").await;
        let ac_pv_l3 = get_f64(dbus, "/Ac/PvOnOutput/L3/Power").await;
        let total_pv = dc_pv + ac_pv_l1 + ac_pv_l2 + ac_pv_l3;

        let cons_l1 = get_f64(dbus, "/Ac/Consumption/L1/Power").await;
        let cons_l2 = get_f64(dbus, "/Ac/Consumption/L2/Power").await;
        let cons_l3 = get_f64(dbus, "/Ac/Consumption/L3/Power").await;
        let consumption = cons_l1 + cons_l2 + cons_l3;

        // Subtract EV charger itself from consumption
        let adjusted_consumption = (consumption - ev_power_w).max(0.0);
        let excess = (total_pv - adjusted_consumption).max(0.0);
        Some(excess as f32)
    }

    /// Compute excess PV power (W) using a provided D-Bus handle, avoiding any driver locks.
    async fn calculate_excess_pv_power_with_dbus(
        dbus: &crate::dbus::DbusService,
        ev_power_w: f64,
    ) -> Option<f32> {
        // Helper to read f64, defaulting to 0
        async fn get_f64(svc: &crate::dbus::DbusService, path: &str) -> f64 {
            match svc
                .read_remote_value("com.victronenergy.system", path)
                .await
            {
                Ok(v) => v
                    .as_f64()
                    .or_else(|| v.as_i64().map(|x| x as f64))
                    .or_else(|| v.as_u64().map(|x| x as f64))
                    .unwrap_or(0.0),
                Err(_) => 0.0,
            }
        }

        let dc_pv = get_f64(dbus, "/Dc/Pv/Power").await;
        let ac_pv_l1 = get_f64(dbus, "/Ac/PvOnOutput/L1/Power").await;
        let ac_pv_l2 = get_f64(dbus, "/Ac/PvOnOutput/L2/Power").await;
        let ac_pv_l3 = get_f64(dbus, "/Ac/PvOnOutput/L3/Power").await;
        let total_pv = dc_pv + ac_pv_l1 + ac_pv_l2 + ac_pv_l3;

        let cons_l1 = get_f64(dbus, "/Ac/Consumption/L1/Power").await;
        let cons_l2 = get_f64(dbus, "/Ac/Consumption/L2/Power").await;
        let cons_l3 = get_f64(dbus, "/Ac/Consumption/L3/Power").await;
        let consumption = cons_l1 + cons_l2 + cons_l3;

        // Subtract EV charger itself from consumption
        let adjusted_consumption = (consumption - ev_power_w).max(0.0);
        let excess = (total_pv - adjusted_consumption).max(0.0);
        Some(excess as f32)
    }
}

impl AlfenDriver {
    /// Build a consolidated status snapshot for API/consumers (legacy JSON builder; kept for reference but unused)
    #[allow(dead_code)]
    fn build_status_snapshot_value(&self) -> serde_json::Value {
        let mut root = serde_json::json!({
            "mode": self.current_mode_code(),
            "start_stop": self.start_stop_code(),
            "set_current": self.get_intended_set_current(),
            "station_max_current": self.get_station_max_current(),
            "device_instance": self.config().device_instance,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });

        // Identity (prefer cached identity; fall back to DBus cache)
        if let Some(p) = self.product_name.as_ref() {
            root["product_name"] = serde_json::json!(p);
        } else if let Some(v) = self.get_db_value("/ProductName") {
            root["product_name"] = v;
        }
        if let Some(s) = self.serial.as_ref() {
            root["serial"] = serde_json::json!(s);
        } else if let Some(v) = self.get_db_value("/Serial") {
            root["serial"] = v;
        }
        if let Some(fw) = self.firmware_version.as_ref() {
            root["firmware"] = serde_json::json!(fw);
        } else if let Some(v) = self.get_db_value("/FirmwareVersion") {
            root["firmware"] = v;
        }

        // Status & phases
        root["status"] = serde_json::json!(self.last_status as u32);
        let phase_count = [
            self.last_l1_current,
            self.last_l2_current,
            self.last_l3_current,
        ]
        .iter()
        .filter(|v| v.is_finite() && v.abs() > 0.01)
        .count();
        root["active_phases"] = serde_json::json!(phase_count);

        // Power & currents from last measured values to avoid DBus dependency
        root["ac_power"] = serde_json::json!(self.last_total_power);
        let max_phase_current = self
            .last_l1_current
            .max(self.last_l2_current.max(self.last_l3_current));
        root["ac_current"] = serde_json::json!(max_phase_current);
        root["l1_voltage"] = serde_json::json!(self.last_l1_voltage);
        root["l2_voltage"] = serde_json::json!(self.last_l2_voltage);
        root["l3_voltage"] = serde_json::json!(self.last_l3_voltage);
        root["l1_current"] = serde_json::json!(self.last_l1_current);
        root["l2_current"] = serde_json::json!(self.last_l2_current);
        root["l3_current"] = serde_json::json!(self.last_l3_current);
        root["l1_power"] = serde_json::json!(self.last_l1_power);
        root["l2_power"] = serde_json::json!(self.last_l2_power);
        root["l3_power"] = serde_json::json!(self.last_l3_power);

        // Session
        let mut session = serde_json::json!({});
        // Charging time based on session stats (no DBus dependency)
        let stats = self.sessions.get_session_stats();
        let charging_time_sec = stats
            .get("session_duration_min")
            .and_then(|v| v.as_f64())
            .map(|m| (m * 60.0).round() as i64)
            .unwrap_or(0);
        session["charging_time_sec"] = serde_json::json!(charging_time_sec);
        if let Some(v) = self.get_db_value("/Ac/Energy/Forward") {
            session["energy_delivered_kwh"] = v;
        }
        let sessions_state = self.sessions_snapshot();
        if let Some(obj) = sessions_state.as_object() {
            if let Some(cur) = obj.get("current_session").and_then(|v| v.as_object()) {
                if let Some(ts) = cur.get("start_time") {
                    session["start_ts"] = ts.clone();
                }
                if let Some(v) = cur.get("energy_delivered_kwh") {
                    session["energy_delivered_kwh"] = v.clone();
                }
            }
            if let Some(last) = obj.get("last_session").and_then(|v| v.as_object()) {
                if session.get("start_ts").is_none()
                    && let Some(ts) = last.get("start_time")
                {
                    session["start_ts"] = ts.clone();
                }
                if let Some(ts) = last.get("end_time") {
                    session["end_ts"] = ts.clone();
                }
                if session.get("energy_delivered_kwh").is_none()
                    && let Some(v) = last.get("energy_delivered_kwh")
                {
                    session["energy_delivered_kwh"] = v.clone();
                }
                if let Some(v) = last.get("cost") {
                    session["cost"] = v.clone();
                }
            }
        }

        // Pricing
        let pricing = &self.config().pricing;
        if !pricing.currency_symbol.is_empty() {
            root["pricing_currency"] = serde_json::json!(pricing.currency_symbol.clone());
        }
        if pricing.source.to_lowercase() == "static" {
            root["energy_rate"] = serde_json::json!(pricing.static_rate_eur_per_kwh);
        }

        root["total_energy_kwh"] = serde_json::json!(self.last_energy_kwh);

        root["session"] = session;

        root
    }

    /// Subscribe to snapshots
    pub fn subscribe_snapshot(&self) -> watch::Receiver<Arc<DriverSnapshot>> {
        self.status_snapshot_rx.clone()
    }

    /// Build typed snapshot struct; optional poll duration to include metrics
    fn build_typed_snapshot(&self, poll_duration_ms: Option<u64>) -> DriverSnapshot {
        let phase_count = [
            self.last_l1_current,
            self.last_l2_current,
            self.last_l3_current,
        ]
        .iter()
        .filter(|v| v.is_finite() && v.abs() > 0.01)
        .count() as u8;
        let ac_current = self
            .last_l1_current
            .max(self.last_l2_current.max(self.last_l3_current));
        let pricing_currency =
            Some(self.config().pricing.currency_symbol.clone()).filter(|sym| !sym.is_empty());
        let energy_rate = if self.config().pricing.source.to_lowercase() == "static" {
            Some(self.config().pricing.static_rate_eur_per_kwh)
        } else {
            None
        };
        let session = {
            let mut s = serde_json::json!({});
            // Prefer exact seconds derived from session start/end times
            let charging_time_sec: i64 = if let Some(cur) = self.sessions.current_session.as_ref() {
                (chrono::Utc::now() - cur.start_time).num_seconds().max(0)
            } else if let Some(last) = self.sessions.last_session.as_ref() {
                if let Some(end) = last.end_time {
                    (end - last.start_time).num_seconds().max(0)
                } else {
                    0
                }
            } else {
                0
            };
            s["charging_time_sec"] = serde_json::json!(charging_time_sec);
            let sessions_state = self.sessions_snapshot();
            if let Some(obj) = sessions_state.as_object() {
                if let Some(cur) = obj.get("current_session").and_then(|v| v.as_object()) {
                    if let Some(ts) = cur.get("start_time") {
                        s["start_ts"] = ts.clone();
                    }
                    if let Some(v) = cur.get("energy_delivered_kwh") {
                        s["energy_delivered_kwh"] = v.clone();
                    }
                }
                if let Some(last) = obj.get("last_session").and_then(|v| v.as_object()) {
                    if s.get("start_ts").is_none()
                        && let Some(ts) = last.get("start_time")
                    {
                        s["start_ts"] = ts.clone();
                    }
                    if let Some(ts) = last.get("end_time") {
                        s["end_ts"] = ts.clone();
                    }
                    if s.get("energy_delivered_kwh").is_none()
                        && let Some(v) = last.get("energy_delivered_kwh")
                    {
                        s["energy_delivered_kwh"] = v.clone();
                    }
                    if let Some(v) = last.get("cost") {
                        s["cost"] = v.clone();
                    }
                }
            }
            s
        };

        DriverSnapshot {
            timestamp: chrono::Utc::now().to_rfc3339(),
            mode: self.current_mode_code(),
            start_stop: self.start_stop_code(),
            set_current: self.get_intended_set_current(),
            applied_current: self.last_sent_current,
            station_max_current: self.get_station_max_current(),
            device_instance: self.config().device_instance,
            product_name: self.product_name.clone(),
            firmware: self.firmware_version.clone(),
            serial: self.serial.clone(),
            status: self.last_status as u32,
            active_phases: phase_count,
            ac_power: self.last_total_power,
            ac_current,
            l1_voltage: self.last_l1_voltage,
            l2_voltage: self.last_l2_voltage,
            l3_voltage: self.last_l3_voltage,
            l1_current: self.last_l1_current,
            l2_current: self.last_l2_current,
            l3_current: self.last_l3_current,
            l1_power: self.last_l1_power,
            l2_power: self.last_l2_power,
            l3_power: self.last_l3_power,
            total_energy_kwh: self.last_energy_kwh,
            pricing_currency,
            energy_rate,
            session,
            poll_duration_ms,
            total_polls: self.total_polls,
            overrun_count: self.overrun_count,
            poll_interval_ms: self.config.poll_interval_ms,
            excess_pv_power_w: self.last_excess_pv_power_w,
        }
    }
}
impl AlfenDriver {
    /// One-shot read of charger identity values via Modbus and publish to D-Bus, with logs
    async fn refresh_charger_identity(&mut self) -> Result<()> {
        if self.modbus_manager.is_none() || self.dbus.is_none() {
            return Ok(());
        }

        let manager = self.modbus_manager.as_mut().unwrap();

        // Manufacturer (string)
        let manufacturer = manager
            .execute_with_reconnect(|client| {
                let id = self.config.modbus.station_slave_id;
                let addr = self.config.registers.manufacturer;
                let cnt = self.config.registers.manufacturer_count;
                Box::pin(async move { client.read_holding_registers(id, addr, cnt).await })
            })
            .await
            .ok()
            .map(|regs| decode_string(&regs, None).unwrap_or_default())
            .unwrap_or_default();

        // Firmware version (string)
        let firmware = manager
            .execute_with_reconnect(|client| {
                let id = self.config.modbus.station_slave_id;
                let addr = self.config.registers.firmware_version;
                let cnt = self.config.registers.firmware_version_count;
                Box::pin(async move { client.read_holding_registers(id, addr, cnt).await })
            })
            .await
            .ok()
            .map(|regs| decode_string(&regs, None).unwrap_or_default())
            .unwrap_or_default();

        // Serial number (string)
        let serial = manager
            .execute_with_reconnect(|client| {
                let id = self.config.modbus.station_slave_id;
                let addr = self.config.registers.station_serial;
                let cnt = self.config.registers.station_serial_count;
                Box::pin(async move { client.read_holding_registers(id, addr, cnt).await })
            })
            .await
            .ok()
            .map(|regs| decode_string(&regs, None).unwrap_or_default())
            .unwrap_or_default();

        // Publish to D-Bus and log
        if let Some(dbus) = &mut self.dbus {
            let mut updates: Vec<(String, serde_json::Value)> = Vec::with_capacity(3);
            // Align ProductName with Python implementation: include manufacturer when available
            if !manufacturer.is_empty() {
                let pname = format!("{} EV Charger", manufacturer);
                updates.push(("/ProductName".to_string(), serde_json::json!(pname)));
            }
            if !firmware.is_empty() {
                updates.push((
                    "/FirmwareVersion".to_string(),
                    serde_json::json!(firmware.clone()),
                ));
            }
            if !serial.is_empty() {
                updates.push(("/Serial".to_string(), serde_json::json!(serial.clone())));
            }
            if !updates.is_empty() {
                let product_name = if !manufacturer.is_empty() {
                    format!("{} EV Charger", manufacturer)
                } else {
                    "Alfen EV Charger".to_string()
                };
                self.logger.info(&format!(
                    "Publishing charger identity: product_name='{}', firmware='{}', serial='{}'",
                    product_name, firmware, serial
                ));
                let _ = dbus.update_paths(updates).await;
            } else {
                self.logger
                    .warn("Charger identity not available via Modbus; leaving defaults");
            }
        }

        // Cache identity for snapshots
        if !manufacturer.is_empty() {
            self.product_name = Some(format!("{} EV Charger", manufacturer));
        }
        if !firmware.is_empty() {
            self.firmware_version = Some(firmware.clone());
        }
        if !serial.is_empty() {
            self.serial = Some(serial.clone());
        }

        Ok(())
    }

    /// Run the driver using a shared Arc<Mutex<AlfenDriver>> without holding the lock across the entire loop.
    ///
    /// This avoids starving other components (e.g., web server) that need brief access to the driver state.
    pub async fn run_on_arc(driver: Arc<tokio::sync::Mutex<AlfenDriver>>) -> Result<()> {
        // Initialization phase (performed under the lock, but only briefly)
        {
            let mut drv = driver.lock().await;
            drv.logger.info("Starting EV charger driver main loop");
            drv.initialize_modbus().await?;
            drv.state.send(DriverState::Running).ok();
            drv.intended_set_current = drv.config.defaults.intended_set_current;
            drv.station_max_current = drv.config.defaults.station_max_current;
        }

        // Start D-Bus (performed separately to keep lock scopes small)
        {
            let mut drv = driver.lock().await;
            match drv.try_start_dbus_with_identity().await {
                Ok(_) => {}
                Err(e) => {
                    if drv.config.require_dbus {
                        drv.logger.error(&format!(
                            "Failed to initialize D-Bus and require_dbus=true: {}",
                            e
                        ));
                        return Err(e);
                    } else {
                        drv.logger.warn(&format!(
                            "D-Bus initialization failed but require_dbus=false, continuing without D-Bus: {}",
                            e
                        ));
                    }
                }
            }
        }

        // Determine poll interval from config (read once under lock)
        let poll_ms = {
            let drv = driver.lock().await;
            drv.config.poll_interval_ms
        };

        // Channels for worker coordination
        let (meas_tx, mut meas_rx) = mpsc::unbounded_channel::<Measurements>();
        let (mb_tx, mb_rx) = mpsc::unbounded_channel::<ModbusCommand>();
        // Stop signal for Modbus worker
        let (mb_stop_tx, mb_stop_rx) = watch::channel(false);

        // Spawn Modbus worker on a dedicated single-thread Tokio runtime
        let modbus_handle = {
            let driver_for_task = driver.clone();
            let meas_tx_clone = meas_tx.clone();
            let mb_stop_rx = mb_stop_rx;
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("modbus runtime");
                rt.block_on(async move {
                    let cfg = {
                        let drv = driver_for_task.lock().await;
                        drv.config.clone()
                    };
                    let mut manager = ModbusConnectionManager::new(
                        &cfg.modbus,
                        cfg.controls.max_retries,
                        Duration::from_secs_f64(cfg.controls.retry_delay),
                    );
                    let mut ticker = interval(Duration::from_millis(poll_ms));
                    let mut cmd_rx = mb_rx;
                    let mut stop_rx = mb_stop_rx;
                    loop {
                        tokio::select! {
                            _ = ticker.tick() => {
                                let poll_started = std::time::Instant::now();
                                let socket_id = cfg.modbus.socket_slave_id;
                                let addr_voltages = cfg.registers.voltages;
                                let addr_currents = cfg.registers.currents;
                                let addr_power = cfg.registers.power;
                                let addr_energy = cfg.registers.energy;
                                let addr_status = cfg.registers.status;
                                let station_id = cfg.modbus.station_slave_id;
                                let addr_station_max = cfg.registers.station_max_current;

                                let voltages = manager.execute_with_reconnect(|client| {
                                    Box::pin(async move { client.read_holding_registers(socket_id, addr_voltages, 6).await })
                                }).await.ok();
                                let currents = manager.execute_with_reconnect(|client| {
                                    Box::pin(async move { client.read_holding_registers(socket_id, addr_currents, 6).await })
                                }).await.ok();
                                let power_regs = manager.execute_with_reconnect(|client| {
                                    Box::pin(async move { client.read_holding_registers(socket_id, addr_power, 8).await })
                                }).await.ok();
                                let energy_regs = manager.execute_with_reconnect(|client| {
                                    Box::pin(async move { client.read_holding_registers(socket_id, addr_energy, 4).await })
                                }).await.ok();
                                let status_regs = manager.execute_with_reconnect(|client| {
                                    Box::pin(async move { client.read_holding_registers(socket_id, addr_status, 5).await })
                                }).await.ok();
                                if let Ok(max_regs) = manager.execute_with_reconnect(|client| {
                                    Box::pin(async move { client.read_holding_registers(station_id, addr_station_max, 2).await })
                                }).await
                                    && max_regs.len() >= 2
                                    && let Ok(max_c) = decode_32bit_float(&max_regs[0..2])
                                    && max_c.is_finite() && max_c > 0.0
                                {
                                    let mut drv = driver_for_task.lock().await;
                                    drv.station_max_current = max_c;
                                }

                                let (l1_v, l2_v, l3_v) = match voltages { Some(v) if v.len()>=6 => (
                                    decode_32bit_float(&v[0..2]).unwrap_or(0.0) as f64,
                                    decode_32bit_float(&v[2..4]).unwrap_or(0.0) as f64,
                                    decode_32bit_float(&v[4..6]).unwrap_or(0.0) as f64,
                                ), _ => (0.0,0.0,0.0)};
                                let (l1_i, l2_i, l3_i) = match currents { Some(v) if v.len()>=6 => (
                                    decode_32bit_float(&v[0..2]).unwrap_or(0.0) as f64,
                                    decode_32bit_float(&v[2..4]).unwrap_or(0.0) as f64,
                                    decode_32bit_float(&v[4..6]).unwrap_or(0.0) as f64,
                                ), _ => (0.0,0.0,0.0)};
                                let (mut l1_p, mut l2_p, mut l3_p, mut p_total) = match power_regs { Some(v) if v.len()>=8 => {
                                    let p1=decode_32bit_float(&v[0..2]).unwrap_or(0.0) as f64;
                                    let p2=decode_32bit_float(&v[2..4]).unwrap_or(0.0) as f64;
                                    let p3=decode_32bit_float(&v[4..6]).unwrap_or(0.0) as f64;
                                    let pt=decode_32bit_float(&v[6..8]).unwrap_or(0.0) as f64;
                                    let s=|x:f64| if x.is_finite(){x}else{0.0};
                                    (s(p1),s(p2),s(p3),s(pt))
                                }, _ => (0.0,0.0,0.0,0.0)};
                                // Fallback for chargers that report 0 for per-phase or total power: approximate using V*I
                                let approx = |v: f64, i: f64| (v * i).round();
                                if l1_p.abs() < 1.0 { l1_p = approx(l1_v, l1_i); }
                                if l2_p.abs() < 1.0 { l2_p = approx(l2_v, l2_i); }
                                if l3_p.abs() < 1.0 { l3_p = approx(l3_v, l3_i); }
                                if p_total.abs() < 1.0 {
                                    p_total = l1_p + l2_p + l3_p;
                                }
                                let energy_kwh = match energy_regs { Some(v) if v.len()>=4 => decode_64bit_float(&v[0..4]).unwrap_or(0.0)/1000.0, _ => 0.0 };
                                let status_base = match status_regs { Some(v) if v.len()>=5 => {
                                    let s = decode_string(&v[0..5], None).unwrap_or_default();
                                    AlfenDriver::map_alfen_status_to_victron(&s) as i32
                                }, _ => 0 };

                                let dur_ms = poll_started.elapsed().as_millis() as u64;
                                let overran = dur_ms > cfg.poll_interval_ms;
                                let _ = meas_tx_clone.send(Measurements { l1_v, l2_v, l3_v, l1_i, l2_i, l3_i, l1_p, l2_p, l3_p, p_total, energy_kwh, status_base, duration_ms: dur_ms, overran });
                            }
                            cmd = cmd_rx.recv() => {
                                match cmd {
                                    Some(ModbusCommand::WriteSetCurrent(effective)) => {
                                        let regs = crate::modbus::encode_32bit_float(effective);
                                        let v0 = regs[0];
                                        let v1 = regs[1];
                                        let sid = cfg.modbus.socket_slave_id;
                                        let addr = cfg.registers.amps_config;
                                        let _ = manager.execute_with_reconnect(move |client| {
                                            Box::pin(async move {
                                                let values = vec![v0, v1];
                                                client.write_multiple_registers(sid, addr, &values).await
                                            })
                                        }).await;
                                    }
                                    None => break,
                                }
                            }
                            _ = stop_rx.changed() => {
                                break;
                            }
                        }
                    }
                });
            })
        };

        // Control task: consumes measurements, applies logic, updates state and snapshot, sends set current
        {
            let driver_for_task = driver.clone();
            let mb_tx_clone = mb_tx.clone();
            tokio::spawn(async move {
                while let Some(m) = meas_rx.recv().await {
                    let mut drv = driver_for_task.lock().await;
                    // Process any pending external commands quickly
                    loop {
                        match drv.commands_rx.try_recv() {
                            Ok(cmd) => drv.handle_command(cmd).await,
                            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => break,
                        }
                    }

                    // Update measured values
                    drv.last_l1_voltage = m.l1_v;
                    drv.last_l2_voltage = m.l2_v;
                    drv.last_l3_voltage = m.l3_v;
                    drv.last_l1_current = m.l1_i;
                    drv.last_l2_current = m.l2_i;
                    drv.last_l3_current = m.l3_i;
                    drv.last_l1_power = m.l1_p;
                    drv.last_l2_power = m.l2_p;
                    drv.last_l3_power = m.l3_p;
                    drv.last_total_power = m.p_total;
                    drv.last_energy_kwh = m.energy_kwh;

                    // Derive extended status & session transitions
                    let mut status = m.status_base;
                    let connected = status == 1 || status == 2;
                    if connected {
                        if matches!(drv.start_stop, StartStopState::Stopped) {
                            status = 6;
                        } else if matches!(drv.current_mode, ChargingMode::Auto) {
                            if drv.last_sent_current < 0.1 {
                                status = 4;
                            }
                        } else if matches!(drv.current_mode, ChargingMode::Scheduled)
                            && !crate::controls::ChargingControls::is_schedule_active(&drv.config)
                        {
                            status = 6;
                        }
                    }
                    let prev_status = drv.last_status;
                    let cur_status = status as u8;
                    if cur_status == 2 && prev_status != 2 && drv.sessions.current_session.is_none()
                    {
                        let _ = drv.sessions.start_session(m.energy_kwh);
                    } else if cur_status != 2
                        && drv.sessions.current_session.is_some()
                        && drv.sessions.end_session(m.energy_kwh).is_ok()
                        && drv.config.pricing.source.to_lowercase() == "static"
                        && let Some(ref last) = drv.sessions.last_session
                    {
                        let cost =
                            last.energy_delivered_kwh * drv.config.pricing.static_rate_eur_per_kwh;
                        drv.sessions.set_cost_on_last_session(cost);
                    }
                    drv.last_status = cur_status;

                    // Control logic (no await while locked)
                    let now_secs = (std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default())
                    .as_secs_f64();
                    let requested = drv.intended_set_current;
                    // Use PV reader's last computed value (it applies lag compensation)
                    let excess_pv_power_w: f32 = drv.last_excess_pv_power_w;
                    // Implement minimum-charge grace: if Auto and pv excess converts < 6A,
                    // hold 6A for a configurable time before dropping to 0.
                    let mut effective: f32 = drv
                        .controls
                        .blocking_compute_effective_current(
                            drv.current_mode,
                            drv.start_stop,
                            requested,
                            drv.station_max_current,
                            now_secs,
                            Some(excess_pv_power_w),
                            &drv.config,
                        )
                        .unwrap_or(0.0);

                    if matches!(drv.current_mode, ChargingMode::Auto)
                        && matches!(drv.start_stop, StartStopState::Enabled)
                    {
                        // Convert 6A threshold to watts with same assumptions (3x230V)
                        let six_a_watts = 6.0f32 * 230.0f32 * 3.0f32;
                        let have_min = excess_pv_power_w >= six_a_watts - 200.0; // small tolerance
                        let now = std::time::Instant::now();

                        if have_min {
                            // Enough PV: clear timer and compute normally
                            drv.min_charge_timer_deadline = None;
                        } else {
                            // Not enough PV: start or maintain timer
                            let duration = std::time::Duration::from_secs(
                                drv.config.controls.min_charge_duration_seconds as u64,
                            );
                            if drv.min_charge_timer_deadline.is_none() {
                                drv.min_charge_timer_deadline = Some(now + duration);
                                drv.logger
                                    .info("PV below 6A; starting minimum-charge countdown");
                            }
                        }

                        // While timer active, clamp effective to 6A (not below)
                        if let Some(deadline) = drv.min_charge_timer_deadline {
                            if deadline > std::time::Instant::now() {
                                if effective < 6.0 {
                                    effective = 6.0;
                                }
                            } else {
                                // Timer expired → stop charging (set to 0 A)
                                effective = 0.0;
                                drv.min_charge_timer_deadline = None;
                                drv.logger
                                    .info("Minimum-charge countdown expired; stopping charging");
                            }
                        }
                    }

                    // Enforce updating at least every current_update_interval ms to keep charger from fallback
                    let watchdog_satisfied = drv.last_current_set_time.elapsed().as_millis()
                        >= drv.config.controls.current_update_interval as u128;
                    let need_watchdog = watchdog_satisfied
                        || drv.last_current_set_time.elapsed().as_secs()
                            >= drv.config.controls.watchdog_interval_seconds as u64;
                    let need_change = (effective - drv.last_sent_current).abs()
                        > drv.config.controls.update_difference_threshold;
                    if need_watchdog || need_change {
                        if need_change {
                            let reason = match drv.current_mode {
                                ChargingMode::Manual => "manual",
                                ChargingMode::Auto => "pv_auto",
                                ChargingMode::Scheduled => "scheduled",
                            };
                            drv.logger.info(&format!(
                                "Adjusting available current: {:.2} A -> {:.2} A (reason={}, pv_excess={:.0} W, station_max={:.1} A)",
                                drv.last_sent_current, effective, reason, excess_pv_power_w, drv.station_max_current
                            ));
                        }
                        let _ = mb_tx_clone.send(ModbusCommand::WriteSetCurrent(effective));
                        drv.last_sent_current = effective;
                        drv.last_current_set_time = std::time::Instant::now();
                        drv.last_set_current_monotonic = std::time::Instant::now();
                    }

                    // Update sessions and persistence (best-effort)
                    let _ = drv.sessions.update(m.p_total, m.energy_kwh);
                    let sessions_state = drv.sessions.get_state();
                    let current_mode_u32 = drv.current_mode as u32;
                    let start_stop_u32 = drv.start_stop as u32;
                    let intended = drv.intended_set_current;
                    // Drop mutable borrow before persistence calls
                    drop(drv);
                    {
                        let mut drv2 = driver_for_task.lock().await;
                        drv2.persistence.set_mode(current_mode_u32);
                        drv2.persistence.set_start_stop(start_stop_u32);
                        drv2.persistence.set_set_current(intended);
                        let _ = drv2.persistence.set_section("session", sessions_state);
                        let _ = drv2.persistence.save();
                    }

                    // Update metrics and publish snapshot
                    {
                        let mut drv3 = driver_for_task.lock().await;
                        drv3.total_polls = drv3.total_polls.saturating_add(1);
                        if m.overran {
                            drv3.overrun_count = drv3.overrun_count.saturating_add(1);
                        }
                        let snapshot = Arc::new(drv3.build_typed_snapshot(Some(m.duration_ms)));
                        let _ = drv3.status_snapshot_tx.send(snapshot);
                    }
                }
            });
        }

        // Start D-Bus exporter task to mirror snapshots to D-Bus without blocking the driver loop
        let exporter_handle = {
            let driver_for_task = driver.clone();
            tokio::spawn(async move {
                // Obtain snapshot receiver once; it clones internally for borrows
                let mut rx = {
                    let drv = driver_for_task.lock().await;
                    drv.subscribe_snapshot()
                };
                loop {
                    // Clone the current Arc snapshot BEFORE any await to avoid holding a non-Send borrow
                    let current_snapshot: Arc<DriverSnapshot> = rx.borrow().clone();
                    // Convert snapshot outside of any driver locks
                    let value =
                        serde_json::to_value(&*current_snapshot).unwrap_or(serde_json::json!({}));
                    // Export without holding the driver lock: temporarily take the dbus handle
                    let mut taken_dbus = {
                        let mut drv = driver_for_task.lock().await;
                        drv.dbus.take()
                    };
                    if let Some(dbus) = taken_dbus.as_mut() {
                        let _ = dbus.export_snapshot(&value).await;
                    }
                    // Restore the dbus handle
                    if let Some(dbus) = taken_dbus {
                        let mut drv = driver_for_task.lock().await;
                        if drv.dbus.is_none() {
                            drv.dbus = Some(dbus);
                        }
                    }
                    if rx.changed().await.is_err() {
                        // channel closed -> pause and retry obtaining a new receiver
                        tokio::time::sleep(Duration::from_millis(250)).await;
                        rx = {
                            let drv = driver_for_task.lock().await;
                            drv.subscribe_snapshot()
                        };
                    }
                }
            })
        };

        // PV reader task (dedicated single-thread runtime) to avoid Send bounds
        // Use a stop channel to allow graceful shutdown
        let (pv_stop_tx, pv_stop_rx) = watch::channel(false);
        let pv_handle = {
            let driver_for_task = driver.clone();
            let pv_stop_rx = pv_stop_rx;
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("pv runtime");
                rt.block_on(async move {
                    let mut stop_rx = pv_stop_rx;
                    let mut ticker = interval(Duration::from_millis(1000));
                    loop {
                        tokio::select! {
                            _ = ticker.tick() => {
                                // Snapshot fields needed for lag compensation without long locks
                                let (ev_power_for_subtract, should_log_debug) = {
                                    let drv = driver_for_task.lock().await;
                                    let lag_ms = drv.config.controls.ev_reporting_lag_ms as u128;
                                    let within_lag = drv.last_set_current_monotonic.elapsed().as_millis() < lag_ms;
                                    let ev_est = if within_lag {
                                        let phases = 3.0f64; // TODO: detect phases
                                        (drv.last_sent_current as f64 * 230.0f64 * phases).max(0.0)
                                    } else { drv.last_total_power };
                                    (ev_est, within_lag)
                                };
                                // Compute PV excess without holding the driver lock: temporarily take dbus
                                let taken_dbus = {
                                    let mut drv = driver_for_task.lock().await;
                                    drv.dbus.take()
                                };
                                let excess_opt = if let Some(dbus) = taken_dbus.as_ref() {
                                    AlfenDriver::calculate_excess_pv_power_with_dbus(dbus, ev_power_for_subtract).await
                                } else {
                                    None
                                };
                                // Restore the dbus handle
                                if let Some(dbus) = taken_dbus {
                                    let mut drv = driver_for_task.lock().await;
                                    if drv.dbus.is_none() {
                                        drv.dbus = Some(dbus);
                                    }
                                }
                                if let Some(ex) = excess_opt {
                                    let mut drv = driver_for_task.lock().await;
                                    // Apply EMA smoothing
                                    let alpha = drv.config.controls.pv_excess_ema_alpha.clamp(0.0, 1.0);
                                    if alpha > 0.0 {
                                        let prev = drv.pv_excess_ema_w;
                                        let ema = alpha * ex + (1.0 - alpha) * prev;
                                        drv.pv_excess_ema_w = ema;
                                        drv.last_excess_pv_power_w = ema;
                                    } else {
                                        drv.pv_excess_ema_w = ex;
                                        drv.last_excess_pv_power_w = ex;
                                    }
                                    // Track last non-zero excess time
                                    if drv.last_excess_pv_power_w > 50.0 {
                                        drv.last_nonzero_excess_at = std::time::Instant::now();
                                    }
                                    if should_log_debug {
                                        drv.logger.debug(&format!("Lag compensation active: ev_power_for_subtract={:.0}W, last_sent_current={:.2}A", ev_power_for_subtract, drv.last_sent_current));
                                    }
                                }
                            }
                            res = stop_rx.changed() => {
                                let _ = res; // treat any change or closure as stop
                                break;
                            }
                        }
                    }
                });
            })
        };

        // Wait for shutdown signal while workers run
        loop {
            let mut drv = driver.lock().await;
            if drv.shutdown_rx.try_recv().is_ok() {
                drv.logger.info("Shutdown signal received");
                break;
            }
            drop(drv);
            tokio::time::sleep(Duration::from_millis(200)).await;
        }

        // Shutdown sequence
        {
            let mut drv = driver.lock().await;
            drv.state.send(DriverState::ShuttingDown).ok();
            drv.shutdown().await?;
        }

        exporter_handle.abort();
        // Signal and join PV thread without blocking the async runtime
        let _ = pv_stop_tx.send(true);
        let _ = tokio::task::spawn_blocking(move || {
            let _ = pv_handle.join();
        })
        .await;
        // Signal and join Modbus worker
        let _ = mb_stop_tx.send(true);
        let _ = tokio::task::spawn_blocking(move || {
            let _ = modbus_handle.join();
        })
        .await;

        Ok(())
    }
}

impl AlfenDriver {
    /// Start D-Bus once we have identity information; avoid publishing placeholder defaults first
    async fn try_start_dbus_with_identity(&mut self) -> Result<()> {
        // Start D-Bus service (we will publish identity immediately after)
        let mut dbus =
            DbusService::new(self.config.device_instance, self.commands_tx.clone()).await?;
        dbus.start().await?;
        self.dbus = Some(dbus);

        // Prepare initial control values before mutably borrowing self.dbus
        let start_stop_init: u8 = self.start_stop as u8;
        // Publish management and core info
        if let Some(d) = &mut self.dbus {
            // Device instance and connection are safe to publish
            let conn_str = format!(
                "Modbus TCP at {}:{}",
                self.config.modbus.ip, self.config.modbus.port
            );
            let _ = d
                .update_paths([
                    (
                        "/Mgmt/ProcessName".to_string(),
                        serde_json::json!("phaeton"),
                    ),
                    (
                        "/Mgmt/ProcessVersion".to_string(),
                        serde_json::json!(env!("CARGO_PKG_VERSION")),
                    ),
                    ("/Mgmt/Connection".to_string(), serde_json::json!(conn_str)),
                    (
                        "/DeviceInstance".to_string(),
                        serde_json::json!(self.config.device_instance),
                    ),
                    ("/ProductId".to_string(), serde_json::json!(0xC024u32)),
                    ("/Connected".to_string(), serde_json::json!(1u8)),
                    ("/Model".to_string(), serde_json::json!("AC22NS")),
                ])
                .await;

            // Create writable control paths using persisted state
            let _ = d
                .ensure_item("/Mode", serde_json::json!(self.current_mode as u8), true)
                .await;
            let _ = d
                .ensure_item("/StartStop", serde_json::json!(start_stop_init), true)
                .await;
            let _ = d
                .ensure_item(
                    "/SetCurrent",
                    serde_json::json!(self.intended_set_current),
                    true,
                )
                .await;
            let _ = d
                .ensure_item("/Position", serde_json::json!(0u8), true)
                .await;
            let _ = d
                .ensure_item("/AutoStart", serde_json::json!(0u8), true)
                .await;
            let _ = d
                .ensure_item("/EnableDisplay", serde_json::json!(0u8), true)
                .await;
        }

        // After DBus is up, read and publish identity (ProductName, FirmwareVersion, Serial)
        let _ = self.refresh_charger_identity().await;

        // Emit a snapshot after identity refresh so UI can show early values
        let snapshot = Arc::new(self.build_typed_snapshot(None));
        let _ = self.status_snapshot_tx.send(snapshot);

        Ok(())
    }
}

impl AlfenDriver {
    /// Handle external command
    async fn handle_command(&mut self, cmd: DriverCommand) {
        match cmd {
            DriverCommand::SetMode(m) => self.set_mode(m).await,
            DriverCommand::SetStartStop(v) => self.set_start_stop(v).await,
            DriverCommand::SetCurrent(a) => self.set_intended_current(a).await,
        }
    }

    /// Map Alfen Mode3 status string to Victron-esque numeric status
    /// 0=Disconnected, 1=Connected, 2=Charging
    pub(crate) fn map_alfen_status_to_victron(status_str: &str) -> u8 {
        let s = status_str.trim_matches(char::from(0)).trim().to_uppercase();
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
            self.logger.info(&format!(
                "Mode changed: {} -> {}",
                self.current_mode as u8, new_mode as u8
            ));
        }
        self.current_mode = new_mode;
        if let Some(dbus) = &mut self.dbus {
            let _ = dbus.update_path("/Mode", serde_json::json!(mode)).await;
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
        if let Some(dbus) = &mut self.dbus {
            let _ = dbus
                .update_path("/StartStop", serde_json::json!(value))
                .await;
        }
        self.persistence.set_start_stop(self.start_stop as u32);
        let _ = self.persistence.save();
    }

    pub async fn set_intended_current(&mut self, amps: f32) {
        let clamped = amps.max(0.0).min(self.config.controls.max_set_current);
        self.intended_set_current = clamped;
        if let Some(dbus) = &mut self.dbus {
            let _ = dbus
                .update_path("/SetCurrent", serde_json::json!(clamped))
                .await;
        }
        self.persistence.set_set_current(self.intended_set_current);
        let _ = self.persistence.save();
        // Record the moment we changed the intended current to enable lag compensation
        self.last_set_current_monotonic = std::time::Instant::now();
    }
}

impl AlfenDriver {
    fn last_poll_duration_ms(&self) -> u64 {
        // Placeholder: could store last duration explicitly; use 0 to indicate unknown
        0
    }
}
