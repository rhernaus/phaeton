use anyhow::Result;
use phaeton::driver::AlfenDriver;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize the driver
    let mut driver = AlfenDriver::new()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create driver: {}", e))?;

    info!("Phaeton Alfen EV Charger Driver starting up");

    // Start the driver
    match driver.run().await {
        Ok(_) => {
            info!("Driver shutdown complete");
            Ok(())
        }
        Err(e) => {
            error!("Driver failed with error: {}", e);
            Err(anyhow::anyhow!("Driver error: {}", e))
        }
    }
}
