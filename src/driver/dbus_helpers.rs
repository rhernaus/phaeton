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
            .read_holding_registers(
                self.config.modbus.station_slave_id,
                self.config.registers.manufacturer,
                self.config.registers.manufacturer_count,
            )
            .await
            .ok()
            .map(|regs| crate::modbus::decode_string(&regs, None).unwrap_or_default())
            .unwrap_or_default();

        let firmware = manager
            .read_holding_registers(
                self.config.modbus.station_slave_id,
                self.config.registers.firmware_version,
                self.config.registers.firmware_version_count,
            )
            .await
            .ok()
            .map(|regs| crate::modbus::decode_string(&regs, None).unwrap_or_default())
            .unwrap_or_default();

        let serial = manager
            .read_holding_registers(
                self.config.modbus.station_slave_id,
                self.config.registers.station_serial,
                self.config.registers.station_serial_count,
            )
            .await
            .ok()
            .map(|regs| crate::modbus::decode_string(&regs, None).unwrap_or_default())
            .unwrap_or_default();

        // Read Station Max Current once per successful connection
        let station_max_current = manager
            .read_holding_registers(
                self.config.modbus.station_slave_id,
                self.config.registers.station_max_current,
                2,
            )
            .await
            .ok()
            .and_then(|regs| {
                if regs.len() >= 2 {
                    crate::modbus::decode_32bit_float(&regs[0..2]).ok()
                } else {
                    None
                }
            })
            .filter(|v| v.is_finite() && *v > 0.0);

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
            if let Some(maxc) = station_max_current {
                updates.push(("/MaxCurrent".to_string(), serde_json::json!(maxc)));
            }
            self.publish_identity_updates(dbus, &manufacturer, &firmware, &serial, updates)
                .await;
        }

        self.update_cached_identity(&manufacturer, &firmware, &serial);
        if let Some(maxc) = station_max_current {
            self.station_max_current = maxc;
        }
        Ok(())
    }

    async fn publish_identity_updates(
        &self,
        dbus: &std::sync::Arc<tokio::sync::Mutex<crate::dbus::DbusService>>,
        manufacturer: &str,
        firmware: &str,
        serial: &str,
        updates: Vec<(String, serde_json::Value)>,
    ) {
        if updates.is_empty() {
            self.logger
                .warn("Charger identity not available via Modbus; leaving defaults");
            return;
        }
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
    }

    fn update_cached_identity(&mut self, manufacturer: &str, firmware: &str, serial: &str) {
        if !manufacturer.is_empty() {
            self.product_name = Some(format!("{} EV Charger", manufacturer));
        }
        if !firmware.is_empty() {
            self.firmware_version = Some(firmware.to_string());
        }
        if !serial.is_empty() {
            self.serial = Some(serial.to_string());
        }
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

    #[tokio::test]
    async fn publish_initial_and_ensure_controls_populate_paths() {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut d = AlfenDriver::new(rx, tx.clone()).await.unwrap();

        // Attach a D-Bus service without real connection
        let svc = crate::dbus::DbusService::new(d.config.device_instance, tx)
            .await
            .unwrap();
        let svc_arc = std::sync::Arc::new(tokio::sync::Mutex::new(svc));
        d.dbus = Some(svc_arc.clone());

        d.publish_initial_dbus_paths().await;
        d.ensure_control_items().await;

        {
            let svc_guard = svc_arc.lock().await;
            let shared = svc_guard.shared.lock().unwrap();
            // Management/identity basics
            assert!(shared.paths.contains_key("/Mgmt/ProcessName"));
            assert!(shared.paths.contains_key("/DeviceInstance"));
            // Controls created and writable
            for (k, should_write) in [
                ("/Mode".to_string(), true),
                ("/StartStop".to_string(), true),
                ("/SetCurrent".to_string(), true),
                ("/Position".to_string(), true),
                ("/AutoStart".to_string(), true),
                ("/EnableDisplay".to_string(), true),
            ] {
                assert!(shared.paths.contains_key(&k), "missing path {}", k);
                if should_write {
                    assert!(shared.writable.contains(&k), "path {} not writable", k);
                }
            }
        }
    }

    #[tokio::test]
    async fn refresh_charger_identity_updates_driver_and_dbus() {
        use crate::driver::modbus_like::ModbusLike;
        use std::collections::HashMap;

        struct MockModbusStrings {
            reads: HashMap<(u8, u16, u16), Vec<u16>>,
        }

        impl MockModbusStrings {
            fn new() -> Self {
                Self {
                    reads: HashMap::new(),
                }
            }
            fn regs_from_str(s: &str, reg_count: u16) -> Vec<u16> {
                let bytes = s.as_bytes();
                let mut out: Vec<u16> = Vec::new();
                let mut i = 0;
                while i < bytes.len() {
                    let hi = bytes[i] as u16;
                    let lo = if i + 1 < bytes.len() {
                        bytes[i + 1] as u16
                    } else {
                        0
                    };
                    out.push((hi << 8) | lo);
                    i += 2;
                }
                // Pad with zeros up to reg_count
                while out.len() < reg_count as usize {
                    out.push(0);
                }
                out
            }
            fn with_str(mut self, slave: u8, addr: u16, count: u16, s: &str) -> Self {
                self.reads
                    .insert((slave, addr, count), Self::regs_from_str(s, count));
                self
            }
        }

        #[async_trait::async_trait]
        impl ModbusLike for MockModbusStrings {
            fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
                self
            }
            async fn read_holding_registers(
                &mut self,
                slave_id: u8,
                address: u16,
                count: u16,
            ) -> crate::error::Result<Vec<u16>> {
                Ok(self
                    .reads
                    .get(&(slave_id, address, count))
                    .cloned()
                    .unwrap_or_default())
            }
            async fn write_multiple_registers(
                &mut self,
                _slave_id: u8,
                _address: u16,
                _values: &[u16],
            ) -> crate::error::Result<()> {
                Ok(())
            }
        }

        let (tx, rx) = mpsc::unbounded_channel();
        let mut d = AlfenDriver::new(rx, tx.clone()).await.unwrap();

        // Attach D-Bus service holder
        let svc = crate::dbus::DbusService::new(d.config.device_instance, tx)
            .await
            .unwrap();
        let svc_arc = std::sync::Arc::new(tokio::sync::Mutex::new(svc));
        d.dbus = Some(svc_arc.clone());

        // Install Modbus mock with string registers
        let cfg = d.config().clone();
        let mock = MockModbusStrings::new()
            .with_str(
                cfg.modbus.station_slave_id,
                cfg.registers.manufacturer,
                cfg.registers.manufacturer_count,
                "Alfen",
            )
            .with_str(
                cfg.modbus.station_slave_id,
                cfg.registers.firmware_version,
                cfg.registers.firmware_version_count,
                "7.2.0",
            )
            .with_str(
                cfg.modbus.station_slave_id,
                cfg.registers.station_serial,
                cfg.registers.station_serial_count,
                "SN123",
            );
        d.modbus_manager = Some(Box::new(mock));

        d.refresh_charger_identity().await.unwrap();

        // Driver identity updated
        assert_eq!(d.product_name.as_deref(), Some("Alfen EV Charger"));
        assert_eq!(d.firmware_version.as_deref(), Some("7.2.0"));
        assert_eq!(d.serial.as_deref(), Some("SN123"));

        // DBus paths updated
        let svc_guard = svc_arc.lock().await;
        let shared = svc_guard.shared.lock().unwrap();
        assert_eq!(
            shared.paths.get("/ProductName"),
            Some(&serde_json::json!("Alfen EV Charger"))
        );
        assert_eq!(
            shared.paths.get("/FirmwareVersion"),
            Some(&serde_json::json!("7.2.0"))
        );
        assert_eq!(
            shared.paths.get("/Serial"),
            Some(&serde_json::json!("SN123"))
        );
    }
}
