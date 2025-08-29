//! Structured logging and tracing for Phaeton
//!
//! This module provides comprehensive logging functionality with support for
//! structured logging, log rotation, and integration with the tracing ecosystem.

use crate::config::LoggingConfig;
use crate::error::{PhaetonError, Result};
use std::path::Path;
use tracing::{Level, info};
use tracing_appender::{non_blocking, rolling};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::{EnvFilter, Layer, fmt, layer::SubscriberExt, util::SubscriberInitExt};

mod broadcast;
mod level;
mod state;
mod structured;

pub use crate::logging::broadcast::subscribe_log_lines;
use crate::logging::broadcast::{BroadcastMakeWriter, get_or_init_log_tx};
pub use crate::logging::level::set_web_log_level_str;
use crate::logging::level::{min_level, parse_line_level, parse_log_level};
pub use crate::logging::state::{get_web_log_level, set_web_log_level};
pub use crate::logging::structured::{
    LogContext, StructuredLogger, get_logger, get_logger_with_context,
};

use crate::logging::state::{INIT_ERROR, INIT_ONCE, LOG_GUARD};

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
                let _ = crate::logging::state::WEB_LOG_LEVEL.set(std::sync::RwLock::new(web_level));
                return Ok(());
            }

            init_file_logging(config, filter, console_level, file_level, web_level)?;
            // Initialize runtime web level
            let _ = crate::logging::state::WEB_LOG_LEVEL.set(std::sync::RwLock::new(web_level));
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
    // Respect the configured file path: if it points to a file, derive the
    // directory, prefix (file stem) and suffix (extension). If it points to a
    // directory, default to "phaeton" + ".log" inside that directory.
    let (dir_path, file_prefix, file_suffix) = {
        let p = Path::new(&config.file);
        if p.extension().is_some() {
            let dir = p.parent().unwrap_or_else(|| Path::new("."));
            let stem = p
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("phaeton")
                .to_string();
            let ext = p
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("log")
                .to_string();
            (dir.to_path_buf(), stem, ext)
        } else {
            (p.to_path_buf(), "phaeton".to_string(), "log".to_string())
        }
    };

    let file_appender = rolling::Builder::new()
        .rotation(rolling::Rotation::DAILY)
        .filename_prefix(file_prefix)
        .filename_suffix(file_suffix)
        .max_log_files(config.backup_count as usize)
        .build(&dir_path)
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

/// Shutdown logging system gracefully
pub fn shutdown() {
    // The tracing system will automatically handle shutdown
    // when the application exits
}

/// Whether a formatted line should be emitted to the web SSE stream given the current runtime web level
pub fn should_emit_to_web(line: &str) -> bool {
    let current = get_web_log_level();
    match parse_line_level(line) {
        Some(line_lvl) => {
            crate::logging::level::level_rank(line_lvl)
                >= crate::logging::level::level_rank(current)
        }
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
