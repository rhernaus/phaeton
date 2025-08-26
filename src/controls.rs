//! Charging control algorithms for Phaeton
//!
//! This module contains the business logic for different charging modes
//! including manual, automatic, and scheduled charging strategies.

use crate::error::Result;
use crate::logging::get_logger;
use chrono::{Datelike, Timelike, Utc};
use chrono_tz::Tz;

/// Charging mode enumeration
#[derive(Debug, Clone, Copy)]
pub enum ChargingMode {
    /// Manual control - user sets current directly
    Manual = 0,

    /// Automatic control - solar-optimized charging
    Auto = 1,

    /// Scheduled control - time-based charging
    Scheduled = 2,
}

/// Start/stop state enumeration
#[derive(Debug, Clone, Copy)]
pub enum StartStopState {
    /// Charging stopped
    Stopped = 0,

    /// Charging enabled
    Enabled = 1,
}

/// Charging control system
pub struct ChargingControls {
    #[allow(dead_code)]
    logger: crate::logging::StructuredLogger,
}

impl ChargingControls {
    /// Create new charging controls
    pub fn new() -> Self {
        let logger = get_logger("controls");
        Self { logger }
    }
}

impl Default for ChargingControls {
    fn default() -> Self {
        Self::new()
    }
}

impl ChargingControls {
    /// Compute effective current based on mode and conditions
    #[allow(clippy::too_many_arguments)]
    pub async fn compute_effective_current(
        &self,
        mode: ChargingMode,
        start_stop: StartStopState,
        requested_current: f32,
        station_max_current: f32,
        _current_time: f64,
        solar_power: Option<f32>,
        config: &crate::config::Config,
    ) -> Result<f32> {
        if matches!(start_stop, StartStopState::Stopped) {
            return Ok(0.0);
        }

        let effective = match mode {
            ChargingMode::Manual => requested_current.min(station_max_current),
            ChargingMode::Auto => {
                // Interpret solar_power as (smoothed) excess Watts available for charging.
                // Convert Watts to Amps using nominal 230V per phase and assume 3 phases.
                let excess_watts = solar_power.unwrap_or(0.0).max(0.0);
                let nominal_voltage = 230.0f32;
                let phases = 3.0f32; // TODO: detect active phases from charger
                let amps_raw = excess_watts / (phases * nominal_voltage);
                // Below EVSE minimum current we should not oscillate with tiny setpoints.
                // If below min_set_current, clamp to exactly 0.0 unless already above threshold.
                let min_current = config.controls.min_set_current.max(0.0);
                let amps = if amps_raw < min_current {
                    0.0
                } else {
                    amps_raw
                };
                amps.min(station_max_current)
            }
            ChargingMode::Scheduled => {
                if Self::is_within_any_schedule(config) {
                    station_max_current
                } else {
                    0.0
                }
            }
        };

        Ok(effective)
    }

    /// Synchronous wrapper for non-async control paths (same logic)
    #[allow(clippy::too_many_arguments)]
    pub fn blocking_compute_effective_current(
        &self,
        mode: ChargingMode,
        start_stop: StartStopState,
        requested_current: f32,
        station_max_current: f32,
        _current_time: f64,
        solar_power: Option<f32>,
        config: &crate::config::Config,
    ) -> Result<f32> {
        if matches!(start_stop, StartStopState::Stopped) {
            return Ok(0.0);
        }
        let effective = match mode {
            ChargingMode::Manual => requested_current.min(station_max_current),
            ChargingMode::Auto => {
                let excess_watts = solar_power.unwrap_or(0.0).max(0.0);
                let nominal_voltage = 230.0f32;
                let phases = 3.0f32;
                let amps_raw = excess_watts / (phases * nominal_voltage);
                let min_current = config.controls.min_set_current.max(0.0);
                let amps = if amps_raw < min_current {
                    0.0
                } else {
                    amps_raw
                };
                amps.min(station_max_current)
            }
            ChargingMode::Scheduled => {
                if Self::is_within_any_schedule(config) {
                    station_max_current
                } else {
                    0.0
                }
            }
        };
        Ok(effective)
    }

    /// Apply current setting to charger
    pub async fn apply_current(&self, _current: f32, _explanation: &str) -> Result<bool> {
        // TODO: Implement actual current setting via Modbus
        Ok(true)
    }

    fn is_within_any_schedule(config: &crate::config::Config) -> bool {
        let tz: Tz = config
            .timezone
            .parse()
            .unwrap_or_else(|_| "UTC".parse().unwrap());
        let now_utc = Utc::now();
        let now_local = now_utc.with_timezone(&tz);
        let weekday = now_local.weekday().num_days_from_monday() as u8; // 0..6
        let minutes_now = now_local.hour() * 60 + now_local.minute();

        for item in &config.schedule.items {
            if !item.active {
                continue;
            }
            if !item.days.is_empty() && !item.days.contains(&weekday) {
                continue;
            }
            let start_min = Self::parse_hhmm(&item.start_time);
            let end_min = Self::parse_hhmm(&item.end_time);
            if start_min == end_min {
                continue;
            }
            let overnight = start_min >= end_min;
            let within = if overnight {
                minutes_now >= start_min || minutes_now < end_min
            } else {
                minutes_now >= start_min && minutes_now < end_min
            };
            if within {
                return true;
            }
        }
        false
    }

    fn parse_hhmm(s: &str) -> u32 {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 2 {
            return 0;
        }
        let h = parts[0].parse::<u32>().unwrap_or(0) % 24;
        let m = parts[1].parse::<u32>().unwrap_or(0) % 60;
        h * 60 + m
    }

    /// Public helper to check if any schedule window is currently active
    pub fn is_schedule_active(config: &crate::config::Config) -> bool {
        Self::is_within_any_schedule(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hhmm() {
        assert_eq!(ChargingControls::parse_hhmm("08:30"), 8 * 60 + 30);
        assert_eq!(ChargingControls::parse_hhmm("23:59"), 23 * 60 + 59);
        assert_eq!(ChargingControls::parse_hhmm("24:00"), 0);
        assert_eq!(ChargingControls::parse_hhmm("bad"), 0);
    }
}
