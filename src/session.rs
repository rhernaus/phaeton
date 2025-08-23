//! Charging session management for Phaeton
//!
//! This module handles tracking and management of charging sessions,
//! including energy consumption, duration, and cost calculations.

use crate::error::{PhaetonError, Result};
use crate::logging::get_logger;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Charging session state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChargingSession {
    /// Unique session ID
    pub id: String,

    /// Start time of the session
    pub start_time: DateTime<Utc>,

    /// End time of the session (if completed)
    pub end_time: Option<DateTime<Utc>>,

    /// Energy consumed at start (kWh)
    pub start_energy_kwh: f64,

    /// Energy consumed at end (if completed)
    pub end_energy_kwh: Option<f64>,

    /// Total energy delivered in this session
    pub energy_delivered_kwh: f64,

    /// Peak power recorded during session
    pub peak_power_w: f64,

    /// Average power during session
    pub average_power_w: f64,

    /// Session cost (if pricing available)
    pub cost: Option<f64>,

    /// Session status
    pub status: SessionStatus,
}

/// Session status enumeration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionStatus {
    /// Session is currently active
    Active,

    /// Session completed successfully
    Completed,

    /// Session was interrupted
    Interrupted,

    /// Session failed
    Failed,
}

/// Session manager for tracking charging sessions
pub struct ChargingSessionManager {
    /// Current active session
    pub current_session: Option<ChargingSession>,

    /// Last completed session
    pub last_session: Option<ChargingSession>,

    /// Session history (limited size)
    session_history: Vec<ChargingSession>,

    /// Maximum history size
    max_history_size: usize,

    /// Logger
    logger: crate::logging::StructuredLogger,
}

impl ChargingSessionManager {
    /// Create a new session manager
    pub fn new(max_history_size: usize) -> Self {
        let logger = get_logger("session");

        Self {
            current_session: None,
            last_session: None,
            session_history: Vec::with_capacity(max_history_size),
            max_history_size,
            logger,
        }
    }

    /// Start a new charging session
    pub fn start_session(&mut self, start_energy_kwh: f64) -> Result<()> {
        if self.current_session.is_some() {
            return Err(PhaetonError::generic("Session already active"));
        }

        let session = ChargingSession {
            id: uuid::Uuid::new_v4().to_string(),
            start_time: Utc::now(),
            end_time: None,
            start_energy_kwh,
            end_energy_kwh: None,
            energy_delivered_kwh: 0.0,
            peak_power_w: 0.0,
            average_power_w: 0.0,
            cost: None,
            status: SessionStatus::Active,
        };

        self.logger
            .info(&format!("Started charging session {}", session.id));
        self.current_session = Some(session);

        Ok(())
    }

    /// Update current session with power and energy data
    pub fn update(&mut self, power_w: f64, energy_kwh: f64) -> Result<()> {
        if let Some(ref mut session) = self.current_session {
            // Update energy delivered
            session.energy_delivered_kwh = energy_kwh - session.start_energy_kwh;

            // Update peak power
            if power_w > session.peak_power_w {
                session.peak_power_w = power_w;
            }

            // Update average power (simple moving average)
            let duration_hours = (Utc::now() - session.start_time).num_seconds() as f64 / 3600.0;
            if duration_hours > 0.0 {
                session.average_power_w = session.energy_delivered_kwh / duration_hours * 1000.0;
            }
        }

        Ok(())
    }

    /// End the current session
    pub fn end_session(&mut self, end_energy_kwh: f64) -> Result<()> {
        if let Some(mut session) = self.current_session.take() {
            session.end_time = Some(Utc::now());
            session.end_energy_kwh = Some(end_energy_kwh);
            let energy_delivered = end_energy_kwh - session.start_energy_kwh;
            session.energy_delivered_kwh = energy_delivered;
            session.status = SessionStatus::Completed;

            // Move to last session and add to history
            self.last_session = Some(session.clone());

            // Add to history, maintaining max size
            self.session_history.push(session);
            if self.session_history.len() > self.max_history_size {
                self.session_history.remove(0);
            }

            self.logger.info(&format!(
                "Ended charging session, delivered {:.3} kWh",
                energy_delivered
            ));

            Ok(())
        } else {
            Err(PhaetonError::generic("No active session to end"))
        }
    }

    /// Get session statistics
    pub fn get_session_stats(&self) -> serde_json::Value {
        let mut stats = serde_json::Map::new();

        if let Some(ref session) = self.current_session {
            stats.insert("session_active".to_string(), true.into());
            stats.insert(
                "session_duration_min".to_string(),
                (((Utc::now() - session.start_time).num_seconds() / 60) as u64).into(),
            );
            stats.insert(
                "energy_delivered_kwh".to_string(),
                session.energy_delivered_kwh.into(),
            );
        } else {
            stats.insert("session_active".to_string(), false.into());
            stats.insert("session_duration_min".to_string(), serde_json::Value::Null);
            stats.insert("energy_delivered_kwh".to_string(), serde_json::Value::Null);
        }

        serde_json::Value::Object(stats)
    }

    /// Get session state for persistence
    pub fn get_state(&self) -> serde_json::Value {
        // TODO: Implement state serialization
        serde_json::Value::Null
    }

    /// Restore session state from persistence
    pub fn restore_state(&mut self, _state: serde_json::Value) -> Result<()> {
        // TODO: Implement state restoration
        Ok(())
    }
}

impl Default for ChargingSessionManager {
    fn default() -> Self {
        Self::new(100) // Default history size of 100 sessions
    }
}
