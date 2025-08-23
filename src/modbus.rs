//! Modbus TCP client for Alfen EV charger communication
//!
//! This module provides async Modbus TCP communication with the Alfen EV charger,
//! handling both socket slave (real-time data) and station slave (configuration)
//! operations with proper error handling and connection management.

use crate::config::ModbusConfig;
use crate::error::{PhaetonError, Result};
use crate::logging::get_logger;
use std::time::Duration;
use tokio::time::{sleep, timeout};
use tokio_modbus::client::tcp;
use tokio_modbus::prelude::*;

/// Modbus TCP client for Alfen communication
pub struct ModbusClient {
    /// Modbus TCP client connection
    client: Option<tokio_modbus::client::Context>,

    /// Configuration
    config: ModbusConfig,

    /// Connection timeout
    connection_timeout: Duration,

    /// Operation timeout
    operation_timeout: Duration,

    /// Logger
    logger: crate::logging::StructuredLogger,
}

impl ModbusClient {
    /// Create a new Modbus client
    pub fn new(config: &ModbusConfig) -> Self {
        let logger = get_logger("modbus");
        Self {
            client: None,
            config: config.clone(),
            connection_timeout: Duration::from_secs(5),
            operation_timeout: Duration::from_secs(2),
            logger,
        }
    }

    /// Connect to the Modbus server
    pub async fn connect(&mut self) -> Result<()> {
        let address = format!("{}:{}", self.config.ip, self.config.port);

        self.logger
            .info(&format!("Connecting to Modbus server at {}", address));

        let socket_addr: std::net::SocketAddr = address
            .parse()
            .map_err(|e| PhaetonError::modbus(format!("Invalid socket address: {}", e)))?;

        match timeout(self.connection_timeout, tcp::connect(socket_addr)).await {
            Ok(Ok(client)) => {
                self.client = Some(client);
                self.logger.info("Successfully connected to Modbus server");
                Ok(())
            }
            Ok(Err(e)) => {
                let error_msg = format!("Failed to connect to Modbus server: {}", e);
                self.logger.error(&error_msg);
                Err(PhaetonError::modbus(error_msg))
            }
            Err(_) => {
                let error_msg = "Connection timeout".to_string();
                self.logger.error(&error_msg);
                Err(PhaetonError::timeout(error_msg))
            }
        }
    }

    /// Disconnect from the Modbus server
    pub async fn disconnect(&mut self) -> Result<()> {
        if let Some(_client) = self.client.take() {
            self.logger.info("Disconnecting from Modbus server");
            // The client will be dropped automatically
            Ok(())
        } else {
            Ok(())
        }
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.client.is_some()
    }

    /// Read holding registers
    pub async fn read_holding_registers(
        &mut self,
        slave_id: u8,
        address: u16,
        count: u16,
    ) -> Result<Vec<u16>> {
        let timeout_duration = self.operation_timeout;

        // Log before borrowing client
        self.logger.debug(&format!(
            "Reading {} registers from address {} on slave {}",
            count, address, slave_id
        ));

        let client = self.get_client()?;
        let request = client.read_holding_registers(address, count);

        match timeout(timeout_duration, request).await {
            Ok(Ok(response)) => {
                self.logger.trace(&format!(
                    "Read {} registers: {:?}",
                    response.len(),
                    response
                ));
                Ok(response)
            }
            Ok(Err(e)) => {
                let error_msg = format!("Failed to read holding registers: {}", e);
                self.logger.error(&error_msg);
                Err(PhaetonError::modbus(error_msg))
            }
            Err(_) => {
                let error_msg = "Read operation timeout".to_string();
                self.logger.error(&error_msg);
                Err(PhaetonError::timeout(error_msg))
            }
        }
    }

    /// Write single register
    pub async fn write_single_register(
        &mut self,
        slave_id: u8,
        address: u16,
        value: u16,
    ) -> Result<()> {
        let timeout_duration = self.operation_timeout;

        // Log before borrowing client
        self.logger.debug(&format!(
            "Writing value {} to register {} on slave {}",
            value, address, slave_id
        ));

        let client = self.get_client()?;
        let request = client.write_single_register(address, value);

        match timeout(timeout_duration, request).await {
            Ok(Ok(_)) => {
                self.logger.debug("Successfully wrote single register");
                Ok(())
            }
            Ok(Err(e)) => {
                let error_msg = format!("Failed to write single register: {}", e);
                self.logger.error(&error_msg);
                Err(PhaetonError::modbus(error_msg))
            }
            Err(_) => {
                let error_msg = "Write operation timeout".to_string();
                self.logger.error(&error_msg);
                Err(PhaetonError::timeout(error_msg))
            }
        }
    }

    /// Write multiple registers
    pub async fn write_multiple_registers(
        &mut self,
        slave_id: u8,
        address: u16,
        values: &[u16],
    ) -> Result<()> {
        let timeout_duration = self.operation_timeout;

        // Log before borrowing client
        self.logger.debug(&format!(
            "Writing {} values to registers starting at {} on slave {}",
            values.len(),
            address,
            slave_id
        ));

        let client = self.get_client()?;
        let request = client.write_multiple_registers(address, values);

        match timeout(timeout_duration, request).await {
            Ok(Ok(_)) => {
                self.logger.debug("Successfully wrote multiple registers");
                Ok(())
            }
            Ok(Err(e)) => {
                let error_msg = format!("Failed to write multiple registers: {}", e);
                self.logger.error(&error_msg);
                Err(PhaetonError::modbus(error_msg))
            }
            Err(_) => {
                let error_msg = "Write operation timeout".to_string();
                self.logger.error(&error_msg);
                Err(PhaetonError::timeout(error_msg))
            }
        }
    }

    /// Get client reference or error if not connected
    fn get_client(&mut self) -> Result<&mut tokio_modbus::client::Context> {
        self.client
            .as_mut()
            .ok_or_else(|| PhaetonError::modbus("Not connected to Modbus server"))
    }
}

/// Utility functions for data conversion

/// Decode 32-bit float from two 16-bit registers (big-endian)
pub fn decode_32bit_float(registers: &[u16]) -> Result<f32> {
    if registers.len() < 2 {
        return Err(PhaetonError::modbus(
            "Insufficient registers for 32-bit float",
        ));
    }

    let bytes = [
        (registers[0] >> 8) as u8,
        (registers[0] & 0xFF) as u8,
        (registers[1] >> 8) as u8,
        (registers[1] & 0xFF) as u8,
    ];

    let value = f32::from_be_bytes(bytes);
    Ok(value)
}

/// Decode 64-bit float from four 16-bit registers (big-endian)
pub fn decode_64bit_float(registers: &[u16]) -> Result<f64> {
    if registers.len() < 4 {
        return Err(PhaetonError::modbus(
            "Insufficient registers for 64-bit float",
        ));
    }

    let bytes = [
        (registers[0] >> 8) as u8,
        (registers[0] & 0xFF) as u8,
        (registers[1] >> 8) as u8,
        (registers[1] & 0xFF) as u8,
        (registers[2] >> 8) as u8,
        (registers[2] & 0xFF) as u8,
        (registers[3] >> 8) as u8,
        (registers[3] & 0xFF) as u8,
    ];

    let value = f64::from_be_bytes(bytes);
    Ok(value)
}

/// Decode string from registers
pub fn decode_string(registers: &[u16], max_length: Option<usize>) -> Result<String> {
    let mut bytes = Vec::new();

    for &reg in registers {
        bytes.push((reg >> 8) as u8);
        bytes.push((reg & 0xFF) as u8);
    }

    // Remove null terminators and trailing whitespace
    let string = String::from_utf8(bytes)
        .map_err(|e| PhaetonError::modbus(format!("Invalid UTF-8 string: {}", e)))?;

    let string = string.trim_matches('\0').trim();

    if let Some(max_len) = max_length {
        Ok(string.chars().take(max_len).collect())
    } else {
        Ok(string.to_string())
    }
}

/// Encode 32-bit float to two 16-bit registers (big-endian)
pub fn encode_32bit_float(value: f32) -> [u16; 2] {
    let bytes = value.to_be_bytes();
    [
        ((bytes[0] as u16) << 8) | (bytes[1] as u16),
        ((bytes[2] as u16) << 8) | (bytes[3] as u16),
    ]
}

/// Connection manager with automatic reconnection
pub struct ModbusConnectionManager {
    client: ModbusClient,
    config: ModbusConfig,
    max_retry_attempts: u32,
    retry_delay: Duration,
    logger: crate::logging::StructuredLogger,
}

impl ModbusConnectionManager {
    /// Create a new connection manager
    pub fn new(config: &ModbusConfig, max_retry_attempts: u32, retry_delay: Duration) -> Self {
        let logger = get_logger("modbus_manager");
        Self {
            client: ModbusClient::new(config),
            config: config.clone(),
            max_retry_attempts,
            retry_delay,
            logger,
        }
    }

    /// Execute a Modbus operation with automatic reconnection
    pub async fn execute_with_reconnect<F, Fut, T>(&mut self, operation: F) -> Result<T>
    where
        F: Fn(&mut ModbusClient) -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let mut attempts = 0;

        loop {
            // Ensure we're connected
            if !self.client.is_connected() {
                if let Err(e) = self.client.connect().await {
                    attempts += 1;
                    if attempts >= self.max_retry_attempts {
                        return Err(e);
                    }
                    self.logger
                        .warn(&format!("Connection attempt {} failed: {}", attempts, e));
                    sleep(self.retry_delay).await;
                    continue;
                }
            }

            // Execute the operation
            match operation(&mut self.client).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    // Check if it's a connection error that requires reconnection
                    if Self::is_connection_error(&e) {
                        self.logger
                            .warn(&format!("Operation failed due to connection error: {}", e));
                        self.client.disconnect().await.ok(); // Ignore disconnect errors
                        attempts += 1;
                        if attempts >= self.max_retry_attempts {
                            return Err(e);
                        }
                        sleep(self.retry_delay).await;
                        continue;
                    } else {
                        return Err(e);
                    }
                }
            }
        }
    }

    /// Check if an error is a connection-related error
    fn is_connection_error(error: &PhaetonError) -> bool {
        match error {
            PhaetonError::Modbus { message: msg } => {
                msg.contains("connection")
                    || msg.contains("Connection")
                    || msg.contains("timeout")
                    || msg.contains("disconnected")
            }
            PhaetonError::Timeout { message: _ } => true,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ModbusConfig;

    #[test]
    fn test_decode_32bit_float() {
        let registers = [0x3F80, 0x0000]; // 1.0 in big-endian
        let result = decode_32bit_float(&registers).unwrap();
        assert!((result - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_decode_64bit_float() {
        let registers = [0x3FF0, 0x0000, 0x0000, 0x0000]; // 1.0 in big-endian
        let result = decode_64bit_float(&registers).unwrap();
        assert!((result - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_encode_32bit_float() {
        let value = 1.0f32;
        let registers = encode_32bit_float(value);
        assert_eq!(registers, [0x3F80, 0x0000]);
    }

    #[test]
    fn test_decode_string() {
        let registers = [0x0041, 0x0042, 0x0043]; // "ABC"
        let result = decode_string(&registers, None).unwrap();
        assert_eq!(result, "ABC");
    }

    #[test]
    fn test_modbus_config() {
        let config = ModbusConfig::default();
        assert_eq!(config.port, 502);
        assert_eq!(config.socket_slave_id, 1);
        assert_eq!(config.station_slave_id, 200);
    }

    #[test]
    fn test_modbus_client_creation() {
        let config = ModbusConfig::default();
        let client = ModbusClient::new(&config);
        assert!(!client.is_connected());
    }
}
