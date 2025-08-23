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
use tokio::sync::{mpsc, watch};
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

/// Commands accepted by the driver from external components (web, etc.)
#[derive(Debug, Clone)]
pub enum DriverCommand {
    SetMode(u8),
    SetStartStop(u8),
    SetCurrent(f32),
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

    /// D-Bus service (stub)
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
    /// Last observed Victron-esque status (0=Disc,1=Conn,2=Charging)
    last_status: u8,

    /// Command receiver for external control
    commands_rx: mpsc::UnboundedReceiver<DriverCommand>,

    /// Command sender (fan-out to subsystems like D-Bus, web if needed)
    commands_tx: mpsc::UnboundedSender<DriverCommand>,
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

        logger.info("Initializing Alfen driver");

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
            last_status: 0,
            commands_rx,
            commands_tx,
        })
    }

    /// Run the driver main loop
    pub async fn run(&mut self) -> Result<()> {
        self.logger.info("Starting Alfen driver main loop");

        // Initialize Modbus connection
        self.initialize_modbus().await?;

        // Update state to running
        self.state.send(DriverState::Running).ok();

        // Initialize D-Bus service (stub) and start
        let mut dbus =
            DbusService::new(self.config.device_instance, self.commands_tx.clone()).await?;
        dbus.start().await?;
        self.dbus = Some(dbus);

        // Initialize control state from config defaults
        self.intended_set_current = self.config.defaults.intended_set_current;
        self.station_max_current = self.config.defaults.station_max_current;
        if let Some(dbus) = &mut self.dbus {
            let _ = dbus
                .update_paths([
                    (
                        "/DeviceInstance".to_string(),
                        serde_json::json!(self.config.device_instance),
                    ),
                    (
                        "/ProductName".to_string(),
                        serde_json::json!("Alfen EV Charger"),
                    ),
                    (
                        "/Mode".to_string(),
                        serde_json::json!(self.current_mode as u8),
                    ),
                    (
                        "/StartStop".to_string(),
                        serde_json::json!(self.start_stop as u8),
                    ),
                    (
                        "/SetCurrent".to_string(),
                        serde_json::json!(self.intended_set_current),
                    ),
                ])
                .await;
        }

        // Main polling loop
        let mut poll_interval = interval(Duration::from_millis(self.config.poll_interval_ms));

        loop {
            tokio::select! {
                _ = poll_interval.tick() => {
                    if let Err(e) = self.poll_cycle().await {
                        self.logger.error(&format!("Poll cycle failed: {}", e));
                        // Continue polling even on errors
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
        if let Some(manager) = &mut self.modbus_manager {
            let socket_id = self.config.modbus.socket_slave_id;
            let addr_voltages = self.config.registers.voltages;
            let addr_currents = self.config.registers.currents;
            let addr_power = self.config.registers.power;
            let addr_energy = self.config.registers.energy;
            let addr_status = self.config.registers.status;
            let addr_amps = self.config.registers.amps_config;
            let station_id = self.config.modbus.station_slave_id;
            let addr_station_max = self.config.registers.station_max_current;

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
                    decode_32bit_float(&v[0..2]).unwrap_or(0.0),
                    decode_32bit_float(&v[2..4]).unwrap_or(0.0),
                    decode_32bit_float(&v[4..6]).unwrap_or(0.0),
                ),
                _ => (0.0, 0.0, 0.0),
            };

            let (l1_i, l2_i, l3_i) = match currents {
                Some(v) if v.len() >= 6 => (
                    decode_32bit_float(&v[0..2]).unwrap_or(0.0),
                    decode_32bit_float(&v[2..4]).unwrap_or(0.0),
                    decode_32bit_float(&v[4..6]).unwrap_or(0.0),
                ),
                _ => (0.0, 0.0, 0.0),
            };

            let (l1_p, l2_p, l3_p, p_total) = match power_regs {
                Some(v) if v.len() >= 8 => (
                    decode_32bit_float(&v[0..2]).unwrap_or(0.0) as f64,
                    decode_32bit_float(&v[2..4]).unwrap_or(0.0) as f64,
                    decode_32bit_float(&v[4..6]).unwrap_or(0.0) as f64,
                    decode_32bit_float(&v[6..8]).unwrap_or(0.0) as f64,
                ),
                _ => (0.0, 0.0, 0.0, 0.0),
            };

            let energy_wh = match energy_regs {
                Some(v) if v.len() >= 4 => decode_64bit_float(&v[0..4]).unwrap_or(0.0),
                _ => 0.0,
            };
            let energy_kwh = energy_wh / 1000.0;

            let status_u8 = match status_regs {
                Some(v) if v.len() >= 5 => {
                    let s = decode_string(&v[0..5], None).unwrap_or_default();
                    Self::map_alfen_status_to_victron(&s) as i32
                }
                _ => 0,
            };
            let status = status_u8 as i32;

            // Control logic: compute effective current and write via Modbus if needed
            let now_secs = (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default())
            .as_secs_f64();
            let requested = self.intended_set_current;
            let effective: f32 = self
                .controls
                .compute_effective_current(
                    self.current_mode,
                    self.start_stop,
                    requested,
                    self.station_max_current,
                    now_secs,
                    Some(p_total as f32),
                    &self.config,
                )
                .await
                .unwrap_or(0.0);

            let need_watchdog = self.last_current_set_time.elapsed().as_secs()
                >= self.config.controls.watchdog_interval_seconds as u64;
            let need_change = (effective - self.last_sent_current).abs()
                > self.config.controls.update_difference_threshold;
            if need_watchdog || need_change {
                let regs = crate::modbus::encode_32bit_float(effective);
                let write_res = manager
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

            // D-Bus metrics (publish authoritative values)
            if let Some(dbus) = &mut self.dbus {
                let mut updates = Vec::with_capacity(16);
                updates.push(("/Ac/L1/Voltage".to_string(), serde_json::json!(l1_v)));
                updates.push(("/Ac/L2/Voltage".to_string(), serde_json::json!(l2_v)));
                updates.push(("/Ac/L3/Voltage".to_string(), serde_json::json!(l3_v)));
                updates.push(("/Ac/L1/Current".to_string(), serde_json::json!(l1_i)));
                updates.push(("/Ac/L2/Current".to_string(), serde_json::json!(l2_i)));
                updates.push(("/Ac/L3/Current".to_string(), serde_json::json!(l3_i)));
                updates.push(("/Ac/L1/Power".to_string(), serde_json::json!(l1_p)));
                updates.push(("/Ac/L2/Power".to_string(), serde_json::json!(l2_p)));
                updates.push(("/Ac/L3/Power".to_string(), serde_json::json!(l3_p)));
                updates.push(("/Ac/Power".to_string(), serde_json::json!(p_total)));
                updates.push(("/Status".to_string(), serde_json::json!(status)));
                // Session energy forward: from active or last session, else 0.0
                let stats = self.sessions.get_session_stats();
                let energy_forward = stats
                    .get("energy_delivered_kwh")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                updates.push((
                    "/Ac/Energy/Forward".to_string(),
                    serde_json::json!(energy_forward),
                ));
                // Derived paths
                let max_phase_current = l1_i.max(l2_i.max(l3_i));
                updates.push((
                    "/Ac/Current".to_string(),
                    serde_json::json!(max_phase_current),
                ));
                updates.push(("/Current".to_string(), serde_json::json!(max_phase_current)));
                // Also publish the requested set current as authoritative value
                updates.push((
                    "/SetCurrent".to_string(),
                    serde_json::json!(self.intended_set_current as f64),
                ));
                let phase_count = [l1_i, l2_i, l3_i]
                    .iter()
                    .filter(|v| v.is_finite() && v.abs() > 0.01)
                    .count();
                updates.push(("/Ac/PhaseCount".to_string(), serde_json::json!(phase_count)));
                let charging_time_sec = stats
                    .get("session_duration_min")
                    .and_then(|v| v.as_f64())
                    .map(|m| (m * 60.0).round() as i64)
                    .unwrap_or(0);
                updates.push((
                    "/ChargingTime".to_string(),
                    serde_json::json!(charging_time_sec),
                ));
                dbus.update_paths(updates).await?;
            }

            // Log a concise summary
            self.logger.debug(&format!(
                "V=({:.1},{:.1},{:.1})V I=({:.2},{:.2},{:.2})A P=({:.0},{:.0},{:.0})W total={:.0}W E={:.3}kWh status={}",
                l1_v, l2_v, l3_v, l1_i, l2_i, l3_i, l1_p, l2_p, l3_p, p_total, energy_kwh, status
            ));

            // TODO: apply control logic and write set-current via Modbus
        }

        self.logger.debug("Poll cycle completed");
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
        self.dbus.as_ref().and_then(|d| d.get(path)).cloned()
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
    fn map_alfen_status_to_victron(status_str: &str) -> u8 {
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
        self.current_mode = match mode {
            1 => ChargingMode::Auto,
            2 => ChargingMode::Scheduled,
            _ => ChargingMode::Manual,
        };
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
    }
}
