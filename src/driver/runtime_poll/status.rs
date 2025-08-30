impl crate::driver::AlfenDriver {
    /// Derive Victron-esque status from base hardware status and current context.
    ///
    /// Rule order (highest precedence first):
    /// - StartStop=Stopped -> 6 (Wait start)
    /// - Scheduled mode with inactive window -> 6 (Wait start)
    /// - Auto or Scheduled with Low SoC -> 7 (Low SOC)
    /// - Auto with near-zero current -> 4 (Wait sun)
    /// - Fallback to base (0/1/2)
    pub(super) fn derive_status(&self, status_base: i32, soc_below_min: Option<bool>) -> i32 {
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
}
