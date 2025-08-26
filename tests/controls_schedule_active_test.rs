use chrono::{Datelike, Timelike, Utc};
use phaeton::config::{Config, ScheduleItem};
use phaeton::controls::ChargingControls;

fn hhmm_from_minutes(total_minutes: i32) -> String {
    let m = total_minutes.rem_euclid(1440);
    let hh = m / 60;
    let mm = m % 60;
    format!("{:02}:{:02}", hh, mm)
}

#[test]
fn schedule_active_overnight_branch_is_true() {
    let mut cfg = Config {
        timezone: "UTC".to_string(),
        ..Default::default()
    };

    // Build an overnight window that is active at 'now' regardless of crossing midnight.
    // Include both today and the previous day to satisfy the implementation's day filter.
    let now = Utc::now();
    let minutes_now = (now.hour() * 60 + now.minute()) as i32;
    let start = hhmm_from_minutes(minutes_now - 10);
    let end = hhmm_from_minutes(minutes_now - 20);

    let weekday = now.weekday().num_days_from_monday() as u8;
    let prev_day = (weekday + 6) % 7;
    cfg.schedule.items = vec![ScheduleItem {
        active: true,
        days: vec![weekday, prev_day],
        start_time: start,
        end_time: end,
        enabled: 1,
        days_mask: 0,
        start: String::new(),
        end: String::new(),
    }];

    assert!(ChargingControls::is_schedule_active(&cfg));
}
