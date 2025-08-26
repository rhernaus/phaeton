//! Core driver logic for Phaeton
//!
//! This module contains the main driver state machine and orchestration logic
//! that coordinates all the different components of the system.

use crate::config::Config;
use crate::controls::{ChargingControls, ChargingMode, StartStopState};
use crate::dbus::DbusService;
use crate::error::Result;
// removed unused imports: LogContext, get_logger
use crate::modbus::{
    ModbusConnectionManager, decode_32bit_float, decode_64bit_float, decode_string,
};
use crate::persistence::PersistenceManager;
use crate::session::ChargingSessionManager;
// serde only used by types; keep driver free of unused imports
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, watch};
use tokio::time::{Duration, interval};

mod types;
pub use types::{DriverCommand, DriverSnapshot, DriverState};
use types::{Measurements, ModbusCommand};
mod commands;
mod dbus_helpers;
mod pv;
mod runtime;
mod runtime_arc;
mod snapshot;

// Measurements and ModbusCommand moved to types.rs

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
    /// Smoothed PV excess power (EMA) for stability
    pv_excess_ema_w: f32,
    /// Timestamp when PV excess was last clearly non-zero (for zero-grace)
    last_nonzero_excess_at: std::time::Instant,
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
                        // Only consider "already charging" if we didn't just enter Auto mode
                        let already_charging = drv.auto_mode_entered_at.is_none()
                            && (drv.last_sent_current >= 5.9 || drv.last_status == 2);

                        if have_min {
                            // Enough PV: clear any active timer and compute normally
                            drv.min_charge_timer_deadline = None;
                        } else if already_charging {
                            // Only when we were already charging do we start a grace timer
                            let duration = std::time::Duration::from_secs(
                                drv.config.controls.min_charge_duration_seconds as u64,
                            );
                            if drv.min_charge_timer_deadline.is_none() {
                                drv.min_charge_timer_deadline = Some(now + duration);
                                drv.logger.info(
                                    "PV below 6A during charging; starting minimum-charge countdown",
                                );
                            }
                        } else {
                            // Not enough PV and we are not yet charging: do not start timer, do not up-clamp
                            drv.min_charge_timer_deadline = None;
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
                        // Once we have seen enough PV or we actually start charging in Auto, clear entry marker
                        if have_min || drv.last_status == 2 {
                            drv.auto_mode_entered_at = None;
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
                    // Export snapshot using shared D-Bus mutex without taking ownership
                    if let Some(dbus_arc) = {
                        let drv = driver_for_task.lock().await;
                        drv.dbus.clone()
                    } {
                        let mut guard = dbus_arc.lock().await;
                        let _ = guard.export_snapshot(&value).await;
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
                                // Compute PV excess using a shared D-Bus handle guarded by a mutex
                                let excess_opt = {
                                    let dbus_arc_opt = {
                                        let drv = driver_for_task.lock().await;
                                        drv.dbus.clone()
                                    };
                                    if let Some(dbus_arc) = dbus_arc_opt {
                                        let guard = dbus_arc.lock().await;
                                        AlfenDriver::calculate_excess_pv_power_with_dbus(&guard, ev_power_for_subtract).await
                                    } else {
                                        None
                                    }
                                };
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
    // try_start_dbus_with_identity moved to dbus_helpers.rs
}

impl AlfenDriver {
    // /// Handle external command
    // handle_command moved to commands.rs

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
}

impl AlfenDriver {
    // last_poll_duration_ms moved to runtime.rs
}
