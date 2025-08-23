//! HTTP server and REST API for Phaeton
//!
//! Provides endpoints for status and control operations.

use crate::driver::AlfenDriver;
use crate::error::Result;
use crate::logging::get_logger;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;
use warp::{Filter, Rejection, Reply};

/// Web server for Phaeton
pub struct WebServer {
    logger: crate::logging::StructuredLogger,
    driver: Arc<Mutex<AlfenDriver>>,
}

#[derive(Debug, Deserialize)]
struct ModeBody {
    mode: u8,
}

#[derive(Debug, Deserialize)]
struct StartStopBody {
    value: u8,
}

#[derive(Debug, Deserialize)]
struct SetCurrentBody {
    amps: f32,
}

impl WebServer {
    /// Create a new web server
    pub async fn new(driver: Arc<Mutex<AlfenDriver>>) -> Result<Self> {
        let logger = get_logger("web");
        Ok(Self { logger, driver })
    }

    /// Create the routes (static, do not capture &self lifetime)
    pub fn create_routes_with_driver(
        driver: Arc<Mutex<AlfenDriver>>,
    ) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
        let status = warp::path!("api" / "status")
            .and(warp::get())
            .and(with_driver(driver.clone()))
            .and_then(handle_status);

        let post_mode = warp::path!("api" / "mode")
            .and(warp::post())
            .and(with_driver(driver.clone()))
            .and(warp::body::json())
            .and_then(handle_mode);

        let post_startstop = warp::path!("api" / "startstop")
            .and(warp::post())
            .and(with_driver(driver.clone()))
            .and(warp::body::json())
            .and_then(handle_startstop);

        let post_set_current = warp::path!("api" / "set_current")
            .and(warp::post())
            .and(with_driver(driver.clone()))
            .and(warp::body::json())
            .and_then(handle_set_current);

        let index = warp::path::end().map(|| "Phaeton Alfen EV Charger Driver");

        status
            .or(post_mode)
            .or(post_startstop)
            .or(post_set_current)
            .or(index)
    }

    /// Start the web server
    pub async fn start(self, host: &str, port: u16) -> Result<()> {
        let routes = Self::create_routes_with_driver(self.driver.clone());
        let addr = format!("{}:{}", host, port);

        self.logger
            .info(&format!("Starting web server on {}", addr));

        warp::serve(routes).run(([127, 0, 0, 1], port)).await;

        Ok(())
    }
}

fn with_driver(
    driver: Arc<Mutex<AlfenDriver>>,
) -> impl Filter<Extract = (Arc<Mutex<AlfenDriver>>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || driver.clone())
}

async fn handle_status(
    driver: Arc<Mutex<AlfenDriver>>,
) -> std::result::Result<impl Reply, Rejection> {
    let drv = driver.lock().await;
    let mut status = json!({
        "mode": drv.current_mode_code(),
        "start_stop": drv.start_stop_code(),
        "set_current": drv.get_intended_set_current(),
        "station_max_current": drv.get_station_max_current(),
        "device_instance": drv.config().device_instance,
    });
    if let Some(v) = drv.get_db_value("/Ac/Power") {
        status["ac_power"] = v;
    }
    if let Some(v) = drv.get_db_value("/Ac/Energy/Forward") {
        status["energy_forward_kwh"] = v;
    }
    Ok(warp::reply::json(&status))
}

async fn handle_mode(
    driver: Arc<Mutex<AlfenDriver>>,
    body: ModeBody,
) -> std::result::Result<impl Reply, Rejection> {
    let mut drv = driver.lock().await;
    drv.set_mode(body.mode).await;
    Ok(warp::reply::with_status("OK", warp::http::StatusCode::OK))
}

async fn handle_startstop(
    driver: Arc<Mutex<AlfenDriver>>,
    body: StartStopBody,
) -> std::result::Result<impl Reply, Rejection> {
    let mut drv = driver.lock().await;
    drv.set_start_stop(body.value).await;
    Ok(warp::reply::with_status("OK", warp::http::StatusCode::OK))
}

async fn handle_set_current(
    driver: Arc<Mutex<AlfenDriver>>,
    body: SetCurrentBody,
) -> std::result::Result<impl Reply, Rejection> {
    let mut drv = driver.lock().await;
    drv.set_intended_current(body.amps).await;
    Ok(warp::reply::with_status("OK", warp::http::StatusCode::OK))
}
