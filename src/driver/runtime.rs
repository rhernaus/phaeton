use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, watch};
use tokio::time::{Duration, interval};

use crate::error::Result;

use super::types::DriverSnapshot;

impl super::AlfenDriver {
    /// Create a new driver instance
    pub async fn new(
        commands_rx: mpsc::UnboundedReceiver<super::types::DriverCommand>,
        commands_tx: mpsc::UnboundedSender<super::types::DriverCommand>,
    ) -> Result<Self> {
        let config = crate::config::Config::load().map_err(|e| {
            eprintln!("Failed to load configuration: {}", e);
            e
        })?;

        // Initialize logging
        crate::logging::init_logging(&config.logging)?;

        let _context =
            crate::logging::LogContext::new("driver").with_device_instance(config.device_instance);

        let logger = crate::logging::get_logger("driver");

        let (shutdown_tx, shutdown_rx) = mpsc::unbounded_channel();
        let (state_tx, _) = watch::channel(super::types::DriverState::Initializing);

        logger.info("Initializing EV charger driver");

        // Initialize persistence and load any saved state (best-effort)
        let mut persistence =
            crate::persistence::PersistenceManager::new("/data/phaeton_state.json");
        let _ = persistence.load();

        // Initialize session manager and restore previous session state if available
        let mut sessions = crate::session::ChargingSessionManager::default();
        if let Some(sess_state) = persistence.get_section("session") {
            let _ = sessions.restore_state(sess_state);
        }

        // Restore control states from persistence
        let mut current_mode = crate::controls::ChargingMode::Manual;
        if let Some(mode_val) = persistence.get::<u32>("mode") {
            current_mode = match mode_val {
                1 => crate::controls::ChargingMode::Auto,
                2 => crate::controls::ChargingMode::Scheduled,
                _ => crate::controls::ChargingMode::Manual,
            };
        }

        let mut start_stop = crate::controls::StartStopState::Stopped;
        if let Some(ss) = persistence.get::<u32>("start_stop") {
            start_stop = if ss == 1 {
                crate::controls::StartStopState::Enabled
            } else {
                crate::controls::StartStopState::Stopped
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
            controls: crate::controls::ChargingControls::new(),
            current_mode,
            start_stop,
            intended_set_current,
            station_max_current: 32.0,
            last_sent_current: 0.0,
            last_current_set_time: std::time::Instant::now(),
            last_set_current_monotonic: std::time::Instant::now(),
            last_status: 0,

            min_charge_timer_deadline: None,
            auto_mode_entered_at: None,
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
        self.state.send(super::types::DriverState::Running).ok();

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
        self.state
            .send(super::types::DriverState::ShuttingDown)
            .ok();
        self.shutdown().await?;

        Ok(())
    }

    /// Initialize Modbus connection
    pub(crate) async fn initialize_modbus(&mut self) -> Result<()> {
        let manager = crate::modbus::ModbusConnectionManager::new(
            &self.config.modbus,
            self.config.controls.max_retries,
            Duration::from_secs_f64(self.config.controls.retry_delay),
        );

        self.modbus_manager = Some(manager);
        self.logger.info("Modbus connection manager initialized");
        Ok(())
    }

    /// Single polling cycle
    #[allow(clippy::cognitive_complexity)]
    pub(crate) async fn poll_cycle(&mut self) -> Result<()> {
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
                    && let Ok(max_c) = crate::modbus::decode_32bit_float(&max_regs[0..2])
                    && max_c.is_finite()
                    && max_c > 0.0
                {
                    self.station_max_current = max_c;
                }

                // Decode values with safe fallbacks
                let (l1_v, l2_v, l3_v) = match voltages {
                    Some(v) if v.len() >= 6 => (
                        crate::modbus::decode_32bit_float(&v[0..2]).unwrap_or(0.0) as f64,
                        crate::modbus::decode_32bit_float(&v[2..4]).unwrap_or(0.0) as f64,
                        crate::modbus::decode_32bit_float(&v[4..6]).unwrap_or(0.0) as f64,
                    ),
                    _ => (0.0, 0.0, 0.0),
                };

                let (l1_i, l2_i, l3_i) = match currents {
                    Some(v) if v.len() >= 6 => (
                        crate::modbus::decode_32bit_float(&v[0..2]).unwrap_or(0.0) as f64,
                        crate::modbus::decode_32bit_float(&v[2..4]).unwrap_or(0.0) as f64,
                        crate::modbus::decode_32bit_float(&v[4..6]).unwrap_or(0.0) as f64,
                    ),
                    _ => (0.0, 0.0, 0.0),
                };

                let (mut l1_p, mut l2_p, mut l3_p, mut p_total) = match power_regs {
                    Some(v) if v.len() >= 8 => {
                        let p1 = crate::modbus::decode_32bit_float(&v[0..2]).unwrap_or(0.0) as f64;
                        let p2 = crate::modbus::decode_32bit_float(&v[2..4]).unwrap_or(0.0) as f64;
                        let p3 = crate::modbus::decode_32bit_float(&v[4..6]).unwrap_or(0.0) as f64;
                        let pt = crate::modbus::decode_32bit_float(&v[6..8]).unwrap_or(0.0) as f64;
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
                    Some(v) if v.len() >= 4 => crate::modbus::decode_64bit_float(&v[0..4]).unwrap_or(0.0),
                    _ => 0.0,
                };
                let energy_kwh = energy_wh / 1000.0;

                let status_base = match status_regs {
                    Some(v) if v.len() >= 5 => {
                        let s = crate::modbus::decode_string(&v[0..5], None).unwrap_or_default();
                        Self::map_alfen_status_to_victron(&s) as i32
                    }
                    _ => 0,
                };
                // Derive extended status: 4=WAIT_SUN, 6=WAIT_START, 7=LOW_SOC (approximate)
                let mut status = status_base;
                let connected = status_base == 1 || status_base == 2;
                if connected {
                    if matches!(self.start_stop, crate::controls::StartStopState::Stopped) {
                        status = 6; // WAIT_START
                    } else if matches!(self.current_mode, crate::controls::ChargingMode::Auto) {
                        // If Auto mode and very low effective/last current, WAIT_SUN
                        if self.last_sent_current < 0.1 {
                            status = 4; // WAIT_SUN
                        }
                    } else if matches!(self.current_mode, crate::controls::ChargingMode::Scheduled) {
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
                        crate::controls::ChargingMode::Manual => "manual",
                        crate::controls::ChargingMode::Auto => "pv_auto",
                        crate::controls::ChargingMode::Scheduled => "scheduled",
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
    pub(crate) async fn shutdown(&mut self) -> Result<()> {
        self.logger.info("Shutting down driver");

        if let Some(_manager) = self.modbus_manager.take() {
            // Disconnect Modbus
            // TODO: Implement proper disconnect
        }

        self.logger.info("Driver shutdown complete");
        Ok(())
    }
}

impl super::AlfenDriver {
    pub(crate) fn last_poll_duration_ms(&self) -> u64 {
        // Placeholder: could store last duration explicitly; use 0 to indicate unknown
        0
    }
}
