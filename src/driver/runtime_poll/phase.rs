impl crate::driver::AlfenDriver {
    pub(super) async fn evaluate_auto_phase_switch(&mut self, excess_pv_power_w: f32) {
        // If currently settling after a switch, do nothing until deadline
        if let Some(deadline) = self.phase_settle_deadline {
            if std::time::Instant::now() < deadline {
                return;
            }
            self.phase_settle_deadline = None;
        }

        // Respect minimum time between switches
        if let Some(last) = self.last_phase_switch {
            let min_gap = std::time::Duration::from_secs(
                self.config.controls.phase_switch_grace_seconds as u64,
            );
            if std::time::Instant::now().duration_since(last) < min_gap {
                return;
            }
        }

        // Compute thresholds based on configured min/max and 230V
        let v = 230.0f32;
        let min_a = self.config.controls.min_set_current.max(0.0);
        let max_a = self.config.controls.max_set_current.max(min_a);
        let hys = self.config.controls.auto_phase_hysteresis_watts.max(0.0);

        let one_p_max_w = max_a * v * 1.0;
        let three_p_min_w = min_a * v * 3.0;

        let want_three = excess_pv_power_w > (three_p_min_w + hys);
        let want_one = excess_pv_power_w < (one_p_max_w - hys);

        let current = if self.applied_phases >= 3 { 3 } else { 1 };
        let target = if current == 1 {
            // consider upswitching if comfortably above 3P min
            if want_three { 3 } else { 1 }
        } else {
            // consider downswitching if comfortably below 1P max
            if want_one { 1 } else { 3 }
        };

        if target != current {
            let _ = self.apply_phases_now(target).await;
        }
    }
}
