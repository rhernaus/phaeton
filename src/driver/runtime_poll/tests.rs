use super::*;
use tokio::sync::mpsc;

#[test]
fn decode_triplet_handles_none_and_short() {
    let t = crate::driver::AlfenDriver::decode_triplet(&None);
    assert_eq!((t.l1, t.l2, t.l3), (0.0, 0.0, 0.0));
    let regs = Some(vec![0u16; 4]);
    let t2 = crate::driver::AlfenDriver::decode_triplet(&regs);
    assert_eq!((t2.l1, t2.l2, t2.l3), (0.0, 0.0, 0.0));
}

#[test]
fn decode_triplet_parses_values() {
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
    assert_eq!(crate::driver::AlfenDriver::decode_energy_kwh(&None), 0.0);
    assert_eq!(
        crate::driver::AlfenDriver::decode_energy_kwh(&Some(vec![0u16; 2])),
        0.0
    );
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
    let regs = vec![0x4332, 0x0000, 0x0000, 0x0000, 0x0000];
    let s = crate::driver::AlfenDriver::compute_status_from_regs(&Some(regs));
    assert_eq!(s, 2);
    let regs_b1 = vec![0x4231, 0, 0, 0, 0];
    assert_eq!(
        crate::driver::AlfenDriver::compute_status_from_regs(&Some(regs_b1)),
        1
    );
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

    d.start_stop = crate::controls::StartStopState::Stopped;
    d.current_mode = crate::controls::ChargingMode::Manual;
    d.last_sent_current = 0.0;
    assert_eq!(d.derive_status(1, None), 6);

    d.start_stop = crate::controls::StartStopState::Enabled;
    d.current_mode = crate::controls::ChargingMode::Auto;
    d.last_sent_current = 0.05;
    assert_eq!(d.derive_status(1, None), 4);

    assert_eq!(d.derive_status(1, Some(true)), 7);

    d.current_mode = crate::controls::ChargingMode::Scheduled;
    assert_eq!(d.derive_status(1, Some(true)), 7);
}

#[tokio::test]
async fn ev_power_for_subtract_and_should_send_update() {
    let (tx, rx) = mpsc::unbounded_channel();
    let mut d = crate::driver::AlfenDriver::new(rx, tx).await.unwrap();

    d.last_sent_current = 10.0;
    d.last_set_current_monotonic = std::time::Instant::now();
    let ev_sub = d.ev_power_for_subtract(1234.0);
    assert!(ev_sub >= 10.0 * 230.0 * 3.0 - 1.0);

    d.last_current_set_time = std::time::Instant::now()
        - std::time::Duration::from_millis(d.config.controls.current_update_interval as u64 + 10);
    d.last_sent_current = 10.0;
    let (should, need_change, _) = d.should_send_update(10.3);
    assert!(should && need_change);

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
