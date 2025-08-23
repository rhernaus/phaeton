use anyhow::Result;
use phaeton::driver::{AlfenDriver, DriverCommand};
use phaeton::web::WebServer;
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

    // Spawn web server
    let web_driver = driver_arc.clone();
    let web_task = tokio::spawn(async move {
        let web = WebServer::new(web_driver).await.expect("web init");
        // Defaults; in the future use config
        if let Err(e) = web.start("127.0.0.1", 8088).await {
            error!("Web server error: {}", e);
        }
    });

    // Run the driver in the current task
    match Arc::clone(&driver_arc).lock().await.run().await {
        Ok(_) => {
            info!("Driver shutdown complete");
            // Ensure web server task ends (it runs until process stops)
            web_task.abort();
            Ok(())
        }
        Err(e) => {
            error!("Driver failed with error: {}", e);
            web_task.abort();
            Err(anyhow::anyhow!("Driver error: {}", e))
        }
    }
}
