use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use zbus::zvariant::OwnedObjectPath;
use zbus::{Connection, Result as ZbusResult, names::WellKnownName};

use crate::driver::DriverCommand;
use crate::driver::DriverSnapshot;
use crate::error::{PhaetonError, Result};
use crate::logging::get_logger;

use super::ev_charger::{EvCharger, EvChargerValues};
use super::items::BusItem;
use super::root::RootBus;
use super::shared::DbusSharedState;

pub struct DbusService {
    logger: crate::logging::StructuredLogger,
    service_name: String,
    connection: Option<Connection>,
    pub(crate) shared: Arc<Mutex<DbusSharedState>>,
    registered_paths: HashSet<String>,
    pub(crate) charger_path: OwnedObjectPath,
    commands_tx: mpsc::UnboundedSender<DriverCommand>,
}

impl DbusService {
    /// Export a typed driver snapshot to D-Bus paths
    pub async fn export_typed_snapshot(&mut self, snap: &DriverSnapshot) -> Result<()> {
        // Derive forward/session energy and charging time if available
        let (energy_forward, charging_time): (f64, i64) =
            if let Some(obj) = snap.session.as_object() {
                let fwd = obj
                    .get("energy_delivered_kwh")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let t = obj
                    .get("charging_time_sec")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                (fwd, t)
            } else {
                (0.0, 0)
            };

        // Map snapshot fields to Victron D-Bus paths
        let updates = [
            ("/Ac/Power".to_string(), serde_json::json!(snap.ac_power)),
            (
                "/Ac/Current".to_string(),
                serde_json::json!(snap.ac_current),
            ),
            ("/Current".to_string(), serde_json::json!(snap.ac_current)),
            (
                "/Ac/Energy/Total".to_string(),
                serde_json::json!(snap.total_energy_kwh),
            ),
            (
                "/Ac/Energy/Forward".to_string(),
                serde_json::json!(energy_forward),
            ),
            (
                "/Ac/PhaseCount".to_string(),
                serde_json::json!(snap.active_phases),
            ),
            (
                "/Ac/L1/Voltage".to_string(),
                serde_json::json!(snap.l1_voltage),
            ),
            (
                "/Ac/L2/Voltage".to_string(),
                serde_json::json!(snap.l2_voltage),
            ),
            (
                "/Ac/L3/Voltage".to_string(),
                serde_json::json!(snap.l3_voltage),
            ),
            (
                "/Ac/L1/Current".to_string(),
                serde_json::json!(snap.l1_current),
            ),
            (
                "/Ac/L2/Current".to_string(),
                serde_json::json!(snap.l2_current),
            ),
            (
                "/Ac/L3/Current".to_string(),
                serde_json::json!(snap.l3_current),
            ),
            ("/Ac/L1/Power".to_string(), serde_json::json!(snap.l1_power)),
            ("/Ac/L2/Power".to_string(), serde_json::json!(snap.l2_power)),
            ("/Ac/L3/Power".to_string(), serde_json::json!(snap.l3_power)),
            ("/Status".to_string(), serde_json::json!(snap.status)),
            (
                "/MaxCurrent".to_string(),
                serde_json::json!(snap.station_max_current),
            ),
            (
                "/ChargingTime".to_string(),
                serde_json::json!(charging_time),
            ),
            ("/Mode".to_string(), serde_json::json!(snap.mode)),
            ("/StartStop".to_string(), serde_json::json!(snap.start_stop)),
            (
                "/SetCurrent".to_string(),
                serde_json::json!(snap.set_current),
            ),
        ];
        self.update_paths(updates).await
    }
    pub async fn update_paths(
        &mut self,
        updates: impl IntoIterator<Item = (String, serde_json::Value)>,
    ) -> Result<()> {
        for (k, v) in updates {
            self.update_path(&k, v).await?;
        }
        Ok(())
    }

    pub async fn update_path(&mut self, path: &str, value: serde_json::Value) -> Result<()> {
        {
            let shared = self.shared.lock().unwrap();
            if let Some(old) = shared.paths.get(path)
                && old == &value
            {
                return Ok(());
            }
        }
        let _ = self.ensure_item(path, value.clone(), false).await;
        {
            let mut shared = self.shared.lock().unwrap();
            shared.paths.insert(path.to_string(), value.clone());
        }
        if let Some(conn) = &self.connection {
            let item_ctx = zbus::object_server::SignalEmitter::new(
                conn,
                zbus::zvariant::OwnedObjectPath::try_from(path).map_err(|e| {
                    PhaetonError::dbus(format!("Invalid object path '{}': {}", path, e))
                })?,
            )
            .map_err(|e| PhaetonError::dbus(format!("SignalEmitter new failed: {}", e)))?;
            let mut changes: std::collections::HashMap<&str, zbus::zvariant::OwnedValue> =
                std::collections::HashMap::new();
            changes.insert("Value", BusItem::serde_to_owned_value(&value));
            let text = crate::dbus::util::format_text_value(&value);
            let text_ov =
                zbus::zvariant::OwnedValue::try_from(zbus::zvariant::Value::from(text.as_str()))
                    .unwrap_or_else(|_| zbus::zvariant::OwnedValue::from(0i64));
            changes.insert("Text", text_ov);
            let _ = crate::dbus::items::BusItem::properties_changed(&item_ctx, changes).await;

            let root_ctx = zbus::object_server::SignalEmitter::new(conn, self.charger_path.clone())
                .map_err(|e| PhaetonError::dbus(format!("Root SignalEmitter failed: {}", e)))?;
            let mut inner: std::collections::HashMap<&str, zbus::zvariant::OwnedValue> =
                std::collections::HashMap::new();
            inner.insert("Value", BusItem::serde_to_owned_value(&value));
            let text = crate::dbus::util::format_text_value(&value);
            let text_ov =
                zbus::zvariant::OwnedValue::try_from(zbus::zvariant::Value::from(text.as_str()))
                    .unwrap_or_else(|_| zbus::zvariant::OwnedValue::from(0i64));
            inner.insert("Text", text_ov);
            let mut outer: std::collections::HashMap<
                &str,
                std::collections::HashMap<&str, zbus::zvariant::OwnedValue>,
            > = std::collections::HashMap::new();
            outer.insert(path, inner);
            let _ = crate::dbus::root::RootBus::items_changed(&root_ctx, outer).await;
        }
        Ok(())
    }
    pub async fn new(
        device_instance: u32,
        commands_tx: mpsc::UnboundedSender<DriverCommand>,
    ) -> Result<Self> {
        let logger = get_logger("dbus");
        logger.info("Initializing D-Bus service (zbus)");
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

    pub async fn start(&mut self) -> Result<()> {
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
        self.request_name(&connection)
            .await
            .map_err(|e| PhaetonError::dbus(format!("RequestName failed: {}", e)))?;
        self.logger
            .info(&format!("D-Bus service started: {}", self.service_name));

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

        let charger = EvCharger {
            values: Mutex::new(EvChargerValues::default()),
            commands_tx: self.commands_tx.clone(),
        };
        connection
            .object_server()
            .at(&self.charger_path, charger)
            .await
            .map_err(|e| PhaetonError::dbus(format!("Register object failed: {}", e)))?;
        let root = RootBus {
            shared: Arc::clone(&self.shared),
        };
        connection
            .object_server()
            .at(&self.charger_path, root)
            .await
            .map_err(|e| PhaetonError::dbus(format!("Register root BusItem failed: {}", e)))?;
        self.connection = Some(connection);
        {
            let mut shared = self.shared.lock().unwrap();
            shared.connection = Some(self.connection.as_ref().unwrap().clone());
        }
        Ok(())
    }

    pub async fn stop(&mut self) -> Result<()> {
        self.logger.info("Stopping D-Bus service");
        self.connection = None;
        Ok(())
    }

    pub async fn ensure_item(
        &mut self,
        path: &str,
        initial_value: serde_json::Value,
        writable: bool,
    ) -> Result<()> {
        let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if !segments.is_empty() {
            for i in 1..=segments.len() {
                let subpath = format!("/{}", segments[..i].join("/"));
                if !self.registered_paths.contains(&subpath) {
                    let obj_path = OwnedObjectPath::try_from(subpath.as_str()).map_err(|e| {
                        PhaetonError::dbus(format!("Invalid object path '{}': {}", subpath, e))
                    })?;
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
                        let node = crate::dbus::root::TreeNode::new(
                            subpath.clone(),
                            Arc::clone(&self.shared),
                        );
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
    pub async fn read_remote_value(
        &self,
        service_name: &str,
        path: &str,
    ) -> Result<serde_json::Value> {
        let conn = match &self.connection {
            Some(c) => c,
            None => return Err(PhaetonError::dbus("No D-Bus connection available")),
        };
        let proxy = tokio::time::timeout(
            std::time::Duration::from_millis(600),
            zbus::Proxy::new(conn, service_name, path, "com.victronenergy.BusItem"),
        )
        .await
        .map_err(|_| PhaetonError::dbus("DBus proxy creation timed out"))?
        .map_err(|e| PhaetonError::dbus(format!("Proxy creation failed: {}", e)))?;

        let val: zbus::zvariant::OwnedValue = tokio::time::timeout(
            std::time::Duration::from_millis(600),
            proxy.call("GetValue", &()),
        )
        .await
        .map_err(|_| PhaetonError::dbus("DBus GetValue timed out"))?
        .map_err(|e| PhaetonError::dbus(format!("GetValue call failed: {}", e)))?;

        Ok(crate::dbus::items::BusItem::owned_value_to_serde(&val))
    }

    /// List available D-Bus service names that start with the provided prefix
    pub async fn list_service_names_with_prefix(&self, prefix: &str) -> Result<Vec<String>> {
        let conn = match &self.connection {
            Some(c) => c,
            None => return Err(PhaetonError::dbus("No D-Bus connection available")),
        };
        let proxy = zbus::fdo::DBusProxy::new(conn)
            .await
            .map_err(|e| PhaetonError::dbus(format!("DBusProxy creation failed: {}", e)))?;
        let names = proxy
            .list_names()
            .await
            .map_err(|e| PhaetonError::dbus(format!("ListNames failed: {}", e)))?;
        Ok(names
            .into_iter()
            .map(|n| n.to_string())
            .filter(|n| n.starts_with(prefix))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn export_snapshot_populates_key_paths() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut svc = DbusService::new(0, tx).await.unwrap();

        let snap = DriverSnapshot {
            timestamp: "2020-01-01T00:00:00Z".to_string(),
            mode: 1,
            start_stop: 1,
            set_current: 6.0,
            applied_current: 6.0,
            station_max_current: 16.0,
            device_instance: 0,
            product_name: Some("Alfen NV EV Charger".to_string()),
            firmware: Some("7.2.0".to_string()),
            serial: Some("ABC".to_string()),
            status: 2,
            active_phases: 3,
            ac_power: 4000.0,
            ac_current: 6.0,
            l1_voltage: 230.0,
            l2_voltage: 230.0,
            l3_voltage: 230.0,
            l1_current: 6.0,
            l2_current: 6.0,
            l3_current: 6.0,
            l1_power: 1300.0,
            l2_power: 1300.0,
            l3_power: 1400.0,
            total_energy_kwh: 2628.0,
            pricing_currency: None,
            energy_rate: None,
            session: serde_json::json!({"charging_time_sec": 60, "energy_delivered_kwh": 0.064}),
            poll_duration_ms: Some(1000),
            total_polls: 10,
            overrun_count: 0,
            poll_interval_ms: 1000,
            excess_pv_power_w: 0.0,
            modbus_connected: Some(true),
            driver_state: "Running".to_string(),
            poll_steps_ms: None,
        };

        svc.export_typed_snapshot(&snap).await.unwrap();
        let shared = svc.shared.lock().unwrap();
        for key in [
            "/Ac/Power",
            "/Ac/Current",
            "/Current",
            "/Ac/Energy/Total",
            "/Ac/Energy/Forward",
            "/Ac/PhaseCount",
            "/Ac/L1/Voltage",
            "/Ac/L2/Voltage",
            "/Ac/L3/Voltage",
            "/Ac/L1/Current",
            "/Ac/L2/Current",
            "/Ac/L3/Current",
            "/Ac/L1/Power",
            "/Ac/L2/Power",
            "/Ac/L3/Power",
            "/Status",
            "/MaxCurrent",
            "/ChargingTime",
            "/Mode",
            "/StartStop",
            "/SetCurrent",
        ] {
            assert!(shared.paths.contains_key(key), "missing path: {}", key);
        }
    }
}
