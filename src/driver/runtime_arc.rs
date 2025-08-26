use super::AlfenDriver;
use crate::error::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{Duration, Instant, interval};

async fn init_modbus_and_state(driver: &Arc<Mutex<AlfenDriver>>) -> Result<()> {
    let mut d = driver.lock().await;
    d.logger.info("Starting EV charger driver main loop");
    d.initialize_modbus().await?;
    d.state.send(super::DriverState::Running).ok();
    d.intended_set_current = d.config.defaults.intended_set_current;
    d.station_max_current = d.config.defaults.station_max_current;
    Ok(())
}

async fn init_dbus_if_configured(driver: &Arc<Mutex<AlfenDriver>>) -> Result<()> {
    let mut d = driver.lock().await;
    if let Err(e) = d.try_start_dbus_with_identity().await {
        if d.config.require_dbus {
            d.logger.error(&format!(
                "Failed to initialize D-Bus and require_dbus=true: {}",
                e
            ));
            return Err(e);
        } else {
            d.logger.warn(&format!(
                "D-Bus initialization failed but require_dbus=false, continuing without D-Bus: {}",
                e
            ));
        }
    }
    Ok(())
}

async fn get_poll_interval_ms(driver: &Arc<Mutex<AlfenDriver>>) -> u64 {
    let d = driver.lock().await;
    d.config.poll_interval_ms
}

async fn handle_commands_and_maybe_shutdown(driver: &Arc<Mutex<AlfenDriver>>) -> Result<bool> {
    let mut d = driver.lock().await;
    while let Ok(cmd) = d.commands_rx.try_recv() {
        d.handle_command(cmd).await;
    }
    if d.shutdown_rx.try_recv().is_ok() {
        d.logger.info("Shutdown signal received");
        d.state.send(super::DriverState::ShuttingDown).ok();
        d.shutdown().await?;
        return Ok(true);
    }
    Ok(false)
}

async fn run_poll_cycle_and_update_metrics(driver: &Arc<Mutex<AlfenDriver>>) {
    let poll_started = Instant::now();
    let mut d = driver.lock().await;
    if let Err(e) = d.poll_cycle().await {
        d.logger.error(&format!("Poll cycle failed: {}", e));
    }
    let dur_ms = poll_started.elapsed().as_millis() as u64;
    d.total_polls = d.total_polls.saturating_add(1);
    if dur_ms > d.config.poll_interval_ms {
        d.overrun_count = d.overrun_count.saturating_add(1);
    }
    // After updating measurements and snapshot in poll_cycle, mirror key values to D-Bus
    if let Some(dbus) = d.dbus.as_ref() {
        let snapshot = d.build_typed_snapshot(Some(dur_ms));
        let _ = dbus.lock().await.export_typed_snapshot(&snapshot).await;
    }
}

/// Run the driver using an Arc<Mutex<AlfenDriver>> without holding the lock across awaits.
/// This ensures other components (web, D-Bus helpers) can briefly lock the driver.
pub(crate) async fn run_on_arc_impl(driver: Arc<Mutex<AlfenDriver>>) -> Result<()> {
    // Initialization phase
    init_modbus_and_state(&driver).await?;
    init_dbus_if_configured(&driver).await?;

    let poll_interval_ms = get_poll_interval_ms(&driver).await;
    let mut ticker = interval(Duration::from_millis(poll_interval_ms));

    // Main loop
    loop {
        ticker.tick().await;

        // Handle commands and shutdown quickly without blocking other tasks
        if handle_commands_and_maybe_shutdown(&driver).await? {
            return Ok(());
        }

        // Execute one poll cycle
        run_poll_cycle_and_update_metrics(&driver).await;
    }
}
