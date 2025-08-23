//! Structured logging and tracing for Phaeton
//!
//! This module provides comprehensive logging functionality with support for
//! structured logging, log rotation, and integration with the tracing ecosystem.

use crate::config::LoggingConfig;
use crate::error::{PhaetonError, Result};
use tracing::{Level, debug, error, info, trace, warn};
use tracing_appender::{non_blocking, rolling};
use tracing_subscriber::{EnvFilter, Layer, fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// Initialize logging system based on configuration
pub fn init_logging(config: &LoggingConfig) -> Result<()> {
    // Parse log level
    let level = parse_log_level(&config.level)?;

    // Create environment filter
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| format!("phaeton={},tokio_modbus=warn", level).into());

    // Set up log file appender with rotation
    let file_appender = rolling::Builder::new()
        .rotation(rolling::Rotation::DAILY)
        .filename_prefix("phaeton")
        .filename_suffix("log")
        .max_log_files(config.backup_count as usize)
        .build(&config.file)
        .map_err(|e| PhaetonError::io(format!("Failed to create log file appender: {}", e)))?;

    let (non_blocking_appender, _guard) = non_blocking(file_appender);

    // Create registry with multiple layers
    let registry = tracing_subscriber::registry().with(filter);

    // Add file logging layer
    let file_layer = fmt::layer()
        .with_writer(non_blocking_appender)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false);

    let file_layer = if config.json_format {
        file_layer.json().boxed()
    } else {
        file_layer.boxed()
    };

    // Set up the subscriber with file logging
    let subscriber = registry.with(file_layer);

    // Add console logging if enabled (simplified approach)
    if config.console_output {
        // For now, we'll rely on the default console output from the file layer
        // TODO: Implement proper dual output (file + console) when needed
    }

    // Initialize the subscriber
    subscriber.init();

    info!(
        "Logging initialized - level: {}, file: {}",
        level, config.file
    );

    Ok(())
}

/// Parse log level string to tracing Level
fn parse_log_level(level_str: &str) -> Result<Level> {
    match level_str.to_uppercase().as_str() {
        "TRACE" => Ok(Level::TRACE),
        "DEBUG" => Ok(Level::DEBUG),
        "INFO" => Ok(Level::INFO),
        "WARN" => Ok(Level::WARN),
        "ERROR" => Ok(Level::ERROR),
        _ => Err(PhaetonError::config(format!(
            "Invalid log level: {}",
            level_str
        ))),
    }
}

/// Context information for log messages
#[derive(Debug, Clone)]
pub struct LogContext {
    /// Component name (e.g., "driver", "modbus", "web")
    pub component: String,

    /// Session ID for tracking requests
    pub session_id: Option<String>,

    /// Device instance for multi-charger setups
    pub device_instance: Option<u32>,

    /// Additional context fields
    pub extra_fields: std::collections::HashMap<String, String>,
}

impl LogContext {
    /// Create a new log context
    pub fn new(component: &str) -> Self {
        Self {
            component: component.to_string(),
            session_id: None,
            device_instance: None,
            extra_fields: std::collections::HashMap::new(),
        }
    }

    /// Set session ID
    pub fn with_session_id(mut self, session_id: String) -> Self {
        self.session_id = Some(session_id);
        self
    }

    /// Set device instance
    pub fn with_device_instance(mut self, device_instance: u32) -> Self {
        self.device_instance = Some(device_instance);
        self
    }

    /// Add extra field
    pub fn with_field(mut self, key: &str, value: String) -> Self {
        self.extra_fields.insert(key.to_string(), value);
        self
    }
}

/// Structured logger with context
#[derive(Clone)]
pub struct StructuredLogger {
    context: LogContext,
}

impl StructuredLogger {
    /// Create a new structured logger with context
    pub fn new(context: LogContext) -> Self {
        Self { context }
    }

    /// Log an info message with context
    pub fn info(&self, message: &str) {
        let fields = self.format_fields();
        info!(%fields, "{}", message);
    }

    /// Log a warning message with context
    pub fn warn(&self, message: &str) {
        let fields = self.format_fields();
        warn!(%fields, "{}", message);
    }

    /// Log an error message with context
    pub fn error(&self, message: &str) {
        let fields = self.format_fields();
        error!(%fields, "{}", message);
    }

    /// Log a debug message with context
    pub fn debug(&self, message: &str) {
        let fields = self.format_fields();
        debug!(%fields, "{}", message);
    }

    /// Log a trace message with context
    pub fn trace(&self, message: &str) {
        let fields = self.format_fields();
        trace!(%fields, "{}", message);
    }

    /// Format context fields for logging
    fn format_fields(&self) -> String {
        let mut fields = vec![format!("component={}", self.context.component)];

        if let Some(ref session_id) = self.context.session_id {
            fields.push(format!("session_id={}", session_id));
        }

        if let Some(device_instance) = self.context.device_instance {
            fields.push(format!("device_instance={}", device_instance));
        }

        for (key, value) in &self.context.extra_fields {
            fields.push(format!("{}={}", key, value));
        }

        fields.join(",")
    }
}

/// Create a logger for a specific component
pub fn get_logger(component: &str) -> StructuredLogger {
    let context = LogContext::new(component);
    StructuredLogger::new(context)
}

/// Create a logger with full context
pub fn get_logger_with_context(context: LogContext) -> StructuredLogger {
    StructuredLogger::new(context)
}

/// Shutdown logging system gracefully
pub fn shutdown() {
    // The tracing system will automatically handle shutdown
    // when the application exits
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Once;

    static INIT: Once = Once::new();

    fn init_test_logging() {
        INIT.call_once(|| {
            let config = LoggingConfig::default();
            init_logging(&config).ok();
        });
    }

    #[test]
    fn test_parse_log_level() {
        assert_eq!(parse_log_level("DEBUG").unwrap(), Level::DEBUG);
        assert_eq!(parse_log_level("info").unwrap(), Level::INFO);
        assert_eq!(parse_log_level("ERROR").unwrap(), Level::ERROR);
        assert!(parse_log_level("invalid").is_err());
    }

    #[test]
    fn test_log_context() {
        let context = LogContext::new("test")
            .with_session_id("session_123".to_string())
            .with_device_instance(1)
            .with_field("key", "value".to_string());

        assert_eq!(context.component, "test");
        assert_eq!(context.session_id, Some("session_123".to_string()));
        assert_eq!(context.device_instance, Some(1));
        assert_eq!(context.extra_fields.get("key"), Some(&"value".to_string()));
    }

    #[test]
    fn test_structured_logger() {
        init_test_logging();

        let context = LogContext::new("test_component");
        let logger = StructuredLogger::new(context);

        // These should not panic
        logger.info("Test info message");
        logger.debug("Test debug message");
        logger.warn("Test warning message");
        logger.error("Test error message");
    }

    #[test]
    fn test_get_logger() {
        let logger = get_logger("test_component");
        assert_eq!(logger.context.component, "test_component");
    }
}
