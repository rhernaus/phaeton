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
            .execute_with_reconnect(|client| {
                Box::pin(async move {
                    client
                        .read_holding_registers(socket_id, addr_voltages, 6)
                        .await
                })
            })
            .await
            .ok();

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
            .execute_with_reconnect(|client| {
                let regs_vec = vec![regs[0], regs[1]];
                Box::pin(async move {
                    client
                        .write_multiple_registers(socket_id, addr_amps, &regs_vec)
                        .await
                })
            })
            .await;
        write_res.is_ok()
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

    #[allow(clippy::cognitive_complexity)]
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

            // Fetch SoC once for both clamping and status derivation
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
            let cur_status = derived_status;
            self.handle_session_transition(cur_status, m.energy_kwh);

            self.sessions.update(m.total_power, m.energy_kwh)?;
            self.persist_state();
            self.update_last_measurements(&m);

            self.logger.debug(&format!(
                "V=({:.1},{:.1},{:.1})V I=({:.2},{:.2},{:.2})A P=({:.0},{:.0},{:.0})W total={:.0}W E={:.3}kWh status={} lag_ms={} last_sent_A={:.2}",
                m.voltages.l1, m.voltages.l2, m.voltages.l3, m.currents.l1, m.currents.l2, m.currents.l3, m.powers.l1, m.powers.l2, m.powers.l3, m.total_power, m.energy_kwh, cur_status,
                self.last_set_current_monotonic.elapsed().as_millis(), self.last_sent_current
            ));

            let _ = self
                .status_tx
                .send(self.build_status_json(effective, m.total_power));
        }

        self.logger.debug("Poll cycle completed");
        let snapshot = Arc::new(self.build_typed_snapshot(Some(self.last_poll_duration_ms())));
        let _ = self.status_snapshot_tx.send(snapshot);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    // helper kept if needed later

    #[test]
    fn decode_triplet_handles_none_and_short() {
        // None -> zeros
        let t = crate::driver::AlfenDriver::decode_triplet(&None);
        assert_eq!((t.l1, t.l2, t.l3), (0.0, 0.0, 0.0));
        // Too short -> zeros
        let regs = Some(vec![0u16; 4]);
        let t2 = crate::driver::AlfenDriver::decode_triplet(&regs);
        assert_eq!((t2.l1, t2.l2, t2.l3), (0.0, 0.0, 0.0));
    }

    #[test]
    fn decode_triplet_parses_values() {
        // Three f32 values: 230.0, 231.5, 229.4
        let a = 230.0f32.to_be_bytes();
        let b = 231.5f32.to_be_bytes();
        let c = 229.4f32.to_be_bytes();
        let regs = vec![
            ((a[0] as u16) << 8) | a[1] as u16,
            ((a[2] as u16) << 8) | a[3] as u16,
            ((b[0] as u16) << 8) | b[1] as u16,
            ((b[2] as u16) << 8) | b[3] as u16,
            ((c[0] as u16) << 8) | c[1] as u16,
            ((c[2] as u16) << 8) | c[3] as u16,
        ];
        let t = crate::driver::AlfenDriver::decode_triplet(&Some(regs));
        assert!((t.l1 - 230.0).abs() < 0.01);
        assert!((t.l2 - 231.5).abs() < 0.01);
        assert!((t.l3 - 229.4).abs() < 0.01);
    }

    #[test]
    fn decode_energy_kwh_handles_inputs() {
        // None -> 0
        assert_eq!(crate::driver::AlfenDriver::decode_energy_kwh(&None), 0.0);
        // Too short -> 0
        assert_eq!(
            crate::driver::AlfenDriver::decode_energy_kwh(&Some(vec![0u16; 2])),
            0.0
        );
        // Valid 64-bit float (e.g., 1234.0 Wh -> 1.234 kWh after division)
        let val: f64 = 1234.0;
        let be = val.to_be_bytes();
        let regs = vec![
            ((be[0] as u16) << 8) | be[1] as u16,
            ((be[2] as u16) << 8) | be[3] as u16,
            ((be[4] as u16) << 8) | be[5] as u16,
            ((be[6] as u16) << 8) | be[7] as u16,
        ];
        let kwh = crate::driver::AlfenDriver::decode_energy_kwh(&Some(regs));
        assert!((kwh - 1.234).abs() < 1e-9);
    }

    #[test]
    fn decode_powers_approximates_when_small() {
        // Provide near-zero power regs so approximation kicks in using v*i rounded
        let p_regs = Some(vec![0u16; 8]);
        let voltages = LineTriplet {
            l1: 230.0,
            l2: 231.0,
            l3: 229.0,
        };
        let currents = LineTriplet {
            l1: 5.0,
            l2: 6.0,
            l3: 7.0,
        };
        let (p_triplet, total) =
            crate::driver::AlfenDriver::decode_powers(&p_regs, &voltages, &currents);
        assert_eq!(p_triplet.l1, (230.0_f64 * 5.0_f64).round());
        assert_eq!(p_triplet.l2, (231.0_f64 * 6.0_f64).round());
        assert_eq!(p_triplet.l3, (229.0_f64 * 7.0_f64).round());
        assert_eq!(total, p_triplet.l1 + p_triplet.l2 + p_triplet.l3);
    }

    #[test]
    fn compute_status_from_regs_maps_strings() {
        // Build registers for string "C2\0\0\0\0"
        let regs = vec![0x4332, 0x0000, 0x0000, 0x0000, 0x0000];
        let s = crate::driver::AlfenDriver::compute_status_from_regs(&Some(regs));
        assert_eq!(s, 2);
        // "B1" -> 1
        let regs_b1 = vec![0x4231, 0, 0, 0, 0];
        assert_eq!(
            crate::driver::AlfenDriver::compute_status_from_regs(&Some(regs_b1)),
            1
        );
        // Unknown -> 0
        let regs_xx = vec![0x5858, 0, 0, 0, 0];
        assert_eq!(
            crate::driver::AlfenDriver::compute_status_from_regs(&Some(regs_xx)),
            0
        );
    }

    #[tokio::test]
    async fn derive_status_variants() {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut d = crate::driver::AlfenDriver::new(rx, tx).await.unwrap();

        // Base 1 (connected) with Stopped -> 6
        d.start_stop = crate::controls::StartStopState::Stopped;
        d.current_mode = crate::controls::ChargingMode::Manual;
        d.last_sent_current = 0.0;
        assert_eq!(d.derive_status(1, None), 6);

        // Auto mode with near-zero last current -> 4
        d.start_stop = crate::controls::StartStopState::Enabled;
        d.current_mode = crate::controls::ChargingMode::Auto;
        d.last_sent_current = 0.05;
        assert_eq!(d.derive_status(1, None), 4);

        // Low SOC overrides to 7 for Auto
        assert_eq!(d.derive_status(1, Some(true)), 7);

        // Scheduled inactive window -> 6
        d.current_mode = crate::controls::ChargingMode::Scheduled;
        // Schedule inactive is handled inside derive (using config); we cannot easily force it here
        // but Low SOC should still override to 7
        assert_eq!(d.derive_status(1, Some(true)), 7);
    }

    #[tokio::test]
    async fn ev_power_for_subtract_and_should_send_update() {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut d = crate::driver::AlfenDriver::new(rx, tx).await.unwrap();

        d.last_sent_current = 10.0; // A
        d.last_set_current_monotonic = std::time::Instant::now();
        // Within lag -> compute from last current
        let ev_sub = d.ev_power_for_subtract(1234.0);
        assert!(ev_sub >= 10.0 * 230.0 * 3.0 - 1.0);

        // Force watchdog satisfied
        d.last_current_set_time = std::time::Instant::now()
            - std::time::Duration::from_millis(
                d.config.controls.current_update_interval as u64 + 10,
            );
        // Need change large enough
        d.last_sent_current = 10.0;
        let (should, need_change, _) = d.should_send_update(10.3);
        assert!(should && need_change);

        // No change, but watchdog still triggers
        let (should2, need_change2, _) = d.should_send_update(10.05);
        assert!(should2 && !need_change2);
    }

    #[tokio::test]
    async fn current_mode_reason_strings() {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut d = crate::driver::AlfenDriver::new(rx, tx).await.unwrap();
        d.current_mode = crate::controls::ChargingMode::Manual;
        assert_eq!(d.current_mode_reason(), "manual");
        d.current_mode = crate::controls::ChargingMode::Auto;
        assert_eq!(d.current_mode_reason(), "pv_auto");
        d.current_mode = crate::controls::ChargingMode::Scheduled;
        assert_eq!(d.current_mode_reason(), "scheduled");
    }
}
