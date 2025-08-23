//! Persistence layer for configuration and state
//!
//! This module handles saving and loading persistent state including
//! configuration changes and driver state across restarts.

use crate::error::Result;
use crate::logging::get_logger;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Persistent state structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistentState {
    /// Mode (Manual, Auto, Scheduled)
    pub mode: u32,

    /// Start/stop state
    pub start_stop: u32,

    /// Set current value
    pub set_current: f32,

    /// Timestamp of insufficient solar condition
    pub insufficient_solar_start: f64,

    /// Session data
    pub session: serde_json::Value,
}

/// Persistence manager
pub struct PersistenceManager {
    file_path: String,
    state: PersistentState,
    logger: crate::logging::StructuredLogger,
}

impl PersistenceManager {
    /// Create a new persistence manager
    pub fn new(file_path: &str) -> Self {
        let logger = get_logger("persistence");
        let state = PersistentState::default();

        Self {
            file_path: file_path.to_string(),
            state,
            logger,
        }
    }

    /// Load state from disk
    pub fn load(&mut self) -> Result<()> {
        let path = Path::new(&self.file_path);

        if !path.exists() {
            self.logger
                .info("No persistent state file found, using defaults");
            return Ok(());
        }

        let contents = std::fs::read_to_string(path)?;
        self.state = serde_json::from_str(&contents)?;
        self.logger.info("Loaded persistent state from disk");

        Ok(())
    }

    /// Save state to disk
    pub fn save(&self) -> Result<()> {
        let contents = serde_json::to_string_pretty(&self.state)?;
        std::fs::write(&self.file_path, contents)?;
        self.logger.debug("Saved persistent state to disk");

        Ok(())
    }

    /// Accessors for core fields
    pub fn set_mode(&mut self, value: u32) {
        self.state.mode = value;
    }

    pub fn set_start_stop(&mut self, value: u32) {
        self.state.start_stop = value;
    }

    pub fn set_set_current(&mut self, value: f32) {
        self.state.set_current = value;
    }

    pub fn set_insufficient_solar_start(&mut self, value: f64) {
        self.state.insufficient_solar_start = value;
    }

    /// Get a value from persistent state (limited support)
    pub fn get<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Option<T> {
        let value = match key {
            "mode" => serde_json::to_value(self.state.mode).ok()?,
            "start_stop" => serde_json::to_value(self.state.start_stop).ok()?,
            "set_current" => serde_json::to_value(self.state.set_current).ok()?,
            "insufficient_solar_start" => {
                serde_json::to_value(self.state.insufficient_solar_start).ok()?
            }
            _ => return None,
        };
        serde_json::from_value(value).ok()
    }

    /// Set a value in persistent state
    pub fn set<T: Serialize>(&mut self, _key: &str, _value: T) -> Result<()> {
        // TODO: Implement key-based storage
        Ok(())
    }

    /// Update entire state
    pub fn update(&mut self, updates: serde_json::Value) -> Result<()> {
        if let Some(obj) = updates.as_object() {
            if let Some(v) = obj.get("mode").and_then(|v| v.as_u64()) {
                self.state.mode = v as u32;
            }
            if let Some(v) = obj.get("start_stop").and_then(|v| v.as_u64()) {
                self.state.start_stop = v as u32;
            }
            if let Some(v) = obj.get("set_current").and_then(|v| v.as_f64()) {
                self.state.set_current = v as f32;
            }
            if let Some(v) = obj.get("insufficient_solar_start").and_then(|v| v.as_f64()) {
                self.state.insufficient_solar_start = v;
            }
            if let Some(v) = obj.get("session") {
                self.state.session = v.clone();
            }
        }
        Ok(())
    }

    /// Get section from state
    pub fn get_section(&self, section: &str) -> Option<serde_json::Value> {
        match section {
            "session" => Some(self.state.session.clone()),
            _ => None,
        }
    }

    /// Set section in state
    pub fn set_section(&mut self, section: &str, data: serde_json::Value) -> Result<()> {
        if section == "session" {
            self.state.session = data;
        }
        Ok(())
    }
}

impl Default for PersistentState {
    fn default() -> Self {
        Self {
            mode: 0,       // Manual mode
            start_stop: 0, // Stopped
            set_current: 6.0,
            insufficient_solar_start: 0.0,
            session: serde_json::Value::Null,
        }
    }
}
