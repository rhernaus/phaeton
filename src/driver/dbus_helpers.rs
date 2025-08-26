use crate::error::Result;

impl super::AlfenDriver {
    pub fn get_db_value(&self, path: &str) -> Option<serde_json::Value> {
        if let Some(d) = &self.dbus {
            if let Ok(guard) = d.try_lock() {
                let shared = guard.shared.lock().unwrap();
                shared.paths.get(path).cloned()
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn get_dbus_cache_snapshot(&self) -> serde_json::Value {
        let mut root = serde_json::Map::new();
        for key in [
            "/DeviceInstance",
            "/ProductName",
            "/FirmwareVersion",
            "/Serial",
            "/Ac/Power",
            "/Ac/Energy/Forward",
            "/Ac/Current",
            "/Ac/PhaseCount",
            "/Status",
            "/Mode",
            "/StartStop",
            "/SetCurrent",
        ] {
            if let Some(v) = self.get_db_value(key) {
                root.insert(key.to_string(), v);
            }
        }
        serde_json::Value::Object(root)
    }

    pub fn subscribe_status(&self) -> tokio::sync::broadcast::Receiver<String> {
        self.status_tx.subscribe()
    }

    pub(crate) async fn refresh_charger_identity(&mut self) -> Result<()> {
        if self.modbus_manager.is_none() || self.dbus.is_none() {
            return Ok(());
        }
        let manager = self.modbus_manager.as_mut().unwrap();

        let manufacturer = manager
            .execute_with_reconnect(|client| {
                let id = self.config.modbus.station_slave_id;
                let addr = self.config.registers.manufacturer;
                let cnt = self.config.registers.manufacturer_count;
                Box::pin(async move { client.read_holding_registers(id, addr, cnt).await })
            })
            .await
            .ok()
            .map(|regs| crate::modbus::decode_string(&regs, None).unwrap_or_default())
            .unwrap_or_default();

        let firmware = manager
            .execute_with_reconnect(|client| {
                let id = self.config.modbus.station_slave_id;
                let addr = self.config.registers.firmware_version;
                let cnt = self.config.registers.firmware_version_count;
                Box::pin(async move { client.read_holding_registers(id, addr, cnt).await })
            })
            .await
            .ok()
            .map(|regs| crate::modbus::decode_string(&regs, None).unwrap_or_default())
            .unwrap_or_default();

        let serial = manager
            .execute_with_reconnect(|client| {
                let id = self.config.modbus.station_slave_id;
                let addr = self.config.registers.station_serial;
                let cnt = self.config.registers.station_serial_count;
                Box::pin(async move { client.read_holding_registers(id, addr, cnt).await })
            })
            .await
            .ok()
            .map(|regs| crate::modbus::decode_string(&regs, None).unwrap_or_default())
            .unwrap_or_default();

        if let Some(dbus) = &self.dbus {
            let mut updates: Vec<(String, serde_json::Value)> = Vec::with_capacity(3);
            if !manufacturer.is_empty() {
                let pname = format!("{} EV Charger", manufacturer);
                updates.push(("/ProductName".to_string(), serde_json::json!(pname)));
            }
            if !firmware.is_empty() {
                updates.push((
                    "/FirmwareVersion".to_string(),
                    serde_json::json!(firmware.clone()),
                ));
            }
            if !serial.is_empty() {
                updates.push(("/Serial".to_string(), serde_json::json!(serial.clone())));
            }
            if !updates.is_empty() {
                let product_name = if !manufacturer.is_empty() {
                    format!("{} EV Charger", manufacturer)
                } else {
                    "Alfen EV Charger".to_string()
                };
                self.logger.info(&format!(
                    "Publishing charger identity: product_name='{}', firmware='{}', serial='{}'",
                    product_name, firmware, serial
                ));
                let _ = dbus.lock().await.update_paths(updates).await;
            } else {
                self.logger
                    .warn("Charger identity not available via Modbus; leaving defaults");
            }
        }

        if !manufacturer.is_empty() {
            self.product_name = Some(format!("{} EV Charger", manufacturer));
        }
        if !firmware.is_empty() {
            self.firmware_version = Some(firmware.clone());
        }
        if !serial.is_empty() {
            self.serial = Some(serial.clone());
        }
        Ok(())
    }

    pub(crate) async fn try_start_dbus_with_identity(&mut self) -> Result<()> {
        let mut dbus =
            crate::dbus::DbusService::new(self.config.device_instance, self.commands_tx.clone())
                .await?;
        dbus.start().await?;
        self.dbus = Some(std::sync::Arc::new(tokio::sync::Mutex::new(dbus)));

        self.publish_initial_dbus_paths().await;
        self.ensure_control_items().await;

        let _ = self.refresh_charger_identity().await;

        let snapshot = std::sync::Arc::new(self.build_typed_snapshot(None));
        let _ = self.status_snapshot_tx.send(snapshot);
        Ok(())
    }

    async fn publish_initial_dbus_paths(&self) {
        if let Some(d) = &self.dbus {
            let conn_str = format!(
                "Modbus TCP at {}:{}",
                self.config.modbus.ip, self.config.modbus.port
            );
            let _ = d
                .lock()
                .await
                .update_paths([
                    (
                        "/Mgmt/ProcessName".to_string(),
                        serde_json::json!("phaeton"),
                    ),
                    (
                        "/Mgmt/ProcessVersion".to_string(),
                        serde_json::json!(env!("CARGO_PKG_VERSION")),
                    ),
                    ("/Mgmt/Connection".to_string(), serde_json::json!(conn_str)),
                    (
                        "/DeviceInstance".to_string(),
                        serde_json::json!(self.config.device_instance),
                    ),
                    ("/ProductId".to_string(), serde_json::json!(0xC024u32)),
                    ("/Connected".to_string(), serde_json::json!(1u8)),
                    ("/Model".to_string(), serde_json::json!("AC22NS")),
                ])
                .await;
        }
    }

    async fn ensure_control_items(&self) {
        if let Some(d) = &self.dbus {
            let start_stop_init: u8 = self.start_stop as u8;
            let _ = d
                .lock()
                .await
                .ensure_item("/Mode", serde_json::json!(self.current_mode as u8), true)
                .await;
            let _ = d
                .lock()
                .await
                .ensure_item("/StartStop", serde_json::json!(start_stop_init), true)
                .await;
            let _ = d
                .lock()
                .await
                .ensure_item(
                    "/SetCurrent",
                    serde_json::json!(self.intended_set_current),
                    true,
                )
                .await;
            let _ = d
                .lock()
                .await
                .ensure_item("/Position", serde_json::json!(0u8), true)
                .await;
            let _ = d
                .lock()
                .await
                .ensure_item("/AutoStart", serde_json::json!(0u8), true)
                .await;
            let _ = d
                .lock()
                .await
                .ensure_item("/EnableDisplay", serde_json::json!(0u8), true)
                .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::AlfenDriver;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn get_db_value_and_cache_snapshot() {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut d = AlfenDriver::new(rx, tx.clone()).await.unwrap();

        // Prepare a D-Bus service and attach
        let svc = crate::dbus::DbusService::new(d.config.device_instance, tx)
            .await
            .unwrap();
        {
            let mut shared = svc.shared.lock().unwrap();
            shared.paths.insert(
                "/ProductName".to_string(),
                serde_json::json!("Test Charger"),
            );
            shared
                .paths
                .insert("/Ac/Power".to_string(), serde_json::json!(1234.0));
            shared
                .paths
                .insert("/SetCurrent".to_string(), serde_json::json!(6.5));
        }
        d.dbus = Some(std::sync::Arc::new(tokio::sync::Mutex::new(svc)));

        // get_db_value should return inserted values
        let pname = d.get_db_value("/ProductName");
        assert_eq!(pname, Some(serde_json::json!("Test Charger")));

        // get_dbus_cache_snapshot should include only known keys that exist
        let snap = d.get_dbus_cache_snapshot();
        let obj = snap.as_object().unwrap();
        assert_eq!(
            obj.get("/ProductName").unwrap(),
            &serde_json::json!("Test Charger")
        );
        assert_eq!(obj.get("/Ac/Power").unwrap(), &serde_json::json!(1234.0));
        assert_eq!(obj.get("/SetCurrent").unwrap(), &serde_json::json!(6.5));
        // Unset key should be absent
        assert!(obj.get("/Serial").is_none());
    }

    #[tokio::test]
    async fn subscribe_status_receives_messages() {
        let (tx, rx) = mpsc::unbounded_channel();
        let d = AlfenDriver::new(rx, tx).await.unwrap();

        let mut rx_status = d.subscribe_status();
        // Send a message via the driver's broadcast channel
        let _ = d.status_tx.send("hello".to_string());
        let msg = rx_status.recv().await.unwrap();
        assert_eq!(msg, "hello");
    }
}
