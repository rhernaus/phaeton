use chrono::Utc;
use phaeton::session::ChargingSessionManager;
use serde_json::json;

#[test]
fn restore_session_state_and_trim_history() {
    let mut mgr = ChargingSessionManager::new(5);

    let now = Utc::now().to_rfc3339();
    let earlier = (Utc::now() - chrono::Duration::minutes(5)).to_rfc3339();

    // Build history larger than max_history_size
    let mut history = vec![];
    for i in 0..12 {
        history.push(json!({
            "id": format!("h{}", i),
            "start_time": earlier,
            "end_time": now,
            "start_energy_kwh": 0.0,
            "end_energy_kwh": 1.0,
            "energy_delivered_kwh": 1.0,
            "peak_power_w": 1000.0,
            "average_power_w": 800.0,
            "cost": null,
            "status": "Completed"
        }));
    }

    let state = json!({
        "current_session": {
            "id": "cur",
            "start_time": earlier,
            "end_time": null,
            "start_energy_kwh": 10.0,
            "end_energy_kwh": null,
            "energy_delivered_kwh": 0.5,
            "peak_power_w": 2000.0,
            "average_power_w": 1500.0,
            "cost": null,
            "status": "Active"
        },
        "last_session": {
            "id": "last",
            "start_time": earlier,
            "end_time": now,
            "start_energy_kwh": 5.0,
            "end_energy_kwh": 7.0,
            "energy_delivered_kwh": 2.0,
            "peak_power_w": 2500.0,
            "average_power_w": 1200.0,
            "cost": null,
            "status": "Completed"
        },
        "history": history,
    });

    mgr.restore_state(state).unwrap();

    assert!(mgr.current_session.is_some());
    assert!(mgr.last_session.is_some());

    // History should be trimmed to max_history_size (5)
    let restored = mgr.get_state();
    let hist = restored.get("history").and_then(|v| v.as_array()).unwrap();
    assert!(hist.len() <= 5);

    // Set cost on last session
    mgr.set_cost_on_last_session(3.14);
    let state2 = mgr.get_state();
    let last = state2.get("last_session").unwrap().as_object().unwrap();
    assert_eq!(last.get("cost").and_then(|v| v.as_f64()), Some(3.14));
}