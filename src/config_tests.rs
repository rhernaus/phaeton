#![cfg(test)]

use super::config::*;

#[test]
fn test_default_config() {
    let config = Config::default();
    assert_eq!(config.modbus.port, 502);
    assert_eq!(config.device_instance, 0);
    assert_eq!(config.poll_interval_ms, 1000);
    assert!(config.require_dbus);
}

#[test]
fn test_config_validation() {
    let mut config = Config::default();
    assert!(config.validate().is_ok());

    // Test invalid IP
    config.modbus.ip = String::new();
    assert!(config.validate().is_err());

    // Reset and test invalid port
    config = Config::default();
    config.modbus.port = 0;
    assert!(config.validate().is_err());
}

#[test]
fn test_config_serialization() {
    let config = Config::default();
    let yaml = serde_yaml::to_string(&config).unwrap();
    let deserialized: Config = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(config.modbus.port, deserialized.modbus.port);
}


