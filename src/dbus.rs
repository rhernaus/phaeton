//! D-Bus integration for Venus OS compatibility
//!
//! Connects to the system bus using zbus and owns the Victron EV charger
//! well-known name. For now, values are stored locally; a full interface
//! will be added later.

use crate::driver::DriverCommand;
use crate::error::{PhaetonError, Result};
use crate::logging::get_logger;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio::time::{Duration, timeout};
use zbus::object_server::{InterfaceRef, SignalEmitter};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};
use zbus::{Connection, Proxy, Result as ZbusResult, names::WellKnownName};

#[derive(Default)]
struct EvChargerValues {
    // Identity and metadata
    device_instance: u32,
    product_name: String,
    firmware_version: String,
    serial: String,
    product_id: u32,
    connected: u8,
    // Controls and measurements
    mode: u8,
    start_stop: u8,
    set_current: f64,
    max_current: f64,
    current: f64,
    ac_power: f64,
    ac_energy_forward: f64,
    // Optional: total meter energy if tracked; currently derived in shared map
    // ac_energy_total: f64,
    ac_current: f64,
    phase_count: u8,
    l1_voltage: f64,
    l2_voltage: f64,
    l3_voltage: f64,
    l1_current: f64,
    l2_current: f64,
    l3_current: f64,
    l1_power: f64,
    l2_power: f64,
    l3_power: f64,
    status: u32,
    charging_time: i64,
    position: u8,
    enable_display: u8,
    auto_start: u8,
    model: String,
}

struct EvCharger {
    values: Mutex<EvChargerValues>,
    #[allow(dead_code)]
    commands_tx: mpsc::UnboundedSender<DriverCommand>,
}

#[zbus::interface(name = "com.victronenergy.evcharger")]
impl EvCharger {
    // Identity
    #[zbus(property)]
    fn device_instance(&self) -> u32 {
        self.values.lock().unwrap().device_instance
    }

    #[zbus(property)]
    fn product_name(&self) -> String {
        self.values.lock().unwrap().product_name.clone()
    }

    #[zbus(property)]
    fn firmware_version(&self) -> String {
        self.values.lock().unwrap().firmware_version.clone()
    }

    #[zbus(property)]
    fn product_id(&self) -> u32 {
        self.values.lock().unwrap().product_id
    }

    #[zbus(property)]
    fn connected(&self) -> u8 {
        self.values.lock().unwrap().connected
    }

    #[zbus(property)]
    fn serial(&self) -> String {
        self.values.lock().unwrap().serial.clone()
    }

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
    fn max_current(&self) -> f64 {
        self.values.lock().unwrap().max_current
    }

    #[zbus(property)]
    fn current(&self) -> f64 {
        self.values.lock().unwrap().current
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
    fn ac_energy_total(&self) -> f64 {
        // Not tracked separately in EvChargerValues; derive from D-Bus shared map when available
        // For now, return forward energy as a safe fallback to avoid exposing NaN
        self.values.lock().unwrap().ac_energy_forward
    }

    #[zbus(property)]
    fn ac_current(&self) -> f64 {
        self.values.lock().unwrap().ac_current
    }

    #[zbus(property)]
    fn ac_phase_count(&self) -> u8 {
        self.values.lock().unwrap().phase_count
    }

    #[zbus(property)]
    fn ac_l1_voltage(&self) -> f64 {
        self.values.lock().unwrap().l1_voltage
    }
    #[zbus(property)]
    fn ac_l2_voltage(&self) -> f64 {
        self.values.lock().unwrap().l2_voltage
    }
    #[zbus(property)]
    fn ac_l3_voltage(&self) -> f64 {
        self.values.lock().unwrap().l3_voltage
    }

    #[zbus(property)]
    fn ac_l1_current(&self) -> f64 {
        self.values.lock().unwrap().l1_current
    }
    #[zbus(property)]
    fn ac_l2_current(&self) -> f64 {
        self.values.lock().unwrap().l2_current
    }
    #[zbus(property)]
    fn ac_l3_current(&self) -> f64 {
        self.values.lock().unwrap().l3_current
    }

    #[zbus(property)]
    fn ac_l1_power(&self) -> f64 {
        self.values.lock().unwrap().l1_power
    }
    #[zbus(property)]
    fn ac_l2_power(&self) -> f64 {
        self.values.lock().unwrap().l2_power
    }
    #[zbus(property)]
    fn ac_l3_power(&self) -> f64 {
        self.values.lock().unwrap().l3_power
    }

    #[zbus(property)]
    fn status(&self) -> u32 {
        self.values.lock().unwrap().status
    }

    #[zbus(property)]
    fn charging_time(&self) -> i64 {
        self.values.lock().unwrap().charging_time
    }

    #[zbus(property)]
    fn position(&self) -> u8 {
        self.values.lock().unwrap().position
    }

    #[zbus(property)]
    fn enable_display(&self) -> u8 {
        self.values.lock().unwrap().enable_display
    }

    #[zbus(property)]
    fn auto_start(&self) -> u8 {
        self.values.lock().unwrap().auto_start
    }

    #[zbus(property)]
    fn model(&self) -> String {
        self.values.lock().unwrap().model.clone()
    }

    // Property setters to control the driver
    #[zbus(property)]
    fn set_mode(&self, mode: u8) -> zbus::Result<()> {
        self.commands_tx
            .send(DriverCommand::SetMode(mode))
            .map_err(|_| zbus::Error::Failure("Failed to enqueue SetMode".into()))
    }

    #[zbus(property)]
    fn set_start_stop(&self, v: u8) -> zbus::Result<()> {
        self.commands_tx
            .send(DriverCommand::SetStartStop(v))
            .map_err(|_| zbus::Error::Failure("Failed to enqueue SetStartStop".into()))
    }

    #[zbus(property)]
    fn set_set_current(&self, amps: f64) -> zbus::Result<()> {
        self.commands_tx
            .send(DriverCommand::SetCurrent(amps as f32))
            .map_err(|_| zbus::Error::Failure("Failed to enqueue SetCurrent".into()))
    }
}

struct RootBus {
    shared: Arc<Mutex<DbusSharedState>>,
}

#[zbus::interface(name = "com.victronenergy.BusItem")]
impl RootBus {
    /// Root-level GetValue should return a dictionary of all items relative to '/'
    #[zbus(name = "GetValue")]
    async fn get_value(&self) -> OwnedValue {
        let map = self.collect_subtree_map("/", false);
        OwnedValue::from(map)
    }

    /// Root-level GetText should return textual representation for all items
    #[zbus(name = "GetText")]
    async fn get_text(&self) -> OwnedValue {
        let map = self.collect_subtree_map("/", true);
        OwnedValue::from(map)
    }

    /// Return all items as { path => { "Value": v, "Text": s } }
    #[zbus(name = "GetItems")]
    async fn get_items(
        &self,
    ) -> std::collections::HashMap<String, std::collections::HashMap<String, OwnedValue>> {
        use std::collections::HashMap;
        let shared = self.shared.lock().unwrap();
        let mut out: HashMap<String, HashMap<String, OwnedValue>> = HashMap::new();
        for (path, val) in shared.paths.iter() {
            let mut entry: HashMap<String, OwnedValue> = HashMap::new();
            entry.insert("Value".to_string(), BusItem::serde_to_owned_value(val));
            let text = format_text_value(val);
            let text_ov = OwnedValue::try_from(Value::from(text.as_str()))
                .unwrap_or_else(|_| OwnedValue::from(0i64));
            entry.insert("Text".to_string(), text_ov);
            out.insert(path.clone(), entry);
        }
        out
    }

    #[zbus(signal)]
    async fn items_changed(
        ctxt: &SignalEmitter<'_>,
        changes: std::collections::HashMap<&str, std::collections::HashMap<&str, OwnedValue>>,
    ) -> zbus::Result<()>;
}

impl RootBus {
    fn collect_subtree_map(
        &self,
        prefix: &str,
        as_text: bool,
    ) -> std::collections::HashMap<String, OwnedValue> {
        use std::collections::HashMap;
        let shared = self.shared.lock().unwrap();
        let mut px = prefix.to_string();
        if !px.ends_with('/') {
            px.push('/');
        }
        let mut result: HashMap<String, OwnedValue> = HashMap::new();
        for (path, val) in shared.paths.iter() {
            if path.starts_with(&px) {
                let suffix = &path[px.len()..];
                let ov = if as_text {
                    let text = format_text_value(val);
                    OwnedValue::try_from(Value::from(text.as_str()))
                        .unwrap_or_else(|_| OwnedValue::from(0i64))
                } else {
                    BusItem::serde_to_owned_value(val)
                };
                result.insert(suffix.to_string(), ov);
            }
        }
        result
    }
}

/// D-Bus service manager
pub struct DbusService {
    logger: crate::logging::StructuredLogger,
    service_name: String,
    connection: Option<Connection>,
    /// Shared BusItem state across all object paths
    shared: Arc<Mutex<DbusSharedState>>,
    /// Track which object paths have been registered on the object server
    registered_paths: HashSet<String>,
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

        // Match the Python driver's service naming so Venus OS/systemcalc recognizes it
        let service_name = format!("com.victronenergy.evcharger.phaeton_{}", device_instance);

        let charger_path = OwnedObjectPath::try_from("/")
            .map_err(|e| PhaetonError::dbus(format!("Invalid object path: {}", e)))?;

        Ok(Self {
            logger,
            service_name,
            connection: None,
            shared: Arc::new(Mutex::new(DbusSharedState::new(
                commands_tx.clone(),
                charger_path.clone(),
            ))),
            registered_paths: HashSet::new(),
            charger_path,
            commands_tx,
        })
    }

    /// Start the D-Bus service: connect and request name
    pub async fn start(&mut self) -> Result<()> {
        // Connect to system bus (falls back to session bus if system unavailable)
        let connection = match Connection::system().await {
            Ok(c) => {
                self.logger.info("Connected to D-Bus: system bus");
                c
            }
            Err(e_sys) => match Connection::session().await {
                Ok(c) => {
                    self.logger.warn(&format!(
                        "System bus unavailable ({}); using session bus",
                        e_sys
                    ));
                    c
                }
                Err(e_sess) => {
                    return Err(PhaetonError::dbus(format!(
                        "DBus connect failed: system={} session={}",
                        e_sys, e_sess
                    )));
                }
            },
        };

        // Request our well-known name
        self.request_name(&connection)
            .await
            .map_err(|e| PhaetonError::dbus(format!("RequestName failed: {}", e)))?;

        self.logger
            .info(&format!("D-Bus service started: {}", self.service_name));

        // Initialize a few common cached paths so the property interface has sane defaults
        {
            let mut shared = self.shared.lock().unwrap();
            shared.paths.insert(
                "/ProductName".to_string(),
                serde_json::json!("Alfen EV Charger"),
            );
            shared
                .paths
                .insert("/FirmwareVersion".to_string(), serde_json::json!("Unknown"));
            shared
                .paths
                .insert("/Serial".to_string(), serde_json::json!("Unknown"));
            shared
                .paths
                .insert("/ProductId".to_string(), serde_json::json!(0xC024u32));
            shared
                .paths
                .insert("/Connected".to_string(), serde_json::json!(0u8));
            shared
                .paths
                .insert("/Ac/Energy/Forward".to_string(), serde_json::json!(0.0));
            shared
                .paths
                .insert("/Ac/PhaseCount".to_string(), serde_json::json!(0));
            shared
                .paths
                .insert("/EnableDisplay".to_string(), serde_json::json!(0u8));
            shared
                .paths
                .insert("/AutoStart".to_string(), serde_json::json!(0u8));
            shared
                .paths
                .insert("/Model".to_string(), serde_json::json!("AC22NS"));
        }

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
        // Note: org.freedesktop.DBus.Properties is provided implicitly by zbus for objects
        // Also register the root '/' as a BusItem tree provider similar to VeDbusRootExport
        let root = RootBus {
            shared: Arc::clone(&self.shared),
        };
        connection
            .object_server()
            .at(&self.charger_path, root)
            .await
            .map_err(|e| PhaetonError::dbus(format!("Register root BusItem failed: {}", e)))?;
        self.connection = Some(connection);
        // Make connection available to BusItem handlers for immediate signal emission
        {
            let mut shared = self.shared.lock().unwrap();
            shared.connection = Some(self.connection.as_ref().unwrap().clone());
        }
        Ok(())
    }

    /// Stop the D-Bus service
    pub async fn stop(&mut self) -> Result<()> {
        self.logger.info("Stopping D-Bus service");
        self.connection = None;
        Ok(())
    }

    /// Ensure a BusItem exists for a path with initial value and writability.
    pub async fn ensure_item(
        &mut self,
        path: &str,
        initial_value: serde_json::Value,
        writable: bool,
    ) -> Result<()> {
        // Register intermediate tree nodes and the leaf path if not registered yet
        let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if !segments.is_empty() {
            for i in 1..=segments.len() {
                let subpath = format!("/{}", segments[..i].join("/"));
                if !self.registered_paths.contains(&subpath) {
                    let obj_path = OwnedObjectPath::try_from(subpath.as_str()).map_err(|e| {
                        PhaetonError::dbus(format!("Invalid object path '{}': {}", subpath, e))
                    })?;

                    // For leaf paths, register a value BusItem; for intermediate nodes, register a TreeNode
                    let item_is_leaf = i == segments.len();
                    if item_is_leaf {
                        let item = BusItem::new(subpath.clone(), Arc::clone(&self.shared));
                        if let Some(conn) = &self.connection {
                            conn.object_server()
                                .at(&obj_path, item)
                                .await
                                .map_err(|e| {
                                    PhaetonError::dbus(format!(
                                        "Register BusItem failed for {}: {}",
                                        subpath, e
                                    ))
                                })?;
                        }
                    } else {
                        let node = TreeNode::new(subpath.clone(), Arc::clone(&self.shared));
                        if let Some(conn) = &self.connection {
                            conn.object_server()
                                .at(&obj_path, node)
                                .await
                                .map_err(|e| {
                                    PhaetonError::dbus(format!(
                                        "Register TreeNode failed for {}: {}",
                                        subpath, e
                                    ))
                                })?;
                        }
                    }

                    self.registered_paths.insert(subpath);
                }
            }
        }

        // Initialize value and writability
        {
            let mut shared = self.shared.lock().unwrap();
            if !shared.paths.contains_key(path) {
                shared.paths.insert(path.to_string(), initial_value);
            }
            if writable {
                shared.writable.insert(path.to_string());
            }
        }

        Ok(())
    }

    /// Update a D-Bus path value (local cache and reflective properties)
    pub async fn update_path(&mut self, path: &str, value: serde_json::Value) -> Result<()> {
        // Skip no-op updates to avoid log spam and unnecessary signals
        {
            let shared = self.shared.lock().unwrap();
            if let Some(old) = shared.paths.get(path)
                && old == &value
            {
                return Ok(());
            }
        }

        // Ensure BusItem exists, default not writable
        let _ = self.ensure_item(path, value.clone(), false).await;

        // Elevate important identity/management paths to INFO level for visibility (only on change)
        match path {
            "/Status" => {
                let name = value
                    .as_u64()
                    .map(|v| match v {
                        0 => "Disconnected",
                        1 => "Connected",
                        2 => "Charging",
                        4 => "Wait Sun",
                        6 => "Wait Start",
                        7 => "Low SOC",
                        _ => "Unknown",
                    })
                    .unwrap_or("Unknown");
                self.logger
                    .info(&format!("DBus set /Status = {} ({})", value, name));
            }
            "/DeviceInstance" | "/ProductName" | "/ProductId" | "/FirmwareVersion" | "/Serial" => {
                self.logger.info(&format!("DBus set {} = {}", path, value))
            }
            _ => self.logger.debug(&format!("DBus set {} = {}", path, value)),
        };

        // Update shared cache with new value
        {
            let mut shared = self.shared.lock().unwrap();
            shared.paths.insert(path.to_string(), value.clone());
        }

        // Reflect into interface properties if known
        if let Some(conn) = &self.connection {
            let iface: InterfaceRef<EvCharger> = conn
                .object_server()
                .interface(&self.charger_path)
                .await
                .map_err(|e| PhaetonError::dbus(format!("Get interface failed: {}", e)))?;

            match path {
                "/DeviceInstance" => {
                    if let Some(v) = value.as_u64() {
                        let obj = iface.get_mut().await;
                        obj.values.lock().unwrap().device_instance = v as u32;
                    }
                }
                "/ProductName" => {
                    if let Some(v) = value.as_str() {
                        let obj = iface.get_mut().await;
                        obj.values.lock().unwrap().product_name = v.to_string();
                    }
                }
                "/FirmwareVersion" => {
                    if let Some(v) = value.as_str() {
                        let obj = iface.get_mut().await;
                        obj.values.lock().unwrap().firmware_version = v.to_string();
                    }
                }
                "/ProductId" => {
                    if let Some(v) = value.as_u64() {
                        let obj = iface.get_mut().await;
                        obj.values.lock().unwrap().product_id = v as u32;
                    }
                }
                "/Serial" => {
                    if let Some(v) = value.as_str() {
                        let obj = iface.get_mut().await;
                        obj.values.lock().unwrap().serial = v.to_string();
                    }
                }
                "/Connected" => {
                    if let Some(v) = value.as_u64() {
                        let obj = iface.get_mut().await;
                        obj.values.lock().unwrap().connected = v as u8;
                    }
                }
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
                "/MaxCurrent" => {
                    if let Some(v) = value.as_f64() {
                        let obj = iface.get_mut().await;
                        obj.values.lock().unwrap().max_current = v;
                    }
                }
                "/Current" => {
                    if let Some(v) = value.as_f64() {
                        let obj = iface.get_mut().await;
                        obj.values.lock().unwrap().current = v;
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
                "/Ac/PhaseCount" => {
                    if let Some(v) = value.as_u64() {
                        let obj = iface.get_mut().await;
                        obj.values.lock().unwrap().phase_count = v as u8;
                    }
                }
                "/Ac/L1/Voltage" => {
                    if let Some(v) = value.as_f64() {
                        let obj = iface.get_mut().await;
                        obj.values.lock().unwrap().l1_voltage = v;
                    }
                }
                "/Ac/L2/Voltage" => {
                    if let Some(v) = value.as_f64() {
                        let obj = iface.get_mut().await;
                        obj.values.lock().unwrap().l2_voltage = v;
                    }
                }
                "/Ac/L3/Voltage" => {
                    if let Some(v) = value.as_f64() {
                        let obj = iface.get_mut().await;
                        obj.values.lock().unwrap().l3_voltage = v;
                    }
                }
                "/Ac/L1/Current" => {
                    if let Some(v) = value.as_f64() {
                        let obj = iface.get_mut().await;
                        obj.values.lock().unwrap().l1_current = v;
                    }
                }
                "/Ac/L2/Current" => {
                    if let Some(v) = value.as_f64() {
                        let obj = iface.get_mut().await;
                        obj.values.lock().unwrap().l2_current = v;
                    }
                }
                "/Ac/L3/Current" => {
                    if let Some(v) = value.as_f64() {
                        let obj = iface.get_mut().await;
                        obj.values.lock().unwrap().l3_current = v;
                    }
                }
                "/Ac/L1/Power" => {
                    if let Some(v) = value.as_f64() {
                        let obj = iface.get_mut().await;
                        obj.values.lock().unwrap().l1_power = v;
                    }
                }
                "/Ac/L2/Power" => {
                    if let Some(v) = value.as_f64() {
                        let obj = iface.get_mut().await;
                        obj.values.lock().unwrap().l2_power = v;
                    }
                }
                "/Ac/L3/Power" => {
                    if let Some(v) = value.as_f64() {
                        let obj = iface.get_mut().await;
                        obj.values.lock().unwrap().l3_power = v;
                    }
                }
                "/Status" => {
                    if let Some(v) = value.as_u64() {
                        let obj = iface.get_mut().await;
                        obj.values.lock().unwrap().status = v as u32;
                    }
                }
                "/ChargingTime" => {
                    if let Some(v) = value.as_i64() {
                        let obj = iface.get_mut().await;
                        obj.values.lock().unwrap().charging_time = v;
                    } else if let Some(v) = value.as_u64() {
                        let obj = iface.get_mut().await;
                        obj.values.lock().unwrap().charging_time = v as i64;
                    }
                }
                "/Position" => {
                    if let Some(v) = value.as_u64() {
                        let obj = iface.get_mut().await;
                        obj.values.lock().unwrap().position = v as u8;
                    }
                }
                "/EnableDisplay" => {
                    if let Some(v) = value.as_u64() {
                        let obj = iface.get_mut().await;
                        obj.values.lock().unwrap().enable_display = v as u8;
                    }
                }
                "/AutoStart" => {
                    if let Some(v) = value.as_u64() {
                        let obj = iface.get_mut().await;
                        obj.values.lock().unwrap().auto_start = v as u8;
                    }
                }
                "/Model" => {
                    if let Some(v) = value.as_str() {
                        let obj = iface.get_mut().await;
                        obj.values.lock().unwrap().model = v.to_string();
                    }
                }
                _ => {}
            }
            // Emit change signals so listeners (VeDbusItemImport) update immediately
            // 1) Per-path PropertiesChanged on the BusItem
            let item_ctx = SignalEmitter::new(
                conn,
                OwnedObjectPath::try_from(path).map_err(|e| {
                    PhaetonError::dbus(format!("Invalid object path '{}': {}", path, e))
                })?,
            )
            .map_err(|e| PhaetonError::dbus(format!("SignalEmitter new failed: {}", e)))?;
            let mut changes: std::collections::HashMap<&str, OwnedValue> =
                std::collections::HashMap::new();
            changes.insert("Value", BusItem::serde_to_owned_value(&value));
            let text = format_text_value(&value);
            let text_ov = OwnedValue::try_from(Value::from(text.as_str()))
                .unwrap_or_else(|_| OwnedValue::from(0i64));
            changes.insert("Text", text_ov);
            let _ = BusItem::properties_changed(&item_ctx, changes).await;

            // 2) Root ItemsChanged summarizing the changed path
            let root_ctx = SignalEmitter::new(conn, self.charger_path.clone())
                .map_err(|e| PhaetonError::dbus(format!("Root SignalEmitter failed: {}", e)))?;
            let mut inner: std::collections::HashMap<&str, OwnedValue> =
                std::collections::HashMap::new();
            inner.insert("Value", BusItem::serde_to_owned_value(&value));
            let text = format_text_value(&value);
            let text_ov = OwnedValue::try_from(Value::from(text.as_str()))
                .unwrap_or_else(|_| OwnedValue::from(0i64));
            inner.insert("Text", text_ov);
            let mut outer: std::collections::HashMap<
                &str,
                std::collections::HashMap<&str, OwnedValue>,
            > = std::collections::HashMap::new();
            outer.insert(path, inner);
            let _ = RootBus::items_changed(&root_ctx, outer).await;
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

    /// Export a snapshot (subset) to D-Bus. Call from a dedicated exporter task.
    pub async fn export_snapshot(&mut self, snapshot: &serde_json::Value) -> Result<()> {
        let mut updates: Vec<(String, serde_json::Value)> = Vec::with_capacity(24);

        let g = |k: &str| snapshot.get(k).cloned();
        if let Some(v) = g("l1_voltage") {
            updates.push(("/Ac/L1/Voltage".to_string(), v));
        }
        if let Some(v) = g("l2_voltage") {
            updates.push(("/Ac/L2/Voltage".to_string(), v));
        }
        if let Some(v) = g("l3_voltage") {
            updates.push(("/Ac/L3/Voltage".to_string(), v));
        }
        if let Some(v) = g("l1_current") {
            updates.push(("/Ac/L1/Current".to_string(), v));
        }
        if let Some(v) = g("l2_current") {
            updates.push(("/Ac/L2/Current".to_string(), v));
        }
        if let Some(v) = g("l3_current") {
            updates.push(("/Ac/L3/Current".to_string(), v));
        }
        if let Some(v) = g("l1_power") {
            updates.push(("/Ac/L1/Power".to_string(), v));
        }
        if let Some(v) = g("l2_power") {
            updates.push(("/Ac/L2/Power".to_string(), v));
        }
        if let Some(v) = g("l3_power") {
            updates.push(("/Ac/L3/Power".to_string(), v));
        }
        if let Some(v) = g("ac_power") {
            updates.push(("/Ac/Power".to_string(), v));
        }
        if let Some(v) = g("status") {
            updates.push(("/Status".to_string(), v));
        }
        if let Some(v) = g("station_max_current") {
            updates.push(("/MaxCurrent".to_string(), v));
        }
        if let Some(session) = snapshot.get("session").and_then(|s| s.as_object())
            && let Some(v) = session.get("energy_delivered_kwh").cloned()
        {
            updates.push(("/Ac/Energy/Forward".to_string(), v));
        }
        if let Some(v) = g("total_energy_kwh") {
            updates.push(("/Ac/Energy/Total".to_string(), v));
        }
        if let Some(v) = g("ac_current") {
            updates.push(("/Ac/Current".to_string(), v.clone()));
            updates.push(("/Current".to_string(), v));
        }
        if let Some(v) = g("active_phases") {
            updates.push(("/Ac/PhaseCount".to_string(), v));
        }
        if let Some(v) = g("set_current") {
            updates.push(("/SetCurrent".to_string(), v));
        }
        if let Some(v) = g("charging_time_sec").or_else(|| {
            snapshot
                .get("session")
                .and_then(|s| s.get("charging_time_sec"))
                .cloned()
        }) {
            updates.push(("/ChargingTime".to_string(), v));
        }
        self.update_paths(updates).await
    }

    /// Read last value (local cache)
    pub fn get(&self, path: &str) -> Option<serde_json::Value> {
        let shared = self.shared.lock().unwrap();
        shared.paths.get(path).cloned()
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

impl DbusService {
    /// Read a value from another Victron D-Bus service implementing com.victronenergy.BusItem
    pub async fn read_remote_value(
        &self,
        service_name: &str,
        path: &str,
    ) -> Result<serde_json::Value> {
        let conn = match &self.connection {
            Some(c) => c,
            None => return Err(PhaetonError::dbus("No D-Bus connection available")),
        };

        // Bound proxy creation time to avoid hanging when the service is absent
        let proxy = timeout(
            Duration::from_millis(600),
            Proxy::new(conn, service_name, path, "com.victronenergy.BusItem"),
        )
        .await
        .map_err(|_| PhaetonError::dbus("DBus proxy creation timed out"))?
        .map_err(|e| PhaetonError::dbus(format!("Proxy creation failed: {}", e)))?;

        // Bound GetValue invocation to avoid request stalls
        let val: OwnedValue = timeout(Duration::from_millis(600), proxy.call("GetValue", &()))
            .await
            .map_err(|_| PhaetonError::dbus("DBus GetValue timed out"))?
            .map_err(|e| PhaetonError::dbus(format!("GetValue call failed: {}", e)))?;

        Ok(BusItem::owned_value_to_serde(&val))
    }

    // Removed grid strategy helper per design decision (not needed)
}

/// Shared state for BusItems
struct DbusSharedState {
    paths: HashMap<String, serde_json::Value>,
    writable: HashSet<String>,
    commands_tx: mpsc::UnboundedSender<DriverCommand>,
    /// Optional: connection handle for emitting signals directly from SetValue
    connection: Option<Connection>,
    /// Root object path for ItemsChanged
    root_path: OwnedObjectPath,
}

impl DbusSharedState {
    fn new(commands_tx: mpsc::UnboundedSender<DriverCommand>, root_path: OwnedObjectPath) -> Self {
        Self {
            paths: HashMap::new(),
            writable: HashSet::new(),
            commands_tx,
            connection: None,
            root_path,
        }
    }
}

/// VeDbus-style BusItem implementing com.victronenergy.BusItem
struct BusItem {
    path: String,
    shared: Arc<Mutex<DbusSharedState>>,
}

impl BusItem {
    fn new(path: String, shared: Arc<Mutex<DbusSharedState>>) -> Self {
        Self { path, shared }
    }

    pub(crate) fn serde_to_owned_value(v: &serde_json::Value) -> OwnedValue {
        match v {
            serde_json::Value::Null => OwnedValue::from(0i64),
            serde_json::Value::Bool(b) => OwnedValue::from(*b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    OwnedValue::from(i)
                } else if let Some(u) = n.as_u64() {
                    OwnedValue::from(u)
                } else {
                    OwnedValue::from(n.as_f64().unwrap_or(0.0))
                }
            }
            serde_json::Value::String(s) => OwnedValue::try_from(Value::from(s.as_str()))
                .unwrap_or_else(|_| OwnedValue::from(0i64)),
            _ => OwnedValue::from(0i64),
        }
    }

    pub(crate) fn owned_value_to_serde(v: &OwnedValue) -> serde_json::Value {
        if let Ok(b) = <bool as TryFrom<&OwnedValue>>::try_from(v) {
            return serde_json::json!(b);
        }
        if let Ok(i) = <i64 as TryFrom<&OwnedValue>>::try_from(v) {
            return serde_json::json!(i);
        }
        if let Ok(u) = <u64 as TryFrom<&OwnedValue>>::try_from(v) {
            return serde_json::json!(u);
        }
        if let Ok(f) = <f64 as TryFrom<&OwnedValue>>::try_from(v) {
            return serde_json::json!(f);
        }
        if let Ok(s) = <&str as TryFrom<&OwnedValue>>::try_from(v) {
            return serde_json::json!(s.to_string());
        }
        serde_json::json!(v.to_string())
    }
}

#[zbus::interface(name = "com.victronenergy.BusItem")]
impl BusItem {
    /// Return the current value of this item
    #[zbus(name = "GetValue")]
    async fn get_value(&self) -> OwnedValue {
        let val = {
            let shared = self.shared.lock().unwrap();
            shared
                .paths
                .get(&self.path)
                .cloned()
                .unwrap_or(serde_json::json!(0))
        };
        Self::serde_to_owned_value(&val)
    }

    /// Attempt to set the value; returns 0 on success (VeDbus-compatible)
    #[zbus(name = "SetValue")]
    async fn set_value(&self, value: OwnedValue) -> i32 {
        // Stage 1: validate, normalize, cache, and capture connection and roots without any await
        let (conn_opt, root_path, normalized_json, sv) = {
            let mut shared = self.shared.lock().unwrap();
            if !shared.writable.contains(&self.path) {
                return 1; // NOT OK
            }
            let sv_local = Self::owned_value_to_serde(&value);
            let normalized = if self.path == "/StartStop" {
                let v = match sv_local {
                    serde_json::Value::Bool(b) => {
                        if b {
                            1
                        } else {
                            0
                        }
                    }
                    serde_json::Value::Number(ref n) => {
                        if n.as_u64().unwrap_or(0) > 0 || n.as_i64().unwrap_or(0) > 0 {
                            1
                        } else {
                            0
                        }
                    }
                    serde_json::Value::String(ref s) => {
                        let t = s.trim().to_ascii_lowercase();
                        if t == "1" || t == "true" || t == "on" || t == "enabled" {
                            1
                        } else {
                            0
                        }
                    }
                    _ => 0,
                };
                serde_json::json!(v)
            } else {
                sv_local.clone()
            };
            shared.paths.insert(self.path.clone(), normalized.clone());
            (
                shared.connection.clone(),
                shared.root_path.clone(),
                normalized,
                sv_local,
            )
        };

        // Emit immediate change signals to mirror VeDbus behaviour (avoids UI comm errors)
        if let Some(conn) = conn_opt {
            if let Ok(obj_path) = OwnedObjectPath::try_from(self.path.as_str())
                && let Ok(item_ctx) = SignalEmitter::new(&conn, obj_path)
            {
                let mut changes: std::collections::HashMap<&str, OwnedValue> =
                    std::collections::HashMap::new();
                changes.insert("Value", BusItem::serde_to_owned_value(&normalized_json));
                let text = format_text_value(&normalized_json);
                if let Ok(text_ov) = OwnedValue::try_from(Value::from(text.as_str())) {
                    changes.insert("Text", text_ov);
                }
                let _ = BusItem::properties_changed(&item_ctx, changes).await;
            }
            if let Ok(root_ctx) = SignalEmitter::new(&conn, root_path) {
                let mut inner: std::collections::HashMap<&str, OwnedValue> =
                    std::collections::HashMap::new();
                inner.insert("Value", BusItem::serde_to_owned_value(&normalized_json));
                let text = format_text_value(&normalized_json);
                if let Ok(text_ov) = OwnedValue::try_from(Value::from(text.as_str())) {
                    inner.insert("Text", text_ov);
                }
                let mut outer: std::collections::HashMap<
                    &str,
                    std::collections::HashMap<&str, OwnedValue>,
                > = std::collections::HashMap::new();
                outer.insert(self.path.as_str(), inner);
                let _ = RootBus::items_changed(&root_ctx, outer).await;
            }
        }

        // Map writable items to driver commands
        let shared = self.shared.lock().unwrap();
        match self.path.as_str() {
            "/Mode" => {
                let m = sv
                    .as_u64()
                    .map(|v| v as u8)
                    .or_else(|| sv.as_i64().map(|v| v as u8))
                    .unwrap_or(0);
                let _ = shared.commands_tx.send(DriverCommand::SetMode(m));
            }
            "/StartStop" => {
                // Use the already-normalized value (0/1) to avoid mismatches between cache/signals and driver command
                let v: u8 = normalized_json
                    .as_u64()
                    .map(|u| if u > 0 { 1 } else { 0 })
                    .or_else(|| normalized_json.as_i64().map(|i| if i > 0 { 1 } else { 0 }))
                    .or_else(|| normalized_json.as_bool().map(|b| if b { 1 } else { 0 }))
                    .unwrap_or(0);
                let _ = shared.commands_tx.send(DriverCommand::SetStartStop(v));
            }
            "/SetCurrent" => {
                let a = sv.as_f64().unwrap_or(0.0) as f32;
                let _ = shared.commands_tx.send(DriverCommand::SetCurrent(a));
            }
            "/EnableDisplay" | "/AutoStart" => {
                // Accept both boolean and numeric writes for convenience
                let v = sv
                    .as_u64()
                    .map(|x| x as u8)
                    .or_else(|| sv.as_i64().map(|x| x as u8))
                    .or_else(|| sv.as_bool().map(|b| if b { 1 } else { 0 }))
                    .unwrap_or(0);
                // Reflect value in cache already updated; also propagate via update signals
                // Not sending a driver command for these yet.
                let _ = v; // placeholder in case of future command mapping
            }
            _ => {}
        }

        0 // OK
    }

    /// Return a textual representation of the current value
    #[zbus(name = "GetText")]
    async fn get_text(&self) -> String {
        let val = {
            let shared = self.shared.lock().unwrap();
            shared
                .paths
                .get(&self.path)
                .cloned()
                .unwrap_or(serde_json::json!(0))
        };
        match val {
            serde_json::Value::Number(n) => {
                if let Some(f) = n.as_f64() {
                    format!("{:.2}", f)
                } else {
                    n.to_string()
                }
            }
            serde_json::Value::String(s) => s,
            serde_json::Value::Bool(b) => b.to_string(),
            _ => val.to_string(),
        }
    }

    #[zbus(signal)]
    async fn properties_changed(
        ctxt: &SignalEmitter<'_>,
        changes: std::collections::HashMap<&str, OwnedValue>,
    ) -> zbus::Result<()>;
}

/// VeDbus-style Tree node implementing com.victronenergy.BusItem for intermediate paths
struct TreeNode {
    path: String,
    shared: Arc<Mutex<DbusSharedState>>,
}

impl TreeNode {
    fn new(path: String, shared: Arc<Mutex<DbusSharedState>>) -> Self {
        Self { path, shared }
    }

    fn collect_subtree_map(&self, as_text: bool) -> std::collections::HashMap<String, OwnedValue> {
        use std::collections::HashMap;
        let shared = self.shared.lock().unwrap();
        let mut px = self.path.clone();
        if !px.ends_with('/') {
            px.push('/');
        }
        let mut result: HashMap<String, OwnedValue> = HashMap::new();
        for (path, val) in shared.paths.iter() {
            if path.starts_with(&px) {
                let suffix = &path[px.len()..];
                let ov = if as_text {
                    let text = format_text_value(val);
                    OwnedValue::try_from(Value::from(text.as_str()))
                        .unwrap_or_else(|_| OwnedValue::from(0i64))
                } else {
                    BusItem::serde_to_owned_value(val)
                };
                result.insert(suffix.to_string(), ov);
            }
        }
        result
    }
}

#[zbus::interface(name = "com.victronenergy.BusItem")]
impl TreeNode {
    /// Return a dictionary of child items under this node
    #[zbus(name = "GetValue")]
    async fn get_value(&self) -> OwnedValue {
        OwnedValue::from(self.collect_subtree_map(false))
    }

    /// Return a dictionary of child items' text under this node
    #[zbus(name = "GetText")]
    async fn get_text(&self) -> OwnedValue {
        OwnedValue::from(self.collect_subtree_map(true))
    }
}

/// Helper to format a JSON value into the textual representation used by GetText
fn format_text_value(val: &serde_json::Value) -> String {
    match val {
        serde_json::Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                format!("{:.2}", f)
            } else {
                n.to_string()
            }
        }
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Bool(b) => b.to_string(),
        _ => val.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_string_is_mapped_to_dbus_string() {
        let json_val = serde_json::json!("Phaeton EV Charger");
        let owned = BusItem::serde_to_owned_value(&json_val);

        // Ensure it is represented as a D-Bus string and round-trips back
        let back = BusItem::owned_value_to_serde(&owned);
        assert_eq!(back, json_val);
    }
}
