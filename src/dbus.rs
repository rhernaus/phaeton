//! D-Bus integration for Venus OS compatibility
//!
//! This module provides D-Bus service integration to expose the charger
//! as a standard Victron device on Venus OS.

use crate::error::Result;
use crate::logging::get_logger;

use std::collections::HashMap;

/// D-Bus service manager
pub struct DbusService {
    logger: crate::logging::StructuredLogger,
    #[allow(dead_code)]
    service_name: String,
    paths: HashMap<String, serde_json::Value>,
}

impl DbusService {
    /// Create a new D-Bus service
    pub async fn new() -> Result<Self> {
        let logger = get_logger("dbus");
        logger.info("Initializing D-Bus service (stub)");
        Ok(Self {
            logger,
            service_name: "com.victronenergy.evcharger.alfen_0".to_string(),
            paths: HashMap::new(),
        })
    }

    /// Start the D-Bus service
    pub async fn start(&mut self) -> Result<()> {
        self.logger.info("Starting D-Bus service (stub)");
        Ok(())
    }

    /// Stop the D-Bus service
    pub async fn stop(&mut self) -> Result<()> {
        self.logger.info("Stopping D-Bus service (stub)");
        Ok(())
    }

    /// Update a D-Bus path value
    pub async fn update_path(&mut self, path: &str, value: serde_json::Value) -> Result<()> {
        self.logger.debug(&format!("DBus set {} = {}", path, value));
        self.paths.insert(path.to_string(), value);
        Ok(())
    }

    /// Convenience to update multiple paths
    pub async fn update_paths(
        &mut self,
        updates: impl IntoIterator<Item = (String, serde_json::Value)>,
    ) -> Result<()> {
        for (k, v) in updates {
            self.update_path(&k, v).await?;
        }
        Ok(())
    }

    /// Read last value (stub storage)
    pub fn get(&self, path: &str) -> Option<&serde_json::Value> {
        self.paths.get(path)
    }
}
