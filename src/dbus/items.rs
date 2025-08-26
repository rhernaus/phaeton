use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

use super::shared::DbusSharedState;
use super::util::format_text_value;

/// VeDbus-style BusItem implementing com.victronenergy.BusItem
pub struct BusItem {
    pub(crate) path: String,
    pub(crate) shared: Arc<Mutex<DbusSharedState>>,
}

impl BusItem {
    pub fn new(path: String, shared: Arc<Mutex<DbusSharedState>>) -> Self {
        Self { path, shared }
    }

    fn normalize_start_stop(value: &serde_json::Value) -> serde_json::Value {
        let v = match value {
            serde_json::Value::Bool(b) => {
                if *b {
                    1
                } else {
                    0
                }
            }
            serde_json::Value::Number(n) => {
                if n.as_u64().unwrap_or(0) > 0 || n.as_i64().unwrap_or(0) > 0 {
                    1
                } else {
                    0
                }
            }
            serde_json::Value::String(s) => {
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
    }

    fn normalize_mode(value: &serde_json::Value) -> serde_json::Value {
        let m: u8 = match value {
            serde_json::Value::Number(n) => {
                let v = n
                    .as_u64()
                    .or_else(|| n.as_i64().map(|i| i as u64))
                    .unwrap_or(0) as u8;
                match v {
                    0 => 0,
                    1 => 1,
                    2 => 2,
                    _ => 0,
                }
            }
            serde_json::Value::Bool(b) => {
                if *b {
                    1
                } else {
                    0
                }
            }
            serde_json::Value::String(s) => {
                let t = s.trim().to_ascii_lowercase();
                if t == "manual" || t == "0" {
                    0
                } else if t == "auto" || t == "1" {
                    1
                } else if t == "scheduled" || t == "schedule" || t == "2" {
                    2
                } else {
                    0
                }
            }
            _ => 0,
        };
        serde_json::json!(m)
    }

    fn normalize_value_for_path(&self, sv_local: &serde_json::Value) -> serde_json::Value {
        match self.path.as_str() {
            "/StartStop" => Self::normalize_start_stop(sv_local),
            "/Mode" => Self::normalize_mode(sv_local),
            _ => sv_local.clone(),
        }
    }

    fn dispatch_driver_command(
        &self,
        shared: &DbusSharedState,
        normalized_json: &serde_json::Value,
        original_sv: &serde_json::Value,
    ) {
        match self.path.as_str() {
            "/Mode" => {
                let m = normalized_json
                    .as_u64()
                    .map(|v| v as u8)
                    .or_else(|| normalized_json.as_i64().map(|v| v as u8))
                    .unwrap_or(0);
                let _ = shared
                    .commands_tx
                    .send(crate::driver::DriverCommand::SetMode(m));
            }
            "/StartStop" => {
                let v: u8 = normalized_json
                    .as_u64()
                    .map(|u| if u > 0 { 1 } else { 0 })
                    .or_else(|| normalized_json.as_i64().map(|i| if i > 0 { 1 } else { 0 }))
                    .or_else(|| normalized_json.as_bool().map(|b| if b { 1 } else { 0 }))
                    .unwrap_or(0);
                let _ = shared
                    .commands_tx
                    .send(crate::driver::DriverCommand::SetStartStop(v));
            }
            "/SetCurrent" => {
                let a = original_sv.as_f64().unwrap_or(0.0) as f32;
                let _ = shared
                    .commands_tx
                    .send(crate::driver::DriverCommand::SetCurrent(a));
            }
            _ => {}
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dbus::shared::DbusSharedState;
    use tokio::sync::mpsc;

    fn make_item(path: &str) -> BusItem {
        let (tx, _rx) = mpsc::unbounded_channel();
        let root = OwnedObjectPath::try_from("/").unwrap();
        let shared = Arc::new(Mutex::new(DbusSharedState::new(tx, root)));
        BusItem::new(path.to_string(), shared)
    }

    #[test]
    fn normalize_start_stop_various_inputs() {
        let item = make_item("/StartStop");
        assert_eq!(
            BusItem::normalize_start_stop(&serde_json::json!(true)),
            serde_json::json!(1)
        );
        assert_eq!(
            BusItem::normalize_start_stop(&serde_json::json!(0)),
            serde_json::json!(0)
        );
        assert_eq!(
            BusItem::normalize_start_stop(&serde_json::json!("On")),
            serde_json::json!(1)
        );
        assert_eq!(
            BusItem::normalize_start_stop(&serde_json::json!("disabled")),
            serde_json::json!(0)
        );
        // Ensure path-based normalization uses the right function
        assert_eq!(
            item.normalize_value_for_path(&serde_json::json!("true")),
            serde_json::json!(1)
        );
    }

    #[test]
    fn normalize_mode_various_inputs() {
        let item = make_item("/Mode");
        assert_eq!(
            BusItem::normalize_mode(&serde_json::json!(2)),
            serde_json::json!(2)
        );
        assert_eq!(
            BusItem::normalize_mode(&serde_json::json!("manual")),
            serde_json::json!(0)
        );
        assert_eq!(
            BusItem::normalize_mode(&serde_json::json!("Auto")),
            serde_json::json!(1)
        );
        assert_eq!(
            BusItem::normalize_mode(&serde_json::json!("scheduled")),
            serde_json::json!(2)
        );
        assert_eq!(
            item.normalize_value_for_path(&serde_json::json!("schedule")),
            serde_json::json!(2)
        );
    }

    #[test]
    fn owned_value_conversions_roundtrip() {
        let j = serde_json::json!({"a":1});
        // Complex types fallback to numeric 0 per implementation
        let ov = BusItem::serde_to_owned_value(&j);
        let back = BusItem::owned_value_to_serde(&ov);
        assert_eq!(back, serde_json::json!(0));

        // Primitives
        let ov_b = BusItem::serde_to_owned_value(&serde_json::json!(true));
        assert_eq!(
            BusItem::owned_value_to_serde(&ov_b),
            serde_json::json!(true)
        );

        let ov_i = BusItem::serde_to_owned_value(&serde_json::json!(-5));
        assert_eq!(BusItem::owned_value_to_serde(&ov_i), serde_json::json!(-5));

        let ov_u = BusItem::serde_to_owned_value(&serde_json::json!(5u64));
        assert_eq!(
            BusItem::owned_value_to_serde(&ov_u),
            serde_json::json!(5u64)
        );

        let ov_f = BusItem::serde_to_owned_value(&serde_json::json!(std::f64::consts::PI));
        assert_eq!(
            BusItem::owned_value_to_serde(&ov_f),
            serde_json::json!(std::f64::consts::PI)
        );
    }
}

#[zbus::interface(name = "com.victronenergy.BusItem")]
impl BusItem {
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

    #[zbus(name = "SetValue")]
    async fn set_value(&self, value: OwnedValue) -> i32 {
        let (conn_opt, root_path, normalized_json, sv) = {
            let mut shared = self.shared.lock().unwrap();
            if !shared.writable.contains(&self.path) {
                return 1;
            }
            let sv_local = Self::owned_value_to_serde(&value);
            let normalized = self.normalize_value_for_path(&sv_local);
            shared.paths.insert(self.path.clone(), normalized.clone());
            (
                shared.connection.clone(),
                shared.root_path.clone(),
                normalized,
                sv_local,
            )
        };

        if let Some(conn) = conn_opt {
            if let Ok(obj_path) = OwnedObjectPath::try_from(self.path.as_str())
                && let Ok(item_ctx) = SignalEmitter::new(&conn, obj_path)
            {
                let mut changes: HashMap<&str, OwnedValue> = HashMap::new();
                changes.insert("Value", BusItem::serde_to_owned_value(&normalized_json));
                let text = format_text_value(&normalized_json);
                if let Ok(text_ov) = OwnedValue::try_from(Value::from(text.as_str())) {
                    changes.insert("Text", text_ov);
                }
                let _ = BusItem::properties_changed(&item_ctx, changes).await;
            }
            if let Ok(root_ctx) = SignalEmitter::new(&conn, root_path) {
                let mut inner: HashMap<&str, OwnedValue> = HashMap::new();
                inner.insert("Value", BusItem::serde_to_owned_value(&normalized_json));
                let text = format_text_value(&normalized_json);
                if let Ok(text_ov) = OwnedValue::try_from(Value::from(text.as_str())) {
                    inner.insert("Text", text_ov);
                }
                let mut outer: HashMap<&str, HashMap<&str, OwnedValue>> = HashMap::new();
                outer.insert(self.path.as_str(), inner);
                let _ = crate::dbus::RootBus::items_changed(&root_ctx, outer).await;
            }
        }

        let shared = self.shared.lock().unwrap();
        self.dispatch_driver_command(&shared, &normalized_json, &sv);

        0
    }

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
        format_text_value(&val)
    }

    #[zbus(signal)]
    pub async fn properties_changed(
        ctxt: &SignalEmitter<'_>,
        changes: HashMap<&str, OwnedValue>,
    ) -> zbus::Result<()>;
}
