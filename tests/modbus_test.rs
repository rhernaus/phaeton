use phaeton::config::ModbusConfig;
use phaeton::modbus::ModbusClient;

#[test]
fn modbus_client_default_timeouts_and_state() {
    let cfg = ModbusConfig::default();
    let client = ModbusClient::new(&cfg);
    assert!(!client.is_connected());
}

// Connection classification is tested internally in src/modbus.rs unit tests
