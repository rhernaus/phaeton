use tracing::{debug, error, info, trace, warn};

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
    pub(crate) context: LogContext,
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
    StructuredLogger::new(LogContext::new(component))
}
/// Create a logger with full context
pub fn get_logger_with_context(context: LogContext) -> StructuredLogger {
    StructuredLogger::new(context)
}
