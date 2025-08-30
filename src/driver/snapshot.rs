use super::types::DriverSnapshot;

impl super::AlfenDriver {
    fn derive_phase_count_for_snapshot(&self) -> u8 {
        if self.applied_phases >= 3 { 3 } else { 1 }
    }

    fn compute_ac_current_for_snapshot(&self) -> f64 {
        self.last_l1_current
            .max(self.last_l2_current.max(self.last_l3_current))
    }

    fn compute_pricing_currency_for_snapshot(&self) -> Option<String> {
        Some(self.config().pricing.currency_symbol.clone()).filter(|sym| !sym.is_empty())
    }

    fn compute_energy_rate_for_snapshot(&self) -> Option<f64> {
        if self.config().pricing.source.to_lowercase() == "static" {
            Some(self.config().pricing.static_rate_eur_per_kwh)
        } else {
            None
        }
    }

    fn build_session_value_for_snapshot(&self) -> serde_json::Value {
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
    }

    pub fn subscribe_snapshot(
        &self,
    ) -> tokio::sync::watch::Receiver<std::sync::Arc<DriverSnapshot>> {
        self.status_snapshot_rx.clone()
    }

    pub(super) fn build_typed_snapshot(&self, poll_duration_ms: Option<u64>) -> DriverSnapshot {
        // Reflect configured phase count immediately and use helpers for clarity
        let phase_count = self.derive_phase_count_for_snapshot();
        let ac_current = self.compute_ac_current_for_snapshot();
        let pricing_currency = self.compute_pricing_currency_for_snapshot();
        let energy_rate = self.compute_energy_rate_for_snapshot();
        let session = self.build_session_value_for_snapshot();

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
            modbus_connected: self
                .modbus_manager
                .as_ref()
                .and_then(|m| m.connection_status()),
            driver_state: match self.get_state() {
                super::types::DriverState::Initializing => "Initializing".to_string(),
                super::types::DriverState::Running => "Running".to_string(),
                super::types::DriverState::Error(_) => "Error".to_string(),
                super::types::DriverState::ShuttingDown => "ShuttingDown".to_string(),
            },
            poll_steps_ms: self.last_poll_steps.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn build_typed_snapshot_populates_core_fields() {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut d = crate::driver::AlfenDriver::new(rx, tx).await.unwrap();

        // Seed some last measurements
        d.last_l1_voltage = 230.0;
        d.last_l2_voltage = 231.0;
        d.last_l3_voltage = 229.0;
        d.last_l1_current = 5.0;
        d.last_l2_current = 6.0;
        d.last_l3_current = 7.0;
        d.last_l1_power = 1100.0;
        d.last_l2_power = 1200.0;
        d.last_l3_power = 1300.0;
        d.last_total_power = 3600.0;
        d.last_energy_kwh = 12.345;
        d.last_sent_current = 6.5;
        d.product_name = Some("Alfen EV Charger".to_string());
        d.firmware_version = Some("1.2.3".to_string());
        d.serial = Some("ABC".to_string());

        let snap = d.build_typed_snapshot(Some(10));
        assert_eq!(snap.device_instance, d.config().device_instance);
        assert_eq!(snap.station_max_current, d.get_station_max_current());
        assert!((snap.ac_power - 3600.0).abs() < 0.001);
        assert!(snap.active_phases >= 1);
        assert_eq!(snap.poll_duration_ms, Some(10));
        assert_eq!(snap.product_name, Some("Alfen EV Charger".to_string()));
        assert_eq!(snap.firmware, Some("1.2.3".to_string()));
        assert_eq!(snap.serial, Some("ABC".to_string()));
    }
}
