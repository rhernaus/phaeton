//! D-Bus integration for Venus OS compatibility
//!
//! Connects to the system bus using zbus and owns the Victron EV charger
//! well-known name. For now, values are stored locally; a full interface
//! will be added later.

use crate::error::{PhaetonError, Result};
use crate::logging::get_logger;
use std::collections::HashMap;
use zbus::{Connection, Result as ZbusResult, names::WellKnownName};

/// D-Bus service manager
pub struct DbusService {
    logger: crate::logging::StructuredLogger,
    service_name: String,
    connection: Option<Connection>,
    paths: HashMap<String, serde_json::Value>,
}

impl DbusService {
    /// Create a new D-Bus service
    pub async fn new(device_instance: u32) -> Result<Self> {
        let logger = get_logger("dbus");
        logger.info("Initializing D-Bus service (zbus)");

        let service_name = format!("com.victronenergy.evcharger.alfen_{}", device_instance);

        Ok(Self {
            logger,
            service_name,
            connection: None,
            paths: HashMap::new(),
        })
    }

    /// Start the D-Bus service: connect and request name
    pub async fn start(&mut self) -> Result<()> {
        // Connect to system bus (falls back to session bus if system unavailable)
        let connection = match Connection::system().await {
            Ok(c) => c,
            Err(_) => Connection::session()
                .await
                .map_err(|e| PhaetonError::dbus(format!("DBus connect failed: {}", e)))?,
        };

        // Request our well-known name
        self.request_name(&connection)
            .await
            .map_err(|e| PhaetonError::dbus(format!("RequestName failed: {}", e)))?;

        self.logger
            .info(&format!("D-Bus service started: {}", self.service_name));

        // Initialize common paths with defaults (local cache)
        self.paths.insert(
            "/ProductName".to_string(),
            serde_json::json!("Alfen EV Charger"),
        );
        self.paths
            .insert("/FirmwareVersion".to_string(), serde_json::json!("Unknown"));
        self.paths
            .insert("/Serial".to_string(), serde_json::json!("Unknown"));
        self.paths
            .insert("/Ac/Energy/Forward".to_string(), serde_json::json!(0.0));
        self.paths
            .insert("/Ac/PhaseCount".to_string(), serde_json::json!(0));

        self.connection = Some(connection);
        Ok(())
    }

    /// Stop the D-Bus service
    pub async fn stop(&mut self) -> Result<()> {
        self.logger.info("Stopping D-Bus service");
        self.connection = None;
        Ok(())
    }

    /// Update a D-Bus path value (local cache for now)
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

    /// Read last value (local cache)
    pub fn get(&self, path: &str) -> Option<&serde_json::Value> {
        self.paths.get(path)
    }

    // TODO: Export a real object tree with org.freedesktop.DBus.Properties to expose values on the bus.

    async fn request_name(&self, connection: &Connection) -> ZbusResult<()> {
        use zbus::fdo::{DBusProxy, RequestNameFlags};
        let proxy = DBusProxy::new(connection).await?;
        let name = WellKnownName::try_from(self.service_name.as_str())?;
        let _ = proxy
            .request_name(name, RequestNameFlags::ReplaceExisting.into())
            .await?;
        Ok(())
    }
}
