use std::collections::{HashMap, HashSet};
use tokio::sync::mpsc;
use zbus::Connection;
use zbus::zvariant::OwnedObjectPath;

use crate::driver::DriverCommand;

pub struct DbusSharedState {
    pub(crate) paths: HashMap<String, serde_json::Value>,
    pub(crate) writable: HashSet<String>,
    pub(crate) commands_tx: mpsc::UnboundedSender<DriverCommand>,
    pub(crate) connection: Option<Connection>,
    pub(crate) root_path: OwnedObjectPath,
}

impl DbusSharedState {
    pub fn new(
        commands_tx: mpsc::UnboundedSender<DriverCommand>,
        root_path: OwnedObjectPath,
    ) -> Self {
        Self {
            paths: HashMap::new(),
            writable: HashSet::new(),
            commands_tx,
            connection: None,
            root_path,
        }
    }
}
