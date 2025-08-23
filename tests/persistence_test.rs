use phaeton::persistence::{PersistenceManager, PersistentState};
use serde_json::json;

#[test]
fn default_state_values() {
    let s = PersistentState::default();
    assert_eq!(s.mode, 0);
    assert_eq!(s.start_stop, 0);
    assert!((s.set_current - 6.0).abs() < f32::EPSILON);
}

#[test]
fn load_save_roundtrip() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let path = tmp.path().to_string_lossy().to_string();

    let mut mgr = PersistenceManager::new(&path);
    mgr.set_mode(2);
    mgr.set_start_stop(1);
    mgr.set_set_current(10.5);
    mgr.set_insufficient_solar_start(123.0);
    mgr.save().unwrap();

    let mut mgr2 = PersistenceManager::new(&path);
    mgr2.load().unwrap();
    assert_eq!(mgr2.get::<u32>("mode").unwrap(), 2);
    assert_eq!(mgr2.get::<u32>("start_stop").unwrap(), 1);
    assert!((mgr2.get::<f32>("set_current").unwrap() - 10.5).abs() < 1e-6);
    assert!((mgr2.get::<f64>("insufficient_solar_start").unwrap() - 123.0).abs() < 1e-6);
}

#[test]
fn update_merges_json_sections() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let path = tmp.path().to_string_lossy().to_string();

    let mut mgr = PersistenceManager::new(&path);
    mgr.update(json!({
        "mode": 1,
        "start_stop": 1,
        "set_current": 12.0,
        "insufficient_solar_start": 456.0,
        "session": {"dummy": true}
    }))
    .unwrap();
    assert_eq!(mgr.get::<u32>("mode").unwrap(), 1);
    assert_eq!(mgr.get::<u32>("start_stop").unwrap(), 1);
    assert!((mgr.get::<f32>("set_current").unwrap() - 12.0).abs() < 1e-6);
    assert!((mgr.get::<f64>("insufficient_solar_start").unwrap() - 456.0).abs() < 1e-6);
    assert!(mgr.get_section("session").is_some());
}
