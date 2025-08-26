use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use zbus::zvariant::OwnedObjectPath;
use zbus::{Connection, Result as ZbusResult, names::WellKnownName};

use crate::driver::DriverCommand;
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
}
