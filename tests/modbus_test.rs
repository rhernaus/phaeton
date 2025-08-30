use phaeton::config::ModbusConfig;
use phaeton::modbus::{
    ModbusClient, decode_32bit_float, decode_64bit_float, decode_string, encode_32bit_float,
};

#[test]
fn modbus_client_default_timeouts_and_state() {
    let cfg = ModbusConfig::default();
    let client = ModbusClient::new(&cfg);
    assert!(!client.is_connected());
}

#[test]
fn decode_32bit_float_happy_path() {
    let regs = [0x3F80u16, 0x0000u16];
    assert!((decode_32bit_float(&regs).unwrap() - 1.0).abs() < f32::EPSILON);
}

#[test]
fn decode_64bit_float_happy_path() {
    let regs = [0x3FF0u16, 0x0000u16, 0x0000u16, 0x0000u16];
    assert!((decode_64bit_float(&regs).unwrap() - 1.0).abs() < f64::EPSILON);
}

#[test]
fn encode_32bit_float_happy_path() {
    assert_eq!(encode_32bit_float(1.0), [0x3F80, 0x0000]);
}

#[test]
fn decode_string_happy_path() {
    let regs = [0x0041u16, 0x0042u16, 0x0043u16];
    assert_eq!(decode_string(&regs, None).unwrap(), "ABC");
}

#[test]
fn modbus_config_defaults() {
    let c = ModbusConfig::default();
    assert_eq!(c.port, 502);
    assert_eq!(c.socket_slave_id, 1);
    assert_eq!(c.station_slave_id, 200);
}

// Connection classification is tested inside the crate in unit tests

#[tokio::test]
async fn modbus_connect_invalid_address_errors() {
    let cfg = ModbusConfig {
        ip: "bad host".to_string(),
        ..Default::default()
    };
    let mut client = ModbusClient::new(&cfg);
    let err = client.connect().await.unwrap_err();
    assert!(err.to_string().contains("Invalid socket address"));
}

#[tokio::test]
async fn modbus_read_write_without_connect_returns_not_connected() {
    let cfg = ModbusConfig::default();
    let mut client = ModbusClient::new(&cfg);
    let err_r = client
        .read_holding_registers(1, 0u16, 2u16)
        .await
        .unwrap_err();
    assert!(err_r.to_string().contains("Not connected"));
    let err_w = client
        .write_single_register(1, 0u16, 0u16)
        .await
        .unwrap_err();
    assert!(err_w.to_string().contains("Not connected"));
}

// Connection classification is tested internally in src/modbus.rs unit tests
