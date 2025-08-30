use crate::error::Result;
use std::sync::Arc;

mod io;
pub mod meas;
mod phase;
mod status;
use meas::RealtimeMeasurements;

impl super::AlfenDriver {
    // derive_status moved to status.rs

    async fn fetch_battery_soc_and_minimum_limit(&self) -> Option<(f64, f64)> {
        let dbus_guard = self.dbus.as_ref()?.lock().await;
        // Read battery SoC from com.victronenergy.system
        async fn get_f64(svc: &crate::dbus::DbusService, service: &str, path: &str) -> Option<f64> {
            match svc.read_remote_value(service, path).await {
                Ok(v) => v
                    .as_f64()
                    .or_else(|| v.as_i64().map(|x| x as f64))
                    .or_else(|| v.as_u64().map(|x| x as f64)),
                Err(_) => None,
            }
        }

        let soc_opt = get_f64(&dbus_guard, "com.victronenergy.system", "/Dc/Battery/Soc").await;
        let soc = match soc_opt {
            Some(s) if s.is_finite() => s,
            _ => return None,
        };

        // Find MinimumSocLimit from any com.victronenergy.multi.* device
        let names = dbus_guard
            .list_service_names_with_prefix("com.victronenergy.multi")
            .await
            .unwrap_or_default();
        for svc_name in names {
            if let Some(min) =
                get_f64(&dbus_guard, &svc_name, "/Settings/Ess/MinimumSocLimit").await
                && min.is_finite()
            {
                return Some((soc, min));
            }
        }
        None
    }

    #[cfg(test)]
    async fn update_station_max_current_from_modbus(&mut self) {
        let station_id = self.config.modbus.station_slave_id;
        let addr_station_max = self.config.registers.station_max_current;
        let manager = self.modbus_manager.as_mut().unwrap();
        if let Ok(max_regs) = manager
            .read_holding_registers(station_id, addr_station_max, 2)
            .await
            && max_regs.len() >= 2
            && let Ok(max_c) = crate::modbus::decode_32bit_float(&max_regs[0..2])
            && max_c.is_finite()
            && max_c > 0.0
        {
            self.station_max_current = max_c;
        }
    }

    // read_realtime_values moved to io.rs

    fn ev_power_for_subtract(&self, p_total: f64) -> f64 {
        let lag_ms = self.config.controls.ev_reporting_lag_ms as u128;
        if self.last_set_current_monotonic.elapsed().as_millis() < lag_ms {
            let phases = if self.applied_phases >= 3 {
                3.0
            } else if self.applied_phases == 1 {
                1.0
            } else {
                // Unknown -> assume 3P to preserve previous behavior and test expectations
                3.0
            };
            (self.last_sent_current as f64 * 230.0f64 * phases).max(0.0)
        } else {
            p_total
        }
    }

    fn should_send_update(&self, effective: f32) -> (bool, bool, bool) {
        let interval_due = self.last_current_set_time.elapsed().as_millis()
            >= self.config.controls.current_update_interval as u128;
        let watchdog_due = self.last_current_set_time.elapsed().as_secs()
            >= self.config.controls.watchdog_interval_seconds as u64;
        let need_watchdog = interval_due || watchdog_due;
        let need_change = (effective - self.last_sent_current).abs()
            > self.config.controls.update_difference_threshold;
        (need_watchdog || need_change, need_change, interval_due)
    }

    fn current_mode_reason(&self) -> &'static str {
        match self.current_mode {
            crate::controls::ChargingMode::Manual => "manual",
            crate::controls::ChargingMode::Auto => "pv_auto",
            crate::controls::ChargingMode::Scheduled => "scheduled",
        }
    }

    async fn write_effective_current(&mut self, effective: f32) -> bool {
        let socket_id = self.config.modbus.socket_slave_id;
        let addr_amps = self.config.registers.amps_config;
        let regs = crate::modbus::encode_32bit_float(effective);
        let write_res = self
            .modbus_manager
            .as_mut()
            .unwrap()
            .write_multiple_registers(socket_id, addr_amps, &regs)
            .await;
        write_res.is_ok()
    }

    async fn compute_effective_current_with_soc(
        &mut self,
        requested: f32,
        now_secs: f64,
        excess_pv_power_w: f32,
    ) -> (f32, Option<bool>) {
        // Determine assumed phases for conversion based on applied phases
        let assumed_phases = if self.applied_phases >= 3 { 3 } else { 1 };
        let mut effective: f32 = self
            .controls
            .compute_effective_current(
                self.current_mode,
                self.start_stop,
                requested,
                self.station_max_current,
                now_secs,
                Some(excess_pv_power_w),
                &self.config,
                assumed_phases,
            )
            .await
            .unwrap_or(0.0);
        let soc_below_min = self.enforce_soc_limit_maybe(&mut effective).await;
        self.apply_insufficient_solar_grace_timer(soc_below_min, &mut effective);
        (effective, soc_below_min)
    }

    fn enforce_phase_settle_on_effective(&mut self, effective: &mut f32) {
        if let Some(deadline) = self.phase_settle_deadline {
            if std::time::Instant::now() < deadline {
                if *effective > 0.0 {
                    self.logger
                        .debug("Phase switch settling active; forcing 0 A");
                }
                *effective = 0.0;
            } else {
                self.phase_settle_deadline = None;
            }
        }
    }

    async fn enforce_soc_limit_maybe(&mut self, effective: &mut f32) -> Option<bool> {
        if !matches!(self.start_stop, crate::controls::StartStopState::Enabled) {
            return None;
        }
        if !(matches!(self.current_mode, crate::controls::ChargingMode::Auto)
            || matches!(self.current_mode, crate::controls::ChargingMode::Scheduled))
        {
            return None;
        }
        if let Some((soc, min_limit)) = self.fetch_battery_soc_and_minimum_limit().await
            && soc.is_finite()
            && min_limit.is_finite()
        {
            let below = soc < min_limit;
            if below && !matches!(self.current_mode, crate::controls::ChargingMode::Manual) {
                if *effective > 0.0 {
                    self.logger.info(&format!(
                        "Stopping charging due to Low SOC: SoC={:.1}% < MinimumSocLimit={:.1}%",
                        soc, min_limit
                    ));
                }
                *effective = 0.0;
            }
            Some(below)
        } else {
            None
        }
    }

    fn apply_insufficient_solar_grace_timer(
        &mut self,
        soc_below_min: Option<bool>,
        effective: &mut f32,
    ) {
        if soc_below_min == Some(true)
            || !matches!(self.start_stop, crate::controls::StartStopState::Enabled)
            || !matches!(self.current_mode, crate::controls::ChargingMode::Auto)
        {
            // Mode disabled, not Auto, or low SoC -> no grace behavior
            if self.min_charge_timer_deadline.is_some()
                && !matches!(self.current_mode, crate::controls::ChargingMode::Auto)
            {
                self.min_charge_timer_deadline = None;
            }
            return;
        }

        let min_current = self.config.controls.min_set_current.max(0.0);
        let now = std::time::Instant::now();
        let was_charging = self.last_sent_current >= (min_current - 0.05);

        // Only start (or keep) the grace timer if we were previously charging
        // at or above the EVSE minimum current and PV has now become
        // insufficient. Do not restart the timer purely because we
        // recently updated the setpoint (e.g., after expiry to 0 A).
        if *effective < min_current && was_charging {
            match self.min_charge_timer_deadline {
                None => {
                    // Start the grace timer
                    let secs = self.config.controls.min_charge_duration_seconds as u64;
                    self.min_charge_timer_deadline =
                        Some(now + std::time::Duration::from_secs(secs));
                    if min_current > 0.0 {
                        *effective = min_current;
                    }
                    self.logger.info(&format!(
                        "Insufficient PV: starting {}s grace timer; holding at {:.2} A",
                        self.config.controls.min_charge_duration_seconds, min_current
                    ));
                }
                Some(deadline) => {
                    if deadline > now {
                        // Keep holding minimum current while timer active
                        if min_current > 0.0 {
                            *effective = min_current;
                        }
                        let remaining = deadline.saturating_duration_since(now).as_secs();
                        self.logger.debug(&format!(
                            "Insufficient PV: grace timer active ({}s remaining)",
                            remaining
                        ));
                    } else {
                        // Timer expired; allow stopping
                        self.min_charge_timer_deadline = None;
                        // effective remains as computed (likely 0.0)
                        self.logger
                            .info("Insufficient PV: grace timer expired; allowing stop");
                    }
                }
            }
        } else if *effective >= min_current {
            // PV sufficient again -> clear any outstanding timer
            if self.min_charge_timer_deadline.is_some() {
                self.min_charge_timer_deadline = None;
                self.logger
                    .info("Sufficient PV restored; clearing grace timer");
            }
        } else {
            // Not recently charging and below min -> ensure timer cleared
            if self.min_charge_timer_deadline.is_some() {
                self.min_charge_timer_deadline = None;
            }
        }
    }

    fn apply_current_if_needed(
        &mut self,
        effective: f32,
        excess_pv_power_w: f32,
    ) -> (bool, bool, bool) {
        let (should_update, need_change, interval_due) = self.should_send_update(effective);
        if should_update {
            let phase_label = if self.applied_phases >= 3 { "3P" } else { "1P" };
            if need_change {
                let reason = self.current_mode_reason();
                self.logger.info(&format!(
                    "Adjusting available current: {:.2} A -> {:.2} A (reason={}, pv_excess={:.0} W, station_max={:.1} A, phases={})",
                    self.last_sent_current, effective, reason, excess_pv_power_w, self.station_max_current, phase_label
                ));
            } else if interval_due {
                let reason = self.current_mode_reason();
                self.logger.info(&format!(
                    "Reasserting available current: {:.2} A (reason={}, pv_excess={:.0} W, station_max={:.1} A, phases={})",
                    effective, reason, excess_pv_power_w, self.station_max_current, phase_label
                ));
            }
        }
        (should_update, need_change, interval_due)
    }

    fn finalize_cycle(
        &mut self,
        m: &RealtimeMeasurements,
        cur_status: u8,
        effective: f32,
    ) -> Result<()> {
        self.handle_session_transition(cur_status, m.energy_kwh);
        self.sessions.update(m.total_power, m.energy_kwh)?;
        self.persist_state();
        self.update_last_measurements(m);
        self.logger.debug(&format!(
            "V=({:.1},{:.1},{:.1})V I=({:.2},{:.2},{:.2})A P=({:.0},{:.0},{:.0})W total={:.0}W E={:.3}kWh status={} lag_ms={} last_sent_A={:.2}",
            m.voltages.l1, m.voltages.l2, m.voltages.l3, m.currents.l1, m.currents.l2, m.currents.l3, m.powers.l1, m.powers.l2, m.powers.l3, m.total_power, m.energy_kwh, cur_status,
            self.last_set_current_monotonic.elapsed().as_millis(), self.last_sent_current
        ));
        let _ = self
            .status_tx
            .send(self.build_status_json(effective, m.total_power));
        Ok(())
    }

    fn handle_session_transition(&mut self, cur_status: u8, energy_kwh: f64) {
        let prev_status = self.last_status;
        if cur_status == 2 && prev_status != 2 && self.sessions.current_session.is_none() {
            let _ = self.sessions.start_session(energy_kwh);
        } else if cur_status != 2
            && self.sessions.current_session.is_some()
            && self.sessions.end_session(energy_kwh).is_ok()
            && self.config.pricing.source.to_lowercase() == "static"
            && let Some(ref last) = self.sessions.last_session
        {
            let cost = last.energy_delivered_kwh * self.config.pricing.static_rate_eur_per_kwh;
            self.sessions.set_cost_on_last_session(cost);
        }
        self.last_status = cur_status;
    }

    fn persist_state(&mut self) {
        self.persistence.set_mode(self.current_mode as u32);
        self.persistence.set_start_stop(self.start_stop as u32);
        self.persistence.set_set_current(self.intended_set_current);
        let _ = self
            .persistence
            .set_section("session", self.sessions.get_state());
        let _ = self.persistence.save();
    }

    fn update_last_measurements(&mut self, m: &RealtimeMeasurements) {
        self.last_l1_voltage = m.voltages.l1;
        self.last_l2_voltage = m.voltages.l2;
        self.last_l3_voltage = m.voltages.l3;
        self.last_l1_current = m.currents.l1;
        self.last_l2_current = m.currents.l2;
        self.last_l3_current = m.currents.l3;
        self.last_l1_power = m.powers.l1;
        self.last_l2_power = m.powers.l2;
        self.last_l3_power = m.powers.l3;
        self.last_total_power = m.total_power;
        self.last_energy_kwh = m.energy_kwh;
    }

    fn build_status_json(&self, effective: f32, p_total: f64) -> String {
        let mut status_obj = serde_json::json!({
            "mode": self.current_mode_code(),
            "start_stop": self.start_stop_code(),
            "set_current": self.get_intended_set_current(),
            "applied_current": effective,
            "station_max_current": self.get_station_max_current(),
            "ac_power": p_total,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        if let Some(v) = self
            .sessions
            .get_session_stats()
            .get("energy_delivered_kwh")
            .and_then(|v| v.as_f64())
        {
            status_obj["energy_forward_kwh"] = serde_json::json!(v);
        }
        status_obj.to_string()
    }

    pub(crate) async fn poll_cycle(&mut self) -> Result<()> {
        self.logger.debug("Starting poll cycle");
        if self.modbus_manager.is_some() {
            let m = self.read_realtime_values().await;
            let now_secs = (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default())
            .as_secs_f64();
            let requested = self.intended_set_current;

            let (excess_pv_power_w, pv_excess_ms) =
                self.compute_pv_excess_smoothed(m.total_power).await;
            self.maybe_evaluate_auto_phase_switch(excess_pv_power_w)
                .await;
            let (effective, soc_below_min, compute_effective_ms) = self
                .compute_effective_with_soc_and_settle(requested, now_secs, excess_pv_power_w)
                .await;
            let write_current_ms = self.maybe_write_current(effective, excess_pv_power_w).await;
            let derived_status = self.derive_final_status(m.status, soc_below_min);
            let finalize_ms = self.finalize_and_log(&m, derived_status, effective)?;
            self.record_post_compute_timings(
                pv_excess_ms,
                compute_effective_ms,
                write_current_ms,
                finalize_ms,
            );
        }

        self.refresh_identity_on_connection_edge().await;
        self.logger.debug("Poll cycle completed");
        let t_snap0 = std::time::Instant::now();
        let snapshot = Arc::new(self.build_typed_snapshot(Some(self.last_poll_duration_ms())));
        if let Some(ref mut steps) = self.last_poll_steps {
            steps.snapshot_build_ms = Some(t_snap0.elapsed().as_millis() as u64);
        }
        let _ = self.status_snapshot_tx.send(snapshot);
        Ok(())
    }

    async fn compute_pv_excess_smoothed(&mut self, total_power: f64) -> (f32, u64) {
        let t0 = std::time::Instant::now();
        let ev_power_for_subtract = self.ev_power_for_subtract(total_power);
        let raw: f32 = self
            .calculate_excess_pv_power(ev_power_for_subtract)
            .await
            .unwrap_or(0.0);
        let ms = t0.elapsed().as_millis() as u64;
        let alpha = self.config.controls.pv_excess_ema_alpha.clamp(0.0, 1.0);
        let smoothed = if alpha > 0.0 {
            alpha * raw + (1.0f32 - alpha) * self.last_excess_pv_power_w
        } else {
            raw
        };
        self.last_excess_pv_power_w = smoothed;
        (smoothed, ms)
    }

    async fn maybe_evaluate_auto_phase_switch(&mut self, excess_pv_power_w: f32) {
        if matches!(self.current_mode, crate::controls::ChargingMode::Auto)
            && self.config.controls.auto_phase_switch
        {
            self.evaluate_auto_phase_switch(excess_pv_power_w).await;
        }
    }

    async fn compute_effective_with_soc_and_settle(
        &mut self,
        requested: f32,
        now_secs: f64,
        excess_pv_power_w: f32,
    ) -> (f32, Option<bool>, u64) {
        let t0 = std::time::Instant::now();
        let (mut effective, soc_below_min) = self
            .compute_effective_current_with_soc(requested, now_secs, excess_pv_power_w)
            .await;
        self.enforce_phase_settle_on_effective(&mut effective);
        let ms = t0.elapsed().as_millis() as u64;
        (effective, soc_below_min, ms)
    }

    async fn maybe_write_current(&mut self, effective: f32, excess_pv_power_w: f32) -> Option<u64> {
        let (should_update, _need_change, _interval_due) =
            self.apply_current_if_needed(effective, excess_pv_power_w);
        if should_update {
            let t0 = std::time::Instant::now();
            if self.write_effective_current(effective).await {
                self.last_sent_current = effective;
                self.last_current_set_time = std::time::Instant::now();
                self.last_set_current_monotonic = std::time::Instant::now();
            } else {
                self.logger.warn("Failed to write set current via Modbus");
            }
            Some(t0.elapsed().as_millis() as u64)
        } else {
            None
        }
    }

    fn derive_final_status(&mut self, base_status: i32, soc_below_min: Option<bool>) -> u8 {
        if let Some(deadline) = self.phase_settle_deadline
            && std::time::Instant::now() < deadline
            && let Some(to) = self.phase_switch_to
        {
            if to >= 3 { 22 } else { 23 }
        } else {
            self.phase_switch_to = None;
            self.derive_status(base_status, soc_below_min) as u8
        }
    }

    fn finalize_and_log(
        &mut self,
        m: &RealtimeMeasurements,
        derived_status: u8,
        effective: f32,
    ) -> Result<u64> {
        let t0 = std::time::Instant::now();
        self.finalize_cycle(m, derived_status, effective)?;
        Ok(t0.elapsed().as_millis() as u64)
    }

    fn record_post_compute_timings(
        &mut self,
        pv_excess_ms: u64,
        compute_effective_ms: u64,
        write_current_ms: Option<u64>,
        finalize_ms: u64,
    ) {
        let mut steps = self.last_poll_steps.take().unwrap_or_default();
        steps.pv_excess_ms = Some(pv_excess_ms);
        steps.compute_effective_ms = Some(compute_effective_ms);
        steps.write_current_ms = write_current_ms;
        steps.finalize_cycle_ms = Some(finalize_ms);
        self.last_poll_steps = Some(steps);
    }

    async fn refresh_identity_on_connection_edge(&mut self) {
        if let Some(conn) = self
            .modbus_manager
            .as_ref()
            .and_then(|m| m.connection_status())
        {
            let prev = self.last_modbus_connected;
            if prev != Some(conn) {
                self.last_modbus_connected = Some(conn);
                if conn {
                    let _ = self.refresh_charger_identity().await;
                }
            }
        }
    }

    // evaluate_auto_phase_switch moved to phase.rs
}

#[cfg(test)]
mod tests;
