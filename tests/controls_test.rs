use chrono::{Datelike, Utc};
use phaeton::config::{Config, ScheduleItem};
use phaeton::controls::{ChargingControls, ChargingMode, StartStopState};

fn base_config() -> Config {
    Config {
        timezone: "UTC".to_string(),
        ..Default::default()
    }
}

#[tokio::test]
async fn manual_mode_respects_limits() {
    let controls = ChargingControls::new();
    let cfg = base_config();
    let amps = controls
        .compute_effective_current(
            ChargingMode::Manual,
            StartStopState::Enabled,
            40.0,
            32.0,
            0.0,
            Some(0.0),
            &cfg,
            3,
        )
        .await
        .unwrap();
    assert!((amps - 32.0).abs() < f32::EPSILON);
}

#[tokio::test]
async fn stopped_returns_zero() {
    let controls = ChargingControls::new();
    let cfg = base_config();
    let amps = controls
        .compute_effective_current(
            ChargingMode::Manual,
            StartStopState::Stopped,
            16.0,
            32.0,
            0.0,
            Some(0.0),
            &cfg,
            3,
        )
        .await
        .unwrap();
    assert_eq!(amps, 0.0);
}

#[tokio::test]
async fn auto_mode_converts_solar_power() {
    let controls = ChargingControls::new();
    let cfg = base_config();
    // 6900 W at 230V three-phase ~ 10 A
    let amps = controls
        .compute_effective_current(
            ChargingMode::Auto,
            StartStopState::Enabled,
            0.0,
            32.0,
            0.0,
            Some(6900.0),
            &cfg,
            3,
        )
        .await
        .unwrap();
    assert!((amps - 10.0).abs() < 0.05);
}

#[tokio::test]
async fn scheduled_mode_respects_schedule() {
    let controls = ChargingControls::new();
    let mut cfg = base_config();
    let weekday = Utc::now().weekday().num_days_from_monday() as u8;
    cfg.schedule.items = vec![ScheduleItem {
        active: true,
        days: vec![weekday],
        start_time: "00:00".to_string(),
        end_time: "23:59".to_string(),
        enabled: 1,
        days_mask: 0,
        start: "".to_string(),
        end: "".to_string(),
    }];

    let amps = controls
        .compute_effective_current(
            ChargingMode::Scheduled,
            StartStopState::Enabled,
            6.0,
            25.0,
            0.0,
            Some(0.0),
            &cfg,
            3,
        )
        .await
        .unwrap();
    assert_eq!(amps, 25.0);

    // Now set schedule for a different day so it should be off
    let other_day = (weekday + 1) % 7;
    cfg.schedule.items[0].days = vec![other_day];
    let amps_off = controls
        .compute_effective_current(
            ChargingMode::Scheduled,
            StartStopState::Enabled,
            6.0,
            25.0,
            0.0,
            Some(0.0),
            &cfg,
            3,
        )
        .await
        .unwrap();
    assert_eq!(amps_off, 0.0);
}
