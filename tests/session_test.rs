use phaeton::session::ChargingSessionManager;

#[test]
fn start_update_end_session() {
    let mut mgr = ChargingSessionManager::default();
    assert!(mgr.start_session(100.0).is_ok());

    // Update with power and energy progress
    mgr.update(3500.0, 101.0).unwrap();
    let stats = mgr.get_session_stats();
    assert_eq!(
        stats.get("session_active").and_then(|v| v.as_bool()),
        Some(true)
    );

    // End the session at 102 kWh
    assert!(mgr.end_session(102.0).is_ok());
    assert!(mgr.current_session.is_none());
    let state = mgr.get_state();
    let last = state.get("last_session").unwrap();
    let energy = last
        .get("energy_delivered_kwh")
        .and_then(|v| v.as_f64())
        .unwrap();
    assert!((energy - 2.0).abs() < 1e-6);
}
