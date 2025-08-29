//! Structured logging and tracing for Phaeton
//!
//! This module provides comprehensive logging functionality with support for
//! structured logging, log rotation, and integration with the tracing ecosystem.

use crate::config::LoggingConfig;
use crate::error::{PhaetonError, Result};
use once_cell::sync::OnceCell;
use std::io::{self, Write};
use std::path::Path;
use std::sync::Once;
use std::sync::RwLock as StdRwLock;
use tokio::sync::broadcast;
use tracing::{Level, debug, error, info, trace, warn};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::{non_blocking, rolling};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::writer::MakeWriter;
use tracing_subscriber::{EnvFilter, Layer, fmt, layer::SubscriberExt, util::SubscriberInitExt};

// Keep the non-blocking worker guard alive for the entire process lifetime
static LOG_GUARD: OnceCell<WorkerGuard> = OnceCell::new();
static INIT_ONCE: Once = Once::new();
static INIT_ERROR: OnceCell<String> = OnceCell::new();
static LOG_BROADCAST_TX: OnceCell<broadcast::Sender<String>> = OnceCell::new();
static WEB_LOG_LEVEL: OnceCell<StdRwLock<Level>> = OnceCell::new();

#[derive(Clone)]
struct BroadcastMakeWriter {
    tx: broadcast::Sender<String>,
}

struct BroadcastWriter {
    tx: broadcast::Sender<String>,
    buffer: Vec<u8>,
}

impl<'a> MakeWriter<'a> for BroadcastMakeWriter {
    type Writer = BroadcastWriter;
    fn make_writer(&'a self) -> Self::Writer {
        BroadcastWriter {
            tx: self.tx.clone(),
            buffer: Vec::with_capacity(256),
        }
    }
}

impl Write for BroadcastWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for BroadcastWriter {
    fn drop(&mut self) {
        if self.buffer.is_empty() {
            return;
        }
        let mut line = String::from_utf8_lossy(&self.buffer).to_string();
        while line.ends_with('\n') || line.ends_with('\r') {
            line.pop();
        }
        let _ = self.tx.send(line);
    }
}

fn get_or_init_log_tx() -> broadcast::Sender<String> {
    LOG_BROADCAST_TX
        .get_or_init(|| {
            let (tx, _rx) = broadcast::channel::<String>(1024);
            tx
        })
        .clone()
}

/// Initialize logging system based on configuration
pub fn init_logging(config: &LoggingConfig) -> Result<()> {
    INIT_ONCE.call_once(|| {
        let init_result = (|| -> Result<()> {
            let base_level = parse_log_level(&config.level)?;

            // Determine most verbose base level so layer-specific filters can down-filter
            let console_level = config
                .console_level
                .as_ref()
                .and_then(|s| parse_log_level(s).ok())
                .unwrap_or(base_level);
            let file_level = config
                .file_level
                .as_ref()
                .and_then(|s| parse_log_level(s).ok())
                .unwrap_or(base_level);
            let web_level = config
                .web_level
                .as_ref()
                .and_then(|s| parse_log_level(s).ok())
                .unwrap_or(base_level);

            let most_verbose = min_level(min_level(console_level, file_level), web_level);
            let filter = build_env_filter(most_verbose);

            if should_use_console_only() {
                init_console_only_logging(filter, config.json_format, console_level, web_level);
                // Initialize runtime web level
                let _ = WEB_LOG_LEVEL.set(StdRwLock::new(web_level));
                return Ok(());
            }

            init_file_logging(config, filter, console_level, file_level, web_level)?;
            // Initialize runtime web level
            let _ = WEB_LOG_LEVEL.set(StdRwLock::new(web_level));
            Ok(())
        })();

        if let Err(e) = init_result {
            let _ = INIT_ERROR.set(e.to_string());
        }
    });

    if let Some(err) = INIT_ERROR.get() {
        return Err(PhaetonError::config(err.clone()));
    }
    Ok(())
}

fn build_env_filter(level: Level) -> EnvFilter {
    EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| format!("phaeton={},tokio_modbus=warn", level).into())
}

fn should_use_console_only() -> bool {
    cfg!(test) || std::env::var_os("PHAETON_DISABLE_FILE_LOG").is_some()
}

fn init_console_only_logging(
    filter: EnvFilter,
    json_format: bool,
    console_level: Level,
    web_level: Level,
) {
    let console_layer = {
        let layer = fmt::layer()
            .with_writer(std::io::stdout)
            .with_target(false)
            .with_thread_ids(false)
            .with_file(false);
        if json_format {
            layer
                .json()
                .with_filter(LevelFilter::from_level(console_level))
                .boxed()
        } else {
            layer
                .with_filter(LevelFilter::from_level(console_level))
                .boxed()
        }
    };

    let broadcast_layer = {
        let make = BroadcastMakeWriter {
            tx: get_or_init_log_tx(),
        };
        let base = fmt::layer()
            .with_writer(make)
            .with_target(false)
            .with_thread_ids(false)
            .with_file(false);
        // Always capture the most verbose for web; runtime filtering will apply in SSE
        if json_format {
            base.json().with_filter(LevelFilter::TRACE).boxed()
        } else {
            base.with_filter(LevelFilter::TRACE).boxed()
        }
    };

    tracing_subscriber::registry()
        .with(filter)
        .with(console_layer)
        .with(broadcast_layer)
        .init();

    info!(
        "Logging initialized - console_level: {:?}, web_level: {:?}, console-only",
        console_level, web_level
    );
}

fn init_file_logging(
    config: &LoggingConfig,
    filter: EnvFilter,
    console_level: Level,
    file_level: Level,
    web_level: Level,
) -> Result<()> {
    let registry = tracing_subscriber::registry().with(filter);

    // Set up log file appender with rotation
    let file_appender = rolling::Builder::new()
        .rotation(rolling::Rotation::DAILY)
        .filename_prefix("phaeton")
        .filename_suffix("log")
        .max_log_files(config.backup_count as usize)
        .build({
            // If config.file is a file path, use its parent dir; otherwise treat as dir
            let p = Path::new(&config.file);
            if p.extension().is_some() {
                p.parent().unwrap_or(p)
            } else {
                p
            }
        })
        .map_err(|e| PhaetonError::io(format!("Failed to create log file appender: {}", e)))?;

    let (non_blocking_appender, guard) = non_blocking(file_appender);
    let _ = LOG_GUARD.set(guard);

    let file_layer = {
        let base = fmt::layer()
            .with_writer(non_blocking_appender)
            .with_target(false)
            .with_thread_ids(false)
            .with_file(false);
        if config.json_format {
            base.json()
                .with_filter(LevelFilter::from_level(file_level))
                .boxed()
        } else {
            base.with_filter(LevelFilter::from_level(file_level))
                .boxed()
        }
    };

    let broadcast_layer = {
        let make = BroadcastMakeWriter {
            tx: get_or_init_log_tx(),
        };
        let base = fmt::layer()
            .with_writer(make)
            .with_target(false)
            .with_thread_ids(false)
            .with_file(false);
        // Always capture the most verbose for web; runtime filtering will apply in SSE
        if config.json_format {
            base.json().with_filter(LevelFilter::TRACE).boxed()
        } else {
            base.with_filter(LevelFilter::TRACE).boxed()
        }
    };

    let subscriber = registry.with(file_layer).with(broadcast_layer);

    if config.console_output {
        let console_layer = {
            let base = fmt::layer()
                .with_writer(std::io::stdout)
                .with_target(false)
                .with_thread_ids(false)
                .with_file(false);
            if config.json_format {
                base.json()
                    .with_filter(LevelFilter::from_level(console_level))
                    .boxed()
            } else {
                base.with_filter(LevelFilter::from_level(console_level))
                    .boxed()
            }
        };
        subscriber.with(console_layer).init();
    } else {
        subscriber.init();
    }

    info!(
        "Logging initialized - console_level: {:?}, file_level: {:?}, web_level: {:?}, file: {}",
        console_level, file_level, web_level, config.file
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

/// Subscribe to a stream of formatted log lines
pub fn subscribe_log_lines() -> broadcast::Receiver<String> {
    get_or_init_log_tx().subscribe()
}

/// Initialize or update the runtime web log level
pub fn set_web_log_level(new_level: Level) {
    if let Some(lock) = WEB_LOG_LEVEL.get() {
        if let Ok(mut guard) = lock.write() {
            *guard = new_level;
        }
    } else {
        let _ = WEB_LOG_LEVEL.set(StdRwLock::new(new_level));
    }
}

/// Helper to parse and set from string
pub fn set_web_log_level_str(level_str: &str) -> Result<()> {
    let lvl = parse_log_level(level_str)?;
    set_web_log_level(lvl);
    Ok(())
}

/// Get the current runtime web log level. Defaults to INFO if unset.
pub fn get_web_log_level() -> Level {
    if let Some(lock) = WEB_LOG_LEVEL.get() {
        if let Ok(guard) = lock.read() {
            *guard
        } else {
            Level::INFO
        }
    } else {
        Level::INFO
    }
}

fn level_rank(level: Level) -> u8 {
    match level {
        Level::TRACE => 0,
        Level::DEBUG => 1,
        Level::INFO => 2,
        Level::WARN => 3,
        Level::ERROR => 4,
    }
}

fn min_level(a: Level, b: Level) -> Level {
    if level_rank(a) <= level_rank(b) { a } else { b }
}

/// Try to parse a level out of a formatted log line
pub fn parse_line_level(line: &str) -> Option<Level> {
    // Try JSON format first: ... "level":"INFO" ...
    if line.contains("\"level\":\"TRACE\"") {
        return Some(Level::TRACE);
    }
    if line.contains("\"level\":\"DEBUG\"") {
        return Some(Level::DEBUG);
    }
    if line.contains("\"level\":\"INFO\"") {
        return Some(Level::INFO);
    }
    if line.contains("\"level\":\"WARN\"") {
        return Some(Level::WARN);
    }
    if line.contains("\"level\":\"ERROR\"") {
        return Some(Level::ERROR);
    }

    // Fallback to plain formatting: timestamp SPACE LEVEL SPACE ...
    if line.contains(" TRACE ") {
        return Some(Level::TRACE);
    }
    if line.contains(" DEBUG ") {
        return Some(Level::DEBUG);
    }
    if line.contains(" INFO ") {
        return Some(Level::INFO);
    }
    if line.contains(" WARN ") {
        return Some(Level::WARN);
    }
    if line.contains(" ERROR ") {
        return Some(Level::ERROR);
    }
    None
}

/// Whether a formatted line should be emitted to the web SSE stream given the current runtime web level
pub fn should_emit_to_web(line: &str) -> bool {
    let current = get_web_log_level();
    match parse_line_level(line) {
        Some(line_lvl) => level_rank(line_lvl) >= level_rank(current),
        None => true,
    }
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
