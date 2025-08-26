use phaeton::config::Config;
use std::fs;

#[test]
fn save_and_load_yaml_roundtrip() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let path = tmp_dir.path().join("config.yaml");

    let mut cfg = Config::default();
    cfg.modbus.ip = "10.0.0.5".to_string();
    cfg.logging.file = path.with_extension("log").to_string_lossy().to_string();

    cfg.save_to_file(&path).unwrap();
    let loaded = Config::from_file(&path).unwrap();

    assert_eq!(loaded.modbus.ip, "10.0.0.5");
    assert_eq!(loaded.logging.file, cfg.logging.file);
}

#[test]
fn config_validation_errors() {
    let mut cfg = Config::default();

    // Invalid IP
    cfg.modbus.ip.clear();
    assert!(cfg.validate().is_err());

    // Invalid port
    cfg = Config::default();
    cfg.modbus.port = 0;
    assert!(cfg.validate().is_err());

    // Non-positive defaults
    cfg = Config::default();
    cfg.defaults.intended_set_current = 0.0;
    assert!(cfg.validate().is_err());

    cfg = Config::default();
    cfg.defaults.station_max_current = 0.0;
    assert!(cfg.validate().is_err());

    // Poll interval zero
    cfg = Config::default();
    cfg.poll_interval_ms = 0;
    assert!(cfg.validate().is_err());
}

#[test]
fn from_file_with_invalid_yaml_fails() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    fs::write(tmp.path(), b"bad: [unclosed").unwrap();
    let err = Config::from_file(tmp.path()).unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("Serialization error"));
}