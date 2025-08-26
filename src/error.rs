//! Error types and handling for Phaeton
//!
//! This module defines the error types used throughout the application,
//! providing consistent error handling and reporting.

use thiserror::Error;

/// Result type alias for Phaeton operations
pub type Result<T> = std::result::Result<T, PhaetonError>;

/// Main error type for Phaeton
#[derive(Debug, Error)]
pub enum PhaetonError {
    /// Configuration-related errors
    #[error("Configuration error: {message}")]
    Config { message: String },

    /// Modbus communication errors
    #[error("Modbus error: {message}")]
    Modbus { message: String },

    /// D-Bus communication errors
    #[error("D-Bus error: {message}")]
    DBus { message: String },

    /// HTTP/Web server errors
    #[error("Web server error: {message}")]
    Web { message: String },

    /// Serialization/deserialization errors
    #[error("Serialization error: {message}")]
    Serialization { message: String },

    /// File I/O errors
    #[error("I/O error: {message}")]
    Io { message: String },

    /// Network-related errors
    #[error("Network error: {message}")]
    Network { message: String },

    /// API integration errors (Tibber, Vehicle APIs)
    #[error("API error: {message}")]
    Api { message: String },

    /// Authentication/authorization errors
    #[error("Authentication error: {message}")]
    Auth { message: String },

    /// Validation errors
    #[error("Validation error: {field} - {message}")]
    Validation { field: String, message: String },

    /// Timeout errors
    #[error("Timeout error: {message}")]
    Timeout { message: String },

    /// Git/update related errors
    #[error("Update error: {message}")]
    Update { message: String },

    /// Generic errors with context
    #[error("Error: {message}")]
    Generic { message: String },
}

impl PhaetonError {
    /// Create a new configuration error
    pub fn config<S: Into<String>>(message: S) -> Self {
        PhaetonError::Config {
            message: message.into(),
        }
    }

    /// Create a new Modbus error
    pub fn modbus<S: Into<String>>(message: S) -> Self {
        PhaetonError::Modbus {
            message: message.into(),
        }
    }

    /// Create a new D-Bus error
    pub fn dbus<S: Into<String>>(message: S) -> Self {
        PhaetonError::DBus {
            message: message.into(),
        }
    }

    /// Create a new web error
    pub fn web<S: Into<String>>(message: S) -> Self {
        PhaetonError::Web {
            message: message.into(),
        }
    }

    /// Create a new validation error
    pub fn validation<S: Into<String>>(field: S, message: S) -> Self {
        PhaetonError::Validation {
            field: field.into(),
            message: message.into(),
        }
    }

    /// Create a new I/O error
    pub fn io<S: Into<String>>(message: S) -> Self {
        PhaetonError::Io {
            message: message.into(),
        }
    }

    /// Create a new network error
    pub fn network<S: Into<String>>(message: S) -> Self {
        PhaetonError::Network {
            message: message.into(),
        }
    }

    /// Create a new API error
    pub fn api<S: Into<String>>(message: S) -> Self {
        PhaetonError::Api {
            message: message.into(),
        }
    }

    /// Create a new timeout error
    pub fn timeout<S: Into<String>>(message: S) -> Self {
        PhaetonError::Timeout {
            message: message.into(),
        }
    }

    /// Create a new update error
    pub fn update<S: Into<String>>(message: S) -> Self {
        PhaetonError::Update {
            message: message.into(),
        }
    }

    /// Create a new auth error
    pub fn auth<S: Into<String>>(message: S) -> Self {
        PhaetonError::Auth {
            message: message.into(),
        }
    }

    /// Create a new generic error
    pub fn generic<S: Into<String>>(message: S) -> Self {
        PhaetonError::Generic {
            message: message.into(),
        }
    }
}

impl From<std::io::Error> for PhaetonError {
    fn from(err: std::io::Error) -> Self {
        PhaetonError::io(err.to_string())
    }
}

impl From<serde_yaml::Error> for PhaetonError {
    fn from(err: serde_yaml::Error) -> Self {
        PhaetonError::Serialization {
            message: err.to_string(),
        }
    }
}

impl From<serde_json::Error> for PhaetonError {
    fn from(err: serde_json::Error) -> Self {
        PhaetonError::Serialization {
            message: err.to_string(),
        }
    }
}

// Note: tokio_modbus::Error may not exist in this version, commented out for now
// impl From<tokio_modbus::Error> for PhaetonError {
//     fn from(err: tokio_modbus::Error) -> Self {
//         PhaetonError::modbus(err.to_string())
//     }
// }

#[cfg(feature = "tibber")]
impl From<reqwest::Error> for PhaetonError {
    fn from(err: reqwest::Error) -> Self {
        PhaetonError::network(err.to_string())
    }
}

// Note: zbus not included in this version, commented out for now
// impl From<zbus::Error> for PhaetonError {
//     fn from(err: zbus::Error) -> Self {
//         PhaetonError::dbus(err.to_string())
//     }
// }

// External config::ConfigError not used; we manage config locally

impl From<chrono::ParseError> for PhaetonError {
    fn from(err: chrono::ParseError) -> Self {
        PhaetonError::validation("datetime", &err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let err = PhaetonError::config("test config error");
        assert!(matches!(err, PhaetonError::Config { .. }));

        let err = PhaetonError::modbus("test modbus error");
        assert!(matches!(err, PhaetonError::Modbus { .. }));

        let err = PhaetonError::validation("field", "test validation error");
        assert!(matches!(err, PhaetonError::Validation { .. }));
    }

    #[test]
    fn test_error_display() {
        let err = PhaetonError::config("test error");
        let error_string = format!("{}", err);
        assert_eq!(error_string, "Configuration error: test error");

        let err = PhaetonError::validation("test_field", "invalid value");
        let error_string = format!("{}", err);
        assert_eq!(error_string, "Validation error: test_field - invalid value");
    }
}
