use super::AlfenDriver;
use crate::error::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{Duration, Instant, interval};

#[cfg(feature = "updater")]
fn spawn_updater_task(driver: Arc<Mutex<AlfenDriver>>) {
    tokio::spawn(async move {
        let logger = crate::logging::get_logger("updater");
        loop {
            // Read current config snapshot without holding the lock across I/O
            let (enabled, auto_check, auto_update, include_prereleases, interval_secs, repo) = {
                let d = driver.lock().await;
                let cfg = d.config();
                let hours = cfg.updates.check_interval_hours.max(1) as u64;
                let repo = if cfg.updates.repository.trim().is_empty() {
                    env!("CARGO_PKG_REPOSITORY").to_string()
                } else {
                    cfg.updates.repository.clone()
                };
                (
                    cfg.updates.enabled,
                    cfg.updates.auto_check,
                    cfg.updates.auto_update,
                    cfg.updates.include_prereleases,
                    hours * 3600,
                    repo,
                )
            };

            if enabled && auto_check {
                let mut updater = crate::updater::GitUpdater::new(repo.clone(), "main".to_string());
                match updater
                    .check_for_updates_with_prereleases(include_prereleases)
                    .await
                {
                    Ok(st) => {
                        let mut msg = format!(
                            "Auto update check: current={}, latest={:?}, available={}",
                            st.current_version, st.latest_version, st.update_available
                        );
                        if auto_update && st.update_available {
                            msg.push_str("; applying update");
                            logger.info(&msg);
                            let mut upd2 =
                                crate::updater::GitUpdater::new(repo.clone(), "main".to_string());
                            if let Err(e) = upd2
                                .apply_updates_with_prereleases(include_prereleases)
                                .await
                            {
                                logger.error(&format!("Auto update apply failed: {}", e));
                            }
                        } else {
                            logger.info(&msg);
                        }
                    }
                    Err(e) => {
                        logger.warn(&format!("Auto update check failed: {}", e));
                    }
                }
            }

            tokio::time::sleep(Duration::from_secs(interval_secs)).await;
        }
    });
}

#[cfg(not(feature = "updater"))]
fn spawn_updater_task(_driver: Arc<Mutex<AlfenDriver>>) {}

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

    // Spawn background updater task (respects config flags)
    spawn_updater_task(driver.clone());

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::DriverCommand;
    use tokio::sync::mpsc;

    async fn make_driver_arc() -> Arc<Mutex<AlfenDriver>> {
        let (tx, rx) = mpsc::unbounded_channel();
        let driver = AlfenDriver::new(rx, tx).await.unwrap();
        Arc::new(Mutex::new(driver))
    }

    #[tokio::test]
    async fn init_modbus_sets_defaults_and_state_running() {
        let driver = make_driver_arc().await;
        init_modbus_and_state(&driver).await.unwrap();
        let d = driver.lock().await;
        assert_eq!(d.get_state(), super::super::types::DriverState::Running);
        assert!(
            (d.intended_set_current - d.config().defaults.intended_set_current).abs()
                < f32::EPSILON
        );
        assert!(
            (d.station_max_current - d.config().defaults.station_max_current).abs() < f32::EPSILON
        );
    }

    #[tokio::test]
    async fn get_poll_interval_reflects_config() {
        let driver = make_driver_arc().await;
        {
            let mut d = driver.lock().await;
            let mut cfg = d.config().clone();
            cfg.poll_interval_ms = 123;
            d.update_config(cfg).unwrap();
        }
        let ms = get_poll_interval_ms(&driver).await;
        assert_eq!(ms, 123);
    }

    #[tokio::test]
    async fn handle_commands_dispatches_and_shutdown_true() {
        let driver = make_driver_arc().await;
        // Dispatch a command
        {
            let d = driver.lock().await;
            let _ = d.commands_tx.send(DriverCommand::SetMode(1));
        }
        handle_commands_and_maybe_shutdown(&driver).await.unwrap();
        {
            let d = driver.lock().await;
            assert_eq!(d.current_mode_code(), 1);
        }

        // Now request shutdown and expect true
        {
            let d = driver.lock().await;
            d.request_shutdown();
        }
        let should_break = handle_commands_and_maybe_shutdown(&driver).await.unwrap();
        assert!(should_break);
    }

    #[tokio::test]
    async fn run_poll_cycle_updates_metrics_without_modbus() {
        let driver = make_driver_arc().await;
        // No Modbus/D-Bus attached; should still run and increment counters
        let before = { driver.lock().await.total_polls };
        run_poll_cycle_and_update_metrics(&driver).await;
        let after = { driver.lock().await.total_polls };
        assert_eq!(after, before + 1);
    }

    #[tokio::test]
    async fn init_dbus_non_required_allows_continue() {
        let driver = make_driver_arc().await;
        {
            let mut d = driver.lock().await;
            let mut cfg = d.config().clone();
            cfg.require_dbus = false;
            d.update_config(cfg).unwrap();
        }
        // Should not error even if cannot connect to a real D-Bus
        init_dbus_if_configured(&driver).await.unwrap();
    }
}
