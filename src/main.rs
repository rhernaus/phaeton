use anyhow::Result;
use phaeton::driver::{AlfenDriver, DriverCommand};
use phaeton::web;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<()> {
    // Create driver command channel
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<DriverCommand>();

    // Initialize the driver with command receiver
    let driver = AlfenDriver::new(cmd_rx, cmd_tx.clone())
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create driver: {}", e))?;

    info!("Phaeton EV Charger Driver starting up");

    // Capture web bind settings before placing driver behind a Mutex
    let (web_host, web_port) = (driver.config().web.host.clone(), driver.config().web.port);

    // Share driver with web server
    let driver_arc = Arc::new(Mutex::new(driver));

    // Spawn Axum server (API + OpenAPI)
    let axum_driver = driver_arc.clone();
    let axum_task = tokio::spawn(async move {
        // Log the selected host/port before starting web server
        {
            let logger = phaeton::logging::get_logger("web");
            let msg = format!(
                "Configured web bind address: host={}, port={}",
                web_host, web_port
            );
            logger.info(&msg);
        }
        if let Err(e) = web::serve(axum_driver.clone(), &web_host, web_port).await {
            error!("Axum server error: {}", e);
        }
    });

    // Run the driver loop without holding the mutex for the entire duration
    match phaeton::driver::AlfenDriver::run_on_arc(driver_arc.clone()).await {
        Ok(_) => {
            info!("Driver shutdown complete");
            // Ensure web server task ends (it runs until process stops)
            axum_task.abort();
            Ok(())
        }
        Err(e) => {
            error!("Driver failed with error: {}", e);
            axum_task.abort();
            Err(anyhow::anyhow!("Driver error: {}", e))
        }
    }
}
