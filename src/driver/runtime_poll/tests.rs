use super::*;
use std::collections::HashMap;
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

struct MockModbus {
    reads: HashMap<(u8, u16, u16), Vec<u16>>,
    write_ok: bool,
    last_write: Option<(u8, u16, Vec<u16>)>,
}

impl MockModbus {
    fn new() -> Self {
        Self {
            reads: HashMap::new(),
            write_ok: true,
            last_write: None,
        }
    }
    fn with_read(mut self, slave: u8, addr: u16, count: u16, data: Vec<u16>) -> Self {
        self.reads.insert((slave, addr, count), data);
        self
    }
}

#[async_trait::async_trait]
impl crate::driver::modbus_like::ModbusLike for MockModbus {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    async fn read_holding_registers(
        &mut self,
        slave_id: u8,
        address: u16,
        count: u16,
    ) -> crate::error::Result<Vec<u16>> {
        Ok(self
            .reads
            .get(&(slave_id, address, count))
            .cloned()
            .unwrap_or_default())
    }

    async fn write_multiple_registers(
        &mut self,
        slave_id: u8,
        address: u16,
        values: &[u16],
    ) -> crate::error::Result<()> {
        self.last_write = Some((slave_id, address, values.to_vec()));
        if self.write_ok {
            Ok(())
        } else {
            Err(crate::error::PhaetonError::modbus("mock write error"))
        }
    }
}

fn regs_from_f32(v: f32) -> Vec<u16> {
    crate::modbus::encode_32bit_float(v).to_vec()
}

fn regs_from_f64(v: f64) -> Vec<u16> {
    let be = v.to_be_bytes();
    vec![
        ((be[0] as u16) << 8) | be[1] as u16,
        ((be[2] as u16) << 8) | be[3] as u16,
        ((be[4] as u16) << 8) | be[5] as u16,
        ((be[6] as u16) << 8) | be[7] as u16,
    ]
}

#[tokio::test]
async fn update_station_max_current_reads_and_sets_value() {
    let (tx, rx) = mpsc::unbounded_channel();
    let mut d = crate::driver::AlfenDriver::new(rx, tx).await.unwrap();
    let regs = regs_from_f32(32.5);
    let mock = MockModbus::new().with_read(
        d.config().modbus.station_slave_id,
        d.config().registers.station_max_current,
        2,
        regs,
    );
    d.modbus_manager = Some(Box::new(mock));
    d.station_max_current = 0.0;
    d.update_station_max_current_from_modbus().await;
    assert!((d.get_station_max_current() - 32.5).abs() < f32::EPSILON);
}

#[tokio::test]
async fn read_realtime_values_decodes_all_fields() {
    let (tx, rx) = mpsc::unbounded_channel();
    let mut d = crate::driver::AlfenDriver::new(rx, tx).await.unwrap();
    let cfg = d.config().clone();
    let volt_regs = [230.0f32, 231.0, 229.0]
        .into_iter()
        .flat_map(regs_from_f32)
        .collect::<Vec<_>>();
    let curr_regs = [6.0f32, 7.0, 8.0]
        .into_iter()
        .flat_map(regs_from_f32)
        .collect::<Vec<_>>();
    let power_regs = [1200.0f32, 1300.0, 1400.0, 3900.0]
        .into_iter()
        .flat_map(regs_from_f32)
        .collect::<Vec<_>>();
    let energy_regs = regs_from_f64(1234.0); // becomes 1.234 kWh
    let status_regs = vec![0x4332, 0, 0, 0, 0]; // "C2\0\0\0\0"
    let mock = MockModbus::new()
        .with_read(
            cfg.modbus.socket_slave_id,
            cfg.registers.voltages,
            6,
            volt_regs,
        )
        .with_read(
            cfg.modbus.socket_slave_id,
            cfg.registers.currents,
            6,
            curr_regs,
        )
        .with_read(
            cfg.modbus.socket_slave_id,
            cfg.registers.power,
            8,
            power_regs,
        )
        .with_read(
            cfg.modbus.socket_slave_id,
            cfg.registers.energy,
            4,
            energy_regs,
        )
        .with_read(
            cfg.modbus.socket_slave_id,
            cfg.registers.status,
            5,
            status_regs,
        );
    d.modbus_manager = Some(Box::new(mock));
    let m = d.read_realtime_values().await;
    assert!((m.voltages.l1 - 230.0).abs() < 0.01);
    assert!((m.currents.l3 - 8.0).abs() < 0.01);
    assert_eq!(m.powers.l2.round() as i64, 1300);
    assert_eq!(m.total_power.round() as i64, 3900);
    assert!((m.energy_kwh - 1.234).abs() < 1e-9);
    assert_eq!(m.status, 2);
}

#[tokio::test]
async fn write_effective_current_encodes_and_writes() {
    let (tx, rx) = mpsc::unbounded_channel();
    let mut d = crate::driver::AlfenDriver::new(rx, tx).await.unwrap();
    let mut mock = MockModbus::new();
    mock.write_ok = true;
    let cfg = d.config().clone();
    d.modbus_manager = Some(Box::new(mock));
    let ok = d.write_effective_current(13.5).await;
    assert!(ok);
    let last = d
        .modbus_manager
        .as_mut()
        .unwrap()
        .as_any_mut()
        .downcast_mut::<MockModbus>()
        .unwrap()
        .last_write
        .clone();
    let (slave, addr, vals) = last.expect("expected a write call");
    assert_eq!(slave, cfg.modbus.socket_slave_id);
    assert_eq!(addr, cfg.registers.amps_config);
    assert_eq!(vals, crate::modbus::encode_32bit_float(13.5).to_vec());
}

#[tokio::test]
async fn poll_cycle_with_manual_mode_writes_current() {
    let (tx, rx) = mpsc::unbounded_channel();
    let mut d = crate::driver::AlfenDriver::new(rx, tx).await.unwrap();
    // Setup readings
    let cfg = d.config().clone();
    let volt_regs = [230.0f32, 231.0, 229.0]
        .into_iter()
        .flat_map(regs_from_f32)
        .collect::<Vec<_>>();
    let curr_regs = [6.0f32, 7.0, 8.0]
        .into_iter()
        .flat_map(regs_from_f32)
        .collect::<Vec<_>>();
    let power_regs = [1200.0f32, 1300.0, 1400.0, 3900.0]
        .into_iter()
        .flat_map(regs_from_f32)
        .collect::<Vec<_>>();
    let energy_regs = regs_from_f64(0.0);
    let status_regs = vec![0x4231, 0, 0, 0, 0]; // "B1" -> connected
    let mock = MockModbus::new()
        .with_read(
            cfg.modbus.socket_slave_id,
            cfg.registers.voltages,
            6,
            volt_regs,
        )
        .with_read(
            cfg.modbus.socket_slave_id,
            cfg.registers.currents,
            6,
            curr_regs,
        )
        .with_read(
            cfg.modbus.socket_slave_id,
            cfg.registers.power,
            8,
            power_regs,
        )
        .with_read(
            cfg.modbus.socket_slave_id,
            cfg.registers.energy,
            4,
            energy_regs,
        )
        .with_read(
            cfg.modbus.socket_slave_id,
            cfg.registers.status,
            5,
            status_regs,
        )
        .with_read(
            cfg.modbus.station_slave_id,
            cfg.registers.station_max_current,
            2,
            regs_from_f32(32.0),
        );
    d.modbus_manager = Some(Box::new(mock));
    d.current_mode = crate::controls::ChargingMode::Manual;
    d.start_stop = crate::controls::StartStopState::Enabled;
    d.intended_set_current = 6.0;
    d.poll_cycle().await.unwrap();
    assert!((d.last_sent_current - 6.0).abs() < f32::EPSILON);
}

#[tokio::test]
async fn insufficient_solar_grace_timer_starts_and_expires() {
    let (tx, rx) = mpsc::unbounded_channel();
    let mut d = crate::driver::AlfenDriver::new(rx, tx).await.unwrap();

    // Configure Auto mode and enabled charging
    d.current_mode = crate::controls::ChargingMode::Auto;
    d.start_stop = crate::controls::StartStopState::Enabled;

    // Set EVSE minimum and a short grace period for the test via config update
    let mut cfg = d.config().clone();
    cfg.controls.min_set_current = 6.0;
    cfg.controls.min_charge_duration_seconds = 2;
    d.update_config(cfg).unwrap();

    // Simulate that we were charging at >= min current
    d.last_sent_current = 6.0;
    d.last_set_current_monotonic = std::time::Instant::now();

    // No PV available -> base effective would be 0.0
    let (eff1, _soc) = d.compute_effective_current_with_soc(0.0, 0.0, 0.0).await;
    // Grace timer should kick in and hold at min current
    assert!((eff1 - 6.0).abs() < 0.01, "expected hold at min current");
    assert!(d.min_charge_timer_deadline.is_some(), "timer should be set");

    // Force timer expiry
    d.min_charge_timer_deadline =
        Some(std::time::Instant::now() - std::time::Duration::from_secs(1));

    // Recompute under same insufficient PV conditions
    let (eff2, _soc2) = d.compute_effective_current_with_soc(0.0, 0.0, 0.0).await;
    // After expiry, allow stopping (0 A)
    assert!(eff2 <= 0.01, "expected stop after timer expiry");
    assert!(
        d.min_charge_timer_deadline.is_none(),
        "timer should clear after expiry"
    );

    // Now provide sufficient PV so base effective >= min -> timer should clear
    let watts = 6000.0_f32; // ~8.7 A on 3 phases -> >= 6 A
    let (eff3, _soc3) = d.compute_effective_current_with_soc(0.0, 0.0, watts).await;
    assert!(eff3 >= 6.0, "sufficient PV should produce >= min current");
    assert!(
        d.min_charge_timer_deadline.is_none(),
        "timer should be cleared when PV sufficient"
    );
}

#[tokio::test]
async fn grace_timer_does_not_restart_without_pv_improvement_after_expiry() {
    let (tx, rx) = mpsc::unbounded_channel();
    let mut d = crate::driver::AlfenDriver::new(rx, tx).await.unwrap();

    // Auto mode, charging enabled
    d.current_mode = crate::controls::ChargingMode::Auto;
    d.start_stop = crate::controls::StartStopState::Enabled;

    // Configure EVSE min current and short grace period
    let mut cfg = d.config().clone();
    cfg.controls.min_set_current = 6.0;
    cfg.controls.min_charge_duration_seconds = 2;
    d.update_config(cfg).unwrap();

    // Assume we have been charging at >= min current
    d.last_sent_current = 6.0;

    // No PV available -> base effective would be 0.0, timer should start and hold at min
    let (eff1, _soc1) = d.compute_effective_current_with_soc(0.0, 0.0, 0.0).await;
    assert!((eff1 - 6.0).abs() < 0.01, "expected hold at min current");
    assert!(d.min_charge_timer_deadline.is_some(), "timer should be set");

    // Expire the timer
    d.min_charge_timer_deadline =
        Some(std::time::Instant::now() - std::time::Duration::from_secs(1));

    // Recompute with still no PV -> should allow stop and clear timer
    let (eff2, _soc2) = d.compute_effective_current_with_soc(0.0, 0.0, 0.0).await;
    assert!(eff2 <= 0.01, "expected stop after expiry");
    assert!(
        d.min_charge_timer_deadline.is_none(),
        "timer should be cleared after expiry"
    );

    // Simulate immediate write to 0 A (as poll_cycle would do), which updates monotonic timestamp
    d.last_sent_current = 0.0;
    d.last_set_current_monotonic = std::time::Instant::now();

    // Still no PV improvement: the timer must NOT restart; effective stays 0.0
    let (eff3, _soc3) = d.compute_effective_current_with_soc(0.0, 0.0, 0.0).await;
    assert!(
        eff3 <= 0.01,
        "effective should remain 0 A without PV improvement"
    );
    assert!(
        d.min_charge_timer_deadline.is_none(),
        "timer must not restart without PV improvement"
    );
}
