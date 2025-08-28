use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, watch};
use tokio::time::{Duration, interval};

use crate::error::Result;

use super::types::DriverSnapshot;

impl super::AlfenDriver {
    /// Create a new driver instance using configuration loaded from defaults.
    pub async fn new(
        commands_rx: mpsc::UnboundedReceiver<super::types::DriverCommand>,
        commands_tx: mpsc::UnboundedSender<super::types::DriverCommand>,
    ) -> Result<Self> {
        let config = crate::config::Config::load().map_err(|e| {
            eprintln!("Failed to load configuration: {}", e);
            e
        })?;
        Self::new_with_config(commands_rx, commands_tx, config).await
    }

    /// Create a new driver instance using an optional override config path.
    /// When `config_path_override` is `Some`, the file must exist and be valid,
    /// otherwise an error is returned without falling back to defaults.
    pub async fn new_with_config_override(
        commands_rx: mpsc::UnboundedReceiver<super::types::DriverCommand>,
        commands_tx: mpsc::UnboundedSender<super::types::DriverCommand>,
        config_path_override: Option<PathBuf>,
    ) -> Result<Self> {
        let config = crate::config::Config::load_with_override(config_path_override.as_deref())
            .map_err(|e| {
                eprintln!("Failed to load configuration: {}", e);
                e
            })?;
        Self::new_with_config(commands_rx, commands_tx, config).await
    }

    /// Internal constructor that builds the driver from a provided Config.
    async fn new_with_config(
        commands_rx: mpsc::UnboundedReceiver<super::types::DriverCommand>,
        commands_tx: mpsc::UnboundedSender<super::types::DriverCommand>,
        config: crate::config::Config,
    ) -> Result<Self> {
        // Initialize logging
        crate::logging::init_logging(&config.logging)?;

        let _context =
            crate::logging::LogContext::new("driver").with_device_instance(config.device_instance);

        let logger = crate::logging::get_logger("driver");

        let (shutdown_tx, shutdown_rx) = mpsc::unbounded_channel();
        let (state_tx, state_rx) = watch::channel(super::types::DriverState::Initializing);

        logger.info("Initializing EV charger driver");

        // Initialize persistence and load any saved state (best-effort)
        let mut persistence =
            crate::persistence::PersistenceManager::new("/data/phaeton_state.json");
        let _ = persistence.load();

        // Initialize session manager and restore previous session state if available
        let mut sessions = crate::session::ChargingSessionManager::default();
        if let Some(sess_state) = persistence.get_section("session") {
            let _ = sessions.restore_state(sess_state);
        }

        // Restore control states from persistence
        let mut current_mode = crate::controls::ChargingMode::Manual;
        if let Some(mode_val) = persistence.get::<u32>("mode") {
            current_mode = match mode_val {
                1 => crate::controls::ChargingMode::Auto,
                2 => crate::controls::ChargingMode::Scheduled,
                _ => crate::controls::ChargingMode::Manual,
            };
        }

        let mut start_stop = crate::controls::StartStopState::Stopped;
        if let Some(ss) = persistence.get::<u32>("start_stop") {
            start_stop = if ss == 1 {
                crate::controls::StartStopState::Enabled
            } else {
                crate::controls::StartStopState::Stopped
            };
        }

        let mut intended_set_current = 0.0f32;
        if let Some(cur) = persistence.get::<f32>("set_current") {
            intended_set_current = cur.max(0.0).min(config.controls.max_set_current);
        }

        // Create status broadcast channel
        let (status_tx, _status_rx) = broadcast::channel::<String>(100);

        // Create status snapshot channel (initialized with empty object)
        let initial_snapshot = Arc::new(DriverSnapshot {
            timestamp: chrono::Utc::now().to_rfc3339(),
            mode: 0,
            start_stop: 0,
            set_current: 0.0,
            applied_current: 0.0,
            station_max_current: 0.0,
            device_instance: config.device_instance,
            product_name: None,
            firmware: None,
            serial: None,
            status: 0,
            active_phases: 0,
            ac_power: 0.0,
            ac_current: 0.0,
            l1_voltage: 0.0,
            l2_voltage: 0.0,
            l3_voltage: 0.0,
            l1_current: 0.0,
            l2_current: 0.0,
            l3_current: 0.0,
            l1_power: 0.0,
            l2_power: 0.0,
            l3_power: 0.0,
            total_energy_kwh: 0.0,
            pricing_currency: None,
            energy_rate: None,
            session: serde_json::json!({}),
            poll_duration_ms: None,
            total_polls: 0,
            overrun_count: 0,
            poll_interval_ms: config.poll_interval_ms,
            excess_pv_power_w: 0.0,
            modbus_connected: None,
            driver_state: "Initializing".to_string(),
        });
        let (status_snapshot_tx, status_snapshot_rx) =
            watch::channel::<Arc<DriverSnapshot>>(initial_snapshot);

        Ok(Self {
            config,
            state: state_tx,
            state_rx,
            modbus_manager: None,
            logger,
            shutdown_tx,
            shutdown_rx,
            persistence,
            sessions,
            dbus: None,
            controls: crate::controls::ChargingControls::new(),
            current_mode,
            start_stop,
            intended_set_current,
            station_max_current: 32.0,
            last_sent_current: 0.0,
            last_current_set_time: std::time::Instant::now(),
            last_set_current_monotonic: std::time::Instant::now(),
            last_status: 0,

            min_charge_timer_deadline: None,
            auto_mode_entered_at: None,
            commands_rx,
            commands_tx,
            status_tx,
            status_snapshot_tx,
            status_snapshot_rx,
            last_l1_voltage: 0.0,
            last_l2_voltage: 0.0,
            last_l3_voltage: 0.0,
            last_l1_current: 0.0,
            last_l2_current: 0.0,
            last_l3_current: 0.0,
            last_l1_power: 0.0,
            last_l2_power: 0.0,
            last_l3_power: 0.0,
            last_total_power: 0.0,
            last_energy_kwh: 0.0,
            product_name: None,
            firmware_version: None,
            serial: None,
            total_polls: 0,
            overrun_count: 0,
            last_excess_pv_power_w: 0.0,
        })
    }

    /// Run the driver main loop
    pub async fn run(&mut self) -> Result<()> {
        self.logger.info("Starting EV charger driver main loop");

        // Initialize Modbus connection
        self.initialize_modbus().await?;

        // Update state to running
        self.state.send(super::types::DriverState::Running).ok();

        // Initialize control state from config defaults
        self.intended_set_current = self.config.defaults.intended_set_current;
        self.station_max_current = self.config.defaults.station_max_current;
        // Attempt to start D-Bus service only after we have identity values
        match self.try_start_dbus_with_identity().await {
            Ok(_) => {}
            Err(e) => {
                if self.config.require_dbus {
                    self.logger.error(&format!(
                        "Failed to initialize D-Bus and require_dbus=true: {}",
                        e
                    ));
                    return Err(e);
                } else {
                    self.logger.warn(&format!(
                        "D-Bus initialization failed but require_dbus=false, continuing without D-Bus: {}",
                        e
                    ));
                }
            }
        }

        // Main polling loop
        let mut poll_interval = interval(Duration::from_millis(self.config.poll_interval_ms));

        loop {
            tokio::select! {
                _ = poll_interval.tick() => {
                    let poll_started = std::time::Instant::now();
                    if let Err(e) = self.poll_cycle().await {
                        self.logger.error(&format!("Poll cycle failed: {}", e));
                        // Continue polling even on errors
                    }
                    let dur_ms = poll_started.elapsed().as_millis() as u64;
                    self.total_polls = self.total_polls.saturating_add(1);
                    if dur_ms > self.config.poll_interval_ms {
                        self.overrun_count = self.overrun_count.saturating_add(1);
                    }
                }
                Some(cmd) = self.commands_rx.recv() => {
                    self.handle_command(cmd).await;
                }
                _ = self.shutdown_rx.recv() => {
                    self.logger.info("Shutdown signal received");
                    break;
                }
            }
        }

        // Shutdown sequence
        self.state
            .send(super::types::DriverState::ShuttingDown)
            .ok();
        self.shutdown().await?;

        Ok(())
    }

    /// Initialize Modbus connection
    pub(crate) async fn initialize_modbus(&mut self) -> Result<()> {
        let manager = crate::modbus::ModbusConnectionManager::new(
            &self.config.modbus,
            self.config.controls.max_retries,
            Duration::from_secs_f64(self.config.controls.retry_delay),
        );

        self.modbus_manager = Some(Box::new(manager));
        self.logger.info("Modbus connection manager initialized");
        Ok(())
    }

    // /// Single polling cycle
    // poll_cycle moved to runtime_poll.rs
    /// Shutdown the driver
    pub(crate) async fn shutdown(&mut self) -> Result<()> {
        self.logger.info("Shutting down driver");

        if let Some(_manager) = self.modbus_manager.take() {
            // Disconnect Modbus
            // TODO: Implement proper disconnect
        }

        self.logger.info("Driver shutdown complete");
        Ok(())
    }
}

impl super::AlfenDriver {
    pub(crate) fn last_poll_duration_ms(&self) -> u64 {
        // Placeholder: could store last duration explicitly; use 0 to indicate unknown
        0
    }
}
