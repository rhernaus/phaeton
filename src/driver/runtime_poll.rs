use crate::error::Result;
use std::sync::Arc;

impl super::AlfenDriver {
    #[allow(clippy::cognitive_complexity)]
    pub(crate) async fn poll_cycle(&mut self) -> Result<()> {
        self.logger.debug("Starting poll cycle");
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

                let voltages = manager
                    .execute_with_reconnect(|client| {
                        Box::pin(async move {
                            client.read_holding_registers(socket_id, addr_voltages, 6).await
                        })
                    })
                    .await
                    .ok();

                let currents = manager
                    .execute_with_reconnect(|client| {
                        Box::pin(async move {
                            client.read_holding_registers(socket_id, addr_currents, 6).await
                        })
                    })
                    .await
                    .ok();

                let power_regs = manager
                    .execute_with_reconnect(|client| {
                        Box::pin(async move {
                            client.read_holding_registers(socket_id, addr_power, 8).await
                        })
                    })
                    .await
                    .ok();

                let energy_regs = manager
                    .execute_with_reconnect(|client| {
                        Box::pin(async move {
                            client.read_holding_registers(socket_id, addr_energy, 4).await
                        })
                    })
                    .await
                    .ok();

                let status_regs = manager
                    .execute_with_reconnect(|client| {
                        Box::pin(async move {
                            client.read_holding_registers(socket_id, addr_status, 5).await
                        })
                    })
                    .await
                    .ok();

                if let Ok(max_regs) = manager
                    .execute_with_reconnect(|client| {
                        Box::pin(async move {
                            client.read_holding_registers(station_id, addr_station_max, 2).await
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
                let mut status = status_base;
                let connected = status_base == 1 || status_base == 2;
                if connected {
                    if matches!(self.start_stop, crate::controls::StartStopState::Stopped) {
                        status = 6;
                    } else if matches!(self.current_mode, crate::controls::ChargingMode::Auto) {
                        if self.last_sent_current < 0.1 {
                            status = 4;
                        }
                    } else if matches!(self.current_mode, crate::controls::ChargingMode::Scheduled)
                        && !crate::controls::ChargingControls::is_schedule_active(&self.config)
                    {
                        status = 6;
                    }
                }

                (l1_v, l2_v, l3_v, l1_i, l2_i, l3_i, l1_p, l2_p, l3_p, p_total, energy_kwh, status)
            };

            let now_secs = (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default())
            .as_secs_f64();
            let requested = self.intended_set_current;

            let ev_power_for_subtract = {
                let lag_ms = self.config.controls.ev_reporting_lag_ms as u128;
                if self.last_set_current_monotonic.elapsed().as_millis() < lag_ms {
                    let phases = 3.0f64;
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

            let prev_status = self.last_status;
            let cur_status = status as u8;
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

            self.sessions.update(p_total, energy_kwh)?;

            self.persistence.set_mode(self.current_mode as u32);
            self.persistence.set_start_stop(self.start_stop as u32);
            self.persistence.set_set_current(self.intended_set_current);
            let _ = self
                .persistence
                .set_section("session", self.sessions.get_state());
            let _ = self.persistence.save();

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

            self.logger.debug(&format!(
                "V=({:.1},{:.1},{:.1})V I=({:.2},{:.2},{:.2})A P=({:.0},{:.0},{:.0})W total={:.0}W E={:.3}kWh status={} lag_ms={} last_sent_A={:.2}",
                l1_v, l2_v, l3_v, l1_i, l2_i, l3_i, l1_p, l2_p, l3_p, p_total, energy_kwh, status,
                self.last_set_current_monotonic.elapsed().as_millis(), self.last_sent_current
            ));

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
            let _ = self.status_tx.send(status_obj.to_string());
        }

        self.logger.debug("Poll cycle completed");
        let snapshot = Arc::new(self.build_typed_snapshot(Some(self.last_poll_duration_ms())));
        let _ = self.status_snapshot_tx.send(snapshot);
        Ok(())
    }
}
