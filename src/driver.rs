//! Core driver logic for Phaeton
//!
//! This module contains the main driver state machine and orchestration logic
//! that coordinates all the different components of the system.

use tokio::sync::{mpsc, watch};
use tokio::time::{Duration, interval};
use crate::error::{Result, PhaetonError};
use crate::config::Config;
use crate::logging::{get_logger, LogContext};
use crate::modbus::ModbusConnectionManager;

/// Main driver state
#[derive(Debug, Clone)]
pub enum DriverState {
    /// Driver is initializing
    Initializing,
    /// Driver is running normally
    Running,
    /// Driver is in error state
    Error(String),
    /// Driver is shutting down
    ShuttingDown,
}

/// Main driver for Phaeton
pub struct AlfenDriver {
    /// Configuration
    config: Config,

    /// Current driver state
    state: watch::Sender<DriverState>,

    /// Modbus connection manager
    modbus_manager: Option<ModbusConnectionManager>,

    /// Logger with context
    logger: crate::logging::StructuredLogger,

    /// Shutdown signal
    shutdown_tx: mpsc::UnboundedSender<()>,

    /// Shutdown receiver
    shutdown_rx: mpsc::UnboundedReceiver<()>,
}

impl AlfenDriver {
    /// Create a new driver instance
    pub async fn new() -> Result<Self> {
        let config = Config::load().map_err(|e| {
            eprintln!("Failed to load configuration: {}", e);
            e
        })?;

        // Initialize logging
        crate::logging::init_logging(&config.logging)?;

        let _context = LogContext::new("driver")
            .with_device_instance(config.device_instance);

        let logger = get_logger("driver");

        let (shutdown_tx, shutdown_rx) = mpsc::unbounded_channel();
        let (state_tx, _) = watch::channel(DriverState::Initializing);

        logger.info("Initializing Alfen driver");

        Ok(Self {
            config,
            state: state_tx,
            modbus_manager: None,
            logger,
            shutdown_tx,
            shutdown_rx,
        })
    }

    /// Run the driver main loop
    pub async fn run(&mut self) -> Result<()> {
        self.logger.info("Starting Alfen driver main loop");

        // Initialize Modbus connection
        self.initialize_modbus().await?;

        // Update state to running
        self.state.send(DriverState::Running).ok();

        // Main polling loop
        let mut poll_interval = interval(Duration::from_millis(self.config.poll_interval_ms));

        loop {
            tokio::select! {
                _ = poll_interval.tick() => {
                    if let Err(e) = self.poll_cycle().await {
                        self.logger.error(&format!("Poll cycle failed: {}", e));
                        // Continue polling even on errors
                    }
                }
                _ = self.shutdown_rx.recv() => {
                    self.logger.info("Shutdown signal received");
                    break;
                }
            }
        }

        // Shutdown sequence
        self.state.send(DriverState::ShuttingDown).ok();
        self.shutdown().await?;

        Ok(())
    }

    /// Initialize Modbus connection
    async fn initialize_modbus(&mut self) -> Result<()> {
        let manager = ModbusConnectionManager::new(
            &self.config.modbus,
            self.config.controls.max_retries,
            Duration::from_secs_f64(self.config.controls.retry_delay),
        );

        self.modbus_manager = Some(manager);
        self.logger.info("Modbus connection manager initialized");
        Ok(())
    }

    /// Single polling cycle
    async fn poll_cycle(&mut self) -> Result<()> {
        self.logger.debug("Starting poll cycle");

        // TODO: Implement actual polling logic
        // 1. Read charger status
        // 2. Read power/energy data
        // 3. Apply control logic
        // 4. Update D-Bus paths
        // 5. Update web status

        self.logger.debug("Poll cycle completed");
        Ok(())
    }

    /// Shutdown the driver
    async fn shutdown(&mut self) -> Result<()> {
        self.logger.info("Shutting down driver");

        if let Some(_manager) = self.modbus_manager.take() {
            // Disconnect Modbus
            // TODO: Implement proper disconnect
        }

        self.logger.info("Driver shutdown complete");
        Ok(())
    }

    /// Get current driver state
    pub fn get_state(&self) -> DriverState {
        self.state.borrow().clone()
    }

    /// Request shutdown
    pub fn request_shutdown(&self) {
        self.shutdown_tx.send(()).ok();
    }

    /// Get configuration reference
    pub fn config(&self) -> &Config {
        &self.config
    }
}
