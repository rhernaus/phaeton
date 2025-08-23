//! D-Bus integration for Venus OS compatibility
//!
//! Connects to the system bus using zbus and owns the Victron EV charger
//! well-known name. For now, values are stored locally; a full interface
//! will be added later.

use crate::driver::DriverCommand;
use crate::error::{PhaetonError, Result};
use crate::logging::get_logger;
use std::collections::HashMap;
use std::sync::Mutex;
use tokio::sync::mpsc;
use zbus::object_server::InterfaceRef;
use zbus::zvariant::OwnedObjectPath;
use zbus::{Connection, Result as ZbusResult, names::WellKnownName};

#[derive(Default)]
struct EvChargerValues {
    mode: u8,
    start_stop: u8,
    set_current: f64,
    ac_power: f64,
    ac_energy_forward: f64,
    ac_current: f64,
}

struct EvCharger {
    values: Mutex<EvChargerValues>,
    #[allow(dead_code)]
    commands_tx: mpsc::UnboundedSender<DriverCommand>,
}

#[zbus::interface(name = "com.victronenergy.evcharger")]
impl EvCharger {
    #[zbus(property)]
    fn mode(&self) -> u8 {
        self.values.lock().unwrap().mode
    }

    #[zbus(property)]
    fn start_stop(&self) -> u8 {
        self.values.lock().unwrap().start_stop
    }

    #[zbus(property)]
    fn set_current(&self) -> f64 {
        self.values.lock().unwrap().set_current
    }

    #[zbus(property)]
    fn ac_power(&self) -> f64 {
        self.values.lock().unwrap().ac_power
    }

    #[zbus(property)]
    fn ac_energy_forward(&self) -> f64 {
        self.values.lock().unwrap().ac_energy_forward
    }

    #[zbus(property)]
    fn ac_current(&self) -> f64 {
        self.values.lock().unwrap().ac_current
    }

    // Property setters to control the driver
    #[zbus(property)]
    fn set_mode(&self, mode: u8) -> zbus::Result<()> {
        let _ = self.commands_tx.send(DriverCommand::SetMode(mode));
        self.values.lock().unwrap().mode = mode;
        Ok(())
    }

    #[zbus(property)]
    fn set_start_stop(&self, v: u8) -> zbus::Result<()> {
        let _ = self.commands_tx.send(DriverCommand::SetStartStop(v));
        self.values.lock().unwrap().start_stop = v;
        Ok(())
    }

    #[zbus(property)]
    fn set_set_current(&self, amps: f64) -> zbus::Result<()> {
        let _ = self
            .commands_tx
            .send(DriverCommand::SetCurrent(amps as f32));
        self.values.lock().unwrap().set_current = amps;
        Ok(())
    }
}

/// D-Bus service manager
pub struct DbusService {
    logger: crate::logging::StructuredLogger,
    service_name: String,
    connection: Option<Connection>,
    paths: HashMap<String, serde_json::Value>,
    charger_path: OwnedObjectPath,
    commands_tx: mpsc::UnboundedSender<DriverCommand>,
}

impl DbusService {
    /// Create a new D-Bus service
    pub async fn new(
        device_instance: u32,
        commands_tx: mpsc::UnboundedSender<DriverCommand>,
    ) -> Result<Self> {
        let logger = get_logger("dbus");
        logger.info("Initializing D-Bus service (zbus)");

        let service_name = format!("com.victronenergy.evcharger.alfen_{}", device_instance);

        let charger_path = OwnedObjectPath::try_from("/")
            .map_err(|e| PhaetonError::dbus(format!("Invalid object path: {}", e)))?;

        Ok(Self {
            logger,
            service_name,
            connection: None,
            paths: HashMap::new(),
            charger_path,
            commands_tx,
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

        // Register charger interface at path
        let charger = EvCharger {
            values: Mutex::new(EvChargerValues::default()),
            commands_tx: self.commands_tx.clone(),
        };
        connection
            .object_server()
            .at(&self.charger_path, charger)
            .await
            .map_err(|e| PhaetonError::dbus(format!("Register object failed: {}", e)))?;
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
        self.paths.insert(path.to_string(), value.clone());

        // Reflect into interface properties if known
        if let Some(conn) = &self.connection {
            let iface: InterfaceRef<EvCharger> = conn
                .object_server()
                .interface(&self.charger_path)
                .await
                .map_err(|e| PhaetonError::dbus(format!("Get interface failed: {}", e)))?;

            match path {
                "/Mode" => {
                    if let Some(v) = value.as_u64() {
                        let obj = iface.get_mut().await;
                        obj.values.lock().unwrap().mode = v as u8;
                    }
                }
                "/StartStop" => {
                    if let Some(v) = value.as_u64() {
                        let obj = iface.get_mut().await;
                        obj.values.lock().unwrap().start_stop = v as u8;
                    }
                }
                "/SetCurrent" => {
                    if let Some(v) = value.as_f64() {
                        let obj = iface.get_mut().await;
                        obj.values.lock().unwrap().set_current = v;
                    }
                }
                "/Ac/Power" => {
                    if let Some(v) = value.as_f64() {
                        let obj = iface.get_mut().await;
                        obj.values.lock().unwrap().ac_power = v;
                    }
                }
                "/Ac/Energy/Forward" => {
                    if let Some(v) = value.as_f64() {
                        let obj = iface.get_mut().await;
                        obj.values.lock().unwrap().ac_energy_forward = v;
                    }
                }
                "/Ac/Current" => {
                    if let Some(v) = value.as_f64() {
                        let obj = iface.get_mut().await;
                        obj.values.lock().unwrap().ac_current = v;
                    }
                }
                _ => {}
            }
        }
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
