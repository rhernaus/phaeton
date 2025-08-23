//! HTTP server and REST API for Phaeton
//!
//! This module provides the web interface including REST API endpoints
//! and static file serving for the web UI.

use warp::{Filter, Reply, Rejection, reject};
use serde_json::json;
use crate::error::{Result, PhaetonError};
use crate::logging::get_logger;

/// Web server for Phaeton
pub struct WebServer {
    logger: crate::logging::StructuredLogger,
}

impl WebServer {
    /// Create a new web server
    pub async fn new() -> Result<Self> {
        let logger = get_logger("web");

        Ok(Self { logger })
    }

    /// Create the routes
    pub fn create_routes(&self) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
        let status = warp::path!("api" / "status")
            .and(warp::get())
            .and_then(Self::handle_status);

        let get_config = warp::path!("api" / "config")
            .and(warp::get())
            .and_then(Self::handle_get_config);

        let index = warp::path::end()
            .map(|| "Phaeton Alfen EV Charger Driver");

        status.or(get_config).or(index)
    }

    /// Start the web server
    pub async fn start(self, host: &str, port: u16) -> Result<()> {
        let routes = self.create_routes();
        let addr = format!("{}:{}", host, port);

        self.logger.info(&format!("Starting web server on {}", addr));

        warp::serve(routes)
            .run(([127, 0, 0, 1], port))
            .await;

        Ok(())
    }

    /// Handle status endpoint
    async fn handle_status() -> Result<impl Reply, Rejection> {
        // TODO: Get actual status from driver
        let status = json!({
            "mode": 0,
            "start_stop": 0,
            "set_current": 6.0,
            "station_max_current": 32.0,
            "status": 0,
            "ac_current": 0.0,
            "ac_power": 0.0,
            "energy_forward_kwh": 0.0,
            "l1_voltage": 230.0,
            "l2_voltage": 230.0,
            "l3_voltage": 230.0,
            "l1_current": 0.0,
            "l2_current": 0.0,
            "l3_current": 0.0,
            "active_phases": 3,
            "charging_time_sec": 0,
            "firmware": "Unknown",
            "serial": "Unknown",
            "product_name": "Alfen EV Charger",
            "device_instance": 0
        });

        Ok(warp::reply::json(&status))
    }

    /// Handle get config endpoint
    async fn handle_get_config() -> Result<impl Reply, Rejection> {
        // TODO: Get actual config from driver
        let config = json!({
            "modbus": {
                "ip": "192.168.1.100",
                "port": 502,
                "socket_slave_id": 1,
                "station_slave_id": 200
            },
            "device_instance": 0,
            "poll_interval_ms": 1000
        });

        Ok(warp::reply::json(&config))
    }
}
