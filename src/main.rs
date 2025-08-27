use anyhow::Result;
use phaeton::driver::{AlfenDriver, DriverCommand};
use phaeton::web;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use tracing::warn;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<()> {
    // Parse CLI arguments
    let mut args = std::env::args().skip(1);
    let mut config_path_override: Option<PathBuf> = None;
    while let Some(arg) = args.next() {
        if arg == "--help" || arg == "-h" {
            println!(
                "Usage: phaeton [--config <path>]\n\n  --config, -c <path>  Path to YAML config file (no fallback)\n  --help, -h           Show this help"
            );
            return Ok(());
        } else if arg == "--config" || arg == "-c" {
            if let Some(val) = args.next() {
                config_path_override = Some(PathBuf::from(val));
            } else {
                eprintln!("Error: --config requires a file path\nTry --help for usage.");
                std::process::exit(2);
            }
        } else if let Some(v) = arg.strip_prefix("--config=") {
            config_path_override = Some(PathBuf::from(v));
        } else {
            warn!("Unknown argument ignored: {}", arg);
        }
    }

    // Create driver command channel
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<DriverCommand>();

    // Initialize the driver with optional config override
    let driver =
        AlfenDriver::new_with_config_override(cmd_rx, cmd_tx.clone(), config_path_override)
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
