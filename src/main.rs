use anyhow::Result;
use phaeton::driver::{AlfenDriver, DriverCommand};
use phaeton::web_axum;
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

    info!("Phaeton Alfen EV Charger Driver starting up");

    // Share driver with web server
    let driver_arc = Arc::new(Mutex::new(driver));

    // Spawn Axum server (API + OpenAPI)
    let axum_driver = driver_arc.clone();
    let axum_task = tokio::spawn(async move {
        let (host, port) = {
            let drv = axum_driver.lock().await;
            (drv.config().web.host.clone(), drv.config().web.port)
        };
        if let Err(e) = web_axum::serve(axum_driver.clone(), &host, port).await {
            error!("Axum server error: {}", e);
        }
    });

    // Run the driver in the current task
    match Arc::clone(&driver_arc).lock().await.run().await {
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
