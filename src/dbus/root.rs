use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedValue, Value};

use super::items::BusItem;
use super::shared::DbusSharedState;
use super::util::format_text_value;

pub struct RootBus {
    pub(crate) shared: Arc<Mutex<DbusSharedState>>,
}

#[zbus::interface(name = "com.victronenergy.BusItem")]
impl RootBus {
    #[zbus(name = "GetValue")]
    async fn get_value(&self) -> OwnedValue {
        let map = self.collect_subtree_map("/", false);
        OwnedValue::from(map)
    }

    #[zbus(name = "GetText")]
    async fn get_text(&self) -> OwnedValue {
        let map = self.collect_subtree_map("/", true);
        OwnedValue::from(map)
    }

    #[zbus(name = "GetItems")]
    async fn get_items(&self) -> HashMap<String, HashMap<String, OwnedValue>> {
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
    pub async fn items_changed(
        ctxt: &SignalEmitter<'_>,
        changes: HashMap<&str, HashMap<&str, OwnedValue>>,
    ) -> zbus::Result<()>;
}

impl RootBus {
    fn collect_subtree_map(&self, prefix: &str, as_text: bool) -> HashMap<String, OwnedValue> {
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

pub struct TreeNode {
    pub(crate) path: String,
    pub(crate) shared: Arc<Mutex<DbusSharedState>>,
}

impl TreeNode {
    pub fn new(path: String, shared: Arc<Mutex<DbusSharedState>>) -> Self {
        Self { path, shared }
    }

    fn collect_subtree_map(&self, as_text: bool) -> HashMap<String, OwnedValue> {
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
    #[zbus(name = "GetValue")]
    async fn get_value(&self) -> OwnedValue {
        OwnedValue::from(self.collect_subtree_map(false))
    }
    #[zbus(name = "GetText")]
    async fn get_text(&self) -> OwnedValue {
        OwnedValue::from(self.collect_subtree_map(true))
    }
}
