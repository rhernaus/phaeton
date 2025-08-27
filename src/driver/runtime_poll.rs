use crate::error::Result;
use std::sync::Arc;

struct LineTriplet {
    l1: f64,
    l2: f64,
    l3: f64,
}

struct RealtimeMeasurements {
    voltages: LineTriplet,
    currents: LineTriplet,
    powers: LineTriplet,
    total_power: f64,
    energy_kwh: f64,
    status: i32,
}

impl super::AlfenDriver {
    fn decode_triplet(regs: &Option<Vec<u16>>) -> LineTriplet {
        if let Some(v) = regs
            && v.len() >= 6
        {
            let a = crate::modbus::decode_32bit_float(&v[0..2]).unwrap_or(0.0) as f64;
            let b = crate::modbus::decode_32bit_float(&v[2..4]).unwrap_or(0.0) as f64;
            let c = crate::modbus::decode_32bit_float(&v[4..6]).unwrap_or(0.0) as f64;
            return LineTriplet {
                l1: a,
                l2: b,
                l3: c,
            };
        }
        LineTriplet {
            l1: 0.0,
            l2: 0.0,
            l3: 0.0,
        }
    }

    fn decode_energy_kwh(regs: &Option<Vec<u16>>) -> f64 {
        if let Some(v) = regs
            && v.len() >= 4
        {
            return crate::modbus::decode_64bit_float(&v[0..4]).unwrap_or(0.0) / 1000.0;
        }
        0.0
    }

    fn decode_powers(
        power_regs: &Option<Vec<u16>>,
        voltages: &LineTriplet,
        currents: &LineTriplet,
    ) -> (LineTriplet, f64) {
        let (mut l1, mut l2, mut l3, mut total) = if let Some(v) = power_regs {
            if v.len() >= 8 {
                let p1 = crate::modbus::decode_32bit_float(&v[0..2]).unwrap_or(0.0) as f64;
                let p2 = crate::modbus::decode_32bit_float(&v[2..4]).unwrap_or(0.0) as f64;
                let p3 = crate::modbus::decode_32bit_float(&v[4..6]).unwrap_or(0.0) as f64;
                let pt = crate::modbus::decode_32bit_float(&v[6..8]).unwrap_or(0.0) as f64;
                let sanitize = |x: f64| if x.is_finite() { x } else { 0.0 };
                (sanitize(p1), sanitize(p2), sanitize(p3), sanitize(pt))
            } else {
                (0.0, 0.0, 0.0, 0.0)
            }
        } else {
            (0.0, 0.0, 0.0, 0.0)
        };

        let approx = |v: f64, i: f64| (v * i).round();
        if l1.abs() < 1.0 {
            l1 = approx(voltages.l1, currents.l1);
        }
        if l2.abs() < 1.0 {
            l2 = approx(voltages.l2, currents.l2);
        }
        if l3.abs() < 1.0 {
            l3 = approx(voltages.l3, currents.l3);
        }
        if total.abs() < 1.0 {
            total = l1 + l2 + l3;
        }

        (LineTriplet { l1, l2, l3 }, total)
    }

    fn compute_status_from_regs(status_regs: &Option<Vec<u16>>) -> i32 {
        if let Some(v) = status_regs
            && v.len() >= 5
        {
            let s = crate::modbus::decode_string(&v[0..5], None).unwrap_or_default();
            return Self::map_alfen_status_to_victron(&s) as i32;
        }
        0
    }

    /// Derive Victron-esque status from base hardware status and current context.
    ///
    /// Rule order (highest precedence first):
    /// - StartStop=Stopped -> 6 (Wait start)
    /// - Scheduled mode with inactive window -> 6 (Wait start)
    /// - Auto or Scheduled with Low SoC -> 7 (Low SOC)
    /// - Auto with near-zero current -> 4 (Wait sun)
    /// - Fallback to base (0/1/2)
    fn derive_status(&self, status_base: i32, soc_below_min: Option<bool>) -> i32 {
        let connected = status_base == 1 || status_base == 2;
        if !connected {
            return status_base;
        }

        // Wait start due to explicit stop
        if matches!(self.start_stop, crate::controls::StartStopState::Stopped) {
            return 6;
        }

        // Low SOC for Auto and Scheduled (Manual continues)
        if (matches!(self.current_mode, crate::controls::ChargingMode::Auto)
            || matches!(self.current_mode, crate::controls::ChargingMode::Scheduled))
            && soc_below_min == Some(true)
        {
            return 7;
        }

        // Wait start due to inactive schedule window
        if matches!(self.current_mode, crate::controls::ChargingMode::Scheduled)
            && !crate::controls::ChargingControls::is_schedule_active(&self.config)
        {
            return 6;
        }

        // Wait sun when Auto but not currently charging / near-zero available
        if matches!(self.current_mode, crate::controls::ChargingMode::Auto)
            && self.last_sent_current < 0.1
        {
            return 4;
        }

        status_base
    }

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

    async fn read_realtime_values(&mut self) -> RealtimeMeasurements {
        let socket_id = self.config.modbus.socket_slave_id;
        let addr_voltages = self.config.registers.voltages;
        let addr_currents = self.config.registers.currents;
        let addr_power = self.config.registers.power;
        let addr_energy = self.config.registers.energy;
        let addr_status = self.config.registers.status;

        let manager = self.modbus_manager.as_mut().unwrap();

        let voltages = manager
            .read_holding_registers(socket_id, addr_voltages, 6)
            .await
            .ok();

        let currents = manager
            .read_holding_registers(socket_id, addr_currents, 6)
            .await
            .ok();

        let power_regs = manager
            .read_holding_registers(socket_id, addr_power, 8)
            .await
            .ok();

        let energy_regs = manager
            .read_holding_registers(socket_id, addr_energy, 4)
            .await
            .ok();

        let status_regs = manager
            .read_holding_registers(socket_id, addr_status, 5)
            .await
            .ok();

        self.update_station_max_current_from_modbus().await;

        let voltages_triplet = Self::decode_triplet(&voltages);
        let currents_triplet = Self::decode_triplet(&currents);
        let (powers_triplet, total_power) =
            Self::decode_powers(&power_regs, &voltages_triplet, &currents_triplet);
        let energy_kwh = Self::decode_energy_kwh(&energy_regs);
        let status = Self::compute_status_from_regs(&status_regs);

        RealtimeMeasurements {
            voltages: voltages_triplet,
            currents: currents_triplet,
            powers: powers_triplet,
            total_power,
            energy_kwh,
            status,
        }
    }

    fn ev_power_for_subtract(&self, p_total: f64) -> f64 {
        let lag_ms = self.config.controls.ev_reporting_lag_ms as u128;
        if self.last_set_current_monotonic.elapsed().as_millis() < lag_ms {
            let phases = 3.0f64;
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
            )
            .await
            .unwrap_or(0.0);

        let mut soc_below_min: Option<bool> = None;
        if matches!(self.start_stop, crate::controls::StartStopState::Enabled)
            && (matches!(self.current_mode, crate::controls::ChargingMode::Auto)
                || matches!(self.current_mode, crate::controls::ChargingMode::Scheduled))
            && let Some((soc, min_limit)) = self.fetch_battery_soc_and_minimum_limit().await
            && soc.is_finite()
            && min_limit.is_finite()
        {
            soc_below_min = Some(soc < min_limit);
            if soc < min_limit
                && !matches!(self.current_mode, crate::controls::ChargingMode::Manual)
            {
                if effective > 0.0 {
                    self.logger.info(&format!(
                        "Stopping charging due to Low SOC: SoC={:.1}% < MinimumSocLimit={:.1}%",
                        soc, min_limit
                    ));
                }
                effective = 0.0;
            }
        }
        (effective, soc_below_min)
    }

    fn apply_current_if_needed(
        &mut self,
        effective: f32,
        excess_pv_power_w: f32,
    ) -> (bool, bool, bool) {
        let (should_update, need_change, interval_due) = self.should_send_update(effective);
        if should_update {
            if need_change {
                let reason = self.current_mode_reason();
                self.logger.info(&format!(
                    "Adjusting available current: {:.2} A -> {:.2} A (reason={}, pv_excess={:.0} W, station_max={:.1} A)",
                    self.last_sent_current, effective, reason, excess_pv_power_w, self.station_max_current
                ));
            } else if interval_due {
                let reason = self.current_mode_reason();
                self.logger.info(&format!(
                    "Reasserting available current: {:.2} A (reason={}, pv_excess={:.0} W, station_max={:.1} A)",
                    effective, reason, excess_pv_power_w, self.station_max_current
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

            let ev_power_for_subtract = self.ev_power_for_subtract(m.total_power);
            let excess_pv_power_w: f32 = self
                .calculate_excess_pv_power(ev_power_for_subtract)
                .await
                .unwrap_or(0.0);
            let (effective, soc_below_min) = self
                .compute_effective_current_with_soc(requested, now_secs, excess_pv_power_w)
                .await;

            let (should_update, _need_change, _interval_due) =
                self.apply_current_if_needed(effective, excess_pv_power_w);
            if should_update {
                if self.write_effective_current(effective).await {
                    self.last_sent_current = effective;
                    self.last_current_set_time = std::time::Instant::now();
                    self.last_set_current_monotonic = std::time::Instant::now();
                } else {
                    self.logger.warn("Failed to write set current via Modbus");
                }
            }

            // Derive final status from base status and context
            let derived_status = self.derive_status(m.status, soc_below_min) as u8;
            self.finalize_cycle(&m, derived_status, effective)?;
        }

        self.logger.debug("Poll cycle completed");
        let snapshot = Arc::new(self.build_typed_snapshot(Some(self.last_poll_duration_ms())));
        let _ = self.status_snapshot_tx.send(snapshot);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
