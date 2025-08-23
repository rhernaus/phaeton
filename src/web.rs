//! HTTP server and REST API for Phaeton
//!
//! Provides endpoints for status and control operations.

use crate::driver::AlfenDriver;
use crate::error::Result;
use crate::logging::get_logger;
use serde::Deserialize;
use serde_json::json;
use std::net::IpAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_stream::{StreamExt, wrappers::BroadcastStream};
use warp::http::Method;
use warp::sse::Event;
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

#[derive(Debug, Deserialize)]
struct UpdateConfigBody {
    config: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct TailParams {
    lines: Option<usize>,
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

        // Config schema endpoint
        let get_schema = warp::path!("api" / "config" / "schema")
            .and(warp::get())
            .and_then(handle_get_config_schema);

        let get_config = warp::path!("api" / "config")
            .and(warp::get())
            .and(with_driver(driver.clone()))
            .and_then(handle_get_config);

        let put_config = warp::path!("api" / "config")
            .and(warp::put())
            .and(with_driver(driver.clone()))
            .and(warp::body::json())
            .and_then(handle_put_config);

        // Logs tail endpoint
        let logs_tail = warp::path!("api" / "logs" / "tail")
            .and(warp::get())
            .and(with_driver(driver.clone()))
            .and(warp::query::<TailParams>())
            .and_then(handle_logs_tail);

        // Logs head endpoint
        let logs_head = warp::path!("api" / "logs" / "head")
            .and(warp::get())
            .and(with_driver(driver.clone()))
            .and(warp::query::<TailParams>())
            .and_then(handle_logs_head);

        // Logs download endpoint
        let logs_download = warp::path!("api" / "logs" / "download")
            .and(warp::get())
            .and(with_driver(driver.clone()))
            .and_then(handle_logs_download);

        // Static UI under /ui
        let ui_index = warp::path("ui")
            .and(warp::path::end())
            .and(warp::fs::file("./webui/index.html"));
        let ui_files = warp::path("ui").and(warp::fs::dir("./webui"));

        // SSE events: live status stream
        let events = warp::path!("api" / "events")
            .and(warp::get())
            .and(with_driver(driver.clone()))
            .and_then(handle_events);

        // Sessions endpoints
        let sessions_state = warp::path!("api" / "sessions")
            .and(warp::get())
            .and(with_driver(driver.clone()))
            .and_then(handle_sessions_state);

        // D-Bus cached paths
        let dbus_dump = warp::path!("api" / "dbus")
            .and(warp::get())
            .and(with_driver(driver.clone()))
            .and_then(handle_dbus_dump);

        // Update endpoints (status/check/apply)
        let update_status = warp::path!("api" / "update" / "status")
            .and(warp::get())
            .and_then(handle_update_status);
        let update_check = warp::path!("api" / "update" / "check")
            .and(warp::post())
            .and_then(handle_update_check);
        let update_apply = warp::path!("api" / "update" / "apply")
            .and(warp::post())
            .and_then(handle_update_apply);

        status
            .or(post_mode)
            .or(post_startstop)
            .or(post_set_current)
            .or(get_schema)
            .or(get_config)
            .or(put_config)
            .or(logs_tail)
            .or(logs_head)
            .or(logs_download)
            .or(ui_index)
            .or(ui_files)
            .or(events)
            .or(sessions_state)
            .or(dbus_dump)
            .or(update_status)
            .or(update_check)
            .or(update_apply)
            .or(index)
    }

    /// Start the web server
    pub async fn start(self, host: &str, port: u16) -> Result<()> {
        let routes = Self::create_routes_with_driver(self.driver.clone());
        let addr = format!("{}:{}", host, port);

        self.logger
            .info(&format!("Starting web server on {}", addr));

        // Enable permissive CORS for local development
        let cors = warp::cors()
            .allow_any_origin()
            .allow_methods(vec![
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::OPTIONS,
            ])
            .allow_headers(vec!["content-type"]);

        let routes = routes.with(cors);

        let ip: IpAddr = host
            .parse()
            .unwrap_or_else(|_| IpAddr::from([127, 0, 0, 1]));

        warp::serve(routes).run((ip, port)).await;

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
    // Use driver's internal API directly; if desired, this could send via channel instead
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

async fn handle_get_config(
    driver: Arc<Mutex<AlfenDriver>>,
) -> std::result::Result<impl Reply, Rejection> {
    let drv = driver.lock().await;
    let cfg = drv.config().clone();
    let json = serde_json::to_value(cfg).unwrap_or(serde_json::json!({"error": "serialization"}));
    Ok(warp::reply::json(&json))
}

async fn handle_get_config_schema() -> std::result::Result<impl Reply, Rejection> {
    let schema = schemars::schema_for!(crate::config::Config);
    let json = serde_json::to_value(&schema).unwrap_or(serde_json::json!({"error":"schema"}));
    Ok(warp::reply::json(&json))
}

async fn handle_put_config(
    driver: Arc<Mutex<AlfenDriver>>,
    body: UpdateConfigBody,
) -> std::result::Result<impl Reply, Rejection> {
    let mut drv = driver.lock().await;
    let new_cfg: crate::config::Config = match serde_json::from_value(body.config.clone()) {
        Ok(c) => c,
        Err(_) => {
            return Ok(warp::reply::with_status(
                "Bad Request",
                warp::http::StatusCode::BAD_REQUEST,
            ));
        }
    };
    if new_cfg.validate().is_err() {
        return Ok(warp::reply::with_status(
            "Invalid config",
            warp::http::StatusCode::BAD_REQUEST,
        ));
    }
    // Apply update
    if drv.update_config(new_cfg).is_err() {
        return Ok(warp::reply::with_status(
            "Failed to apply config",
            warp::http::StatusCode::INTERNAL_SERVER_ERROR,
        ));
    }
    Ok(warp::reply::with_status("OK", warp::http::StatusCode::OK))
}

async fn handle_logs_tail(
    driver: Arc<Mutex<AlfenDriver>>,
    params: TailParams,
) -> std::result::Result<impl Reply, Rejection> {
    let (log_path, max_lines) = {
        let drv = driver.lock().await;
        (
            drv.config().logging.file.clone(),
            params.lines.unwrap_or(200).min(10_000),
        )
    };

    let resp = match tokio::fs::read_to_string(&log_path).await {
        Ok(contents) => {
            let mut lines: Vec<&str> = contents.lines().collect();
            if lines.len() > max_lines {
                lines = lines.split_off(lines.len() - max_lines);
            }
            let body = lines.join("\n");
            let reply = warp::reply::with_header(body, "Content-Type", "text/plain; charset=utf-8");
            reply.into_response()
        }
        Err(_) => {
            let reply = warp::reply::with_status(
                "Log file not available",
                warp::http::StatusCode::NOT_FOUND,
            );
            let reply =
                warp::reply::with_header(reply, "Content-Type", "text/plain; charset=utf-8");
            reply.into_response()
        }
    };
    Ok(resp)
}

async fn handle_logs_head(
    driver: Arc<Mutex<AlfenDriver>>,
    params: TailParams,
) -> std::result::Result<impl Reply, Rejection> {
    let (log_path, max_lines) = {
        let drv = driver.lock().await;
        (
            drv.config().logging.file.clone(),
            params.lines.unwrap_or(200).min(10_000),
        )
    };
    let resp = match tokio::fs::read_to_string(&log_path).await {
        Ok(contents) => {
            let mut lines: Vec<&str> = contents.lines().collect();
            if lines.len() > max_lines {
                lines.truncate(max_lines);
            }
            let body = lines.join("\n");
            let reply = warp::reply::with_header(body, "Content-Type", "text/plain; charset=utf-8");
            reply.into_response()
        }
        Err(_) => {
            warp::reply::with_status("Log file not available", warp::http::StatusCode::NOT_FOUND)
                .into_response()
        }
    };
    Ok(resp)
}

async fn handle_logs_download(
    driver: Arc<Mutex<AlfenDriver>>,
) -> std::result::Result<impl Reply, Rejection> {
    let log_path = {
        let drv = driver.lock().await;
        drv.config().logging.file.clone()
    };
    let resp = match tokio::fs::read(&log_path).await {
        Ok(bytes) => {
            let reply = warp::reply::with_header(bytes, "Content-Type", "application/octet-stream");
            reply.into_response()
        }
        Err(_) => {
            warp::reply::with_status("Log file not available", warp::http::StatusCode::NOT_FOUND)
                .into_response()
        }
    };
    Ok(resp)
}

async fn handle_events(
    driver: Arc<Mutex<AlfenDriver>>,
) -> std::result::Result<impl Reply, Rejection> {
    // Subscribe to driver's broadcast channel
    let rx = {
        let drv = driver.lock().await;
        drv.subscribe_status()
    };

    let stream = BroadcastStream::new(rx).filter_map(|msg| match msg {
        Ok(payload) => {
            let ev: Event = Event::default().event("status").data(payload);
            Some(Ok::<Event, std::convert::Infallible>(ev))
        }
        Err(_) => None,
    });

    Ok(warp::sse::reply(warp::sse::keep_alive().stream(stream)))
}

async fn handle_sessions_state(
    driver: Arc<Mutex<AlfenDriver>>,
) -> std::result::Result<impl Reply, Rejection> {
    let drv = driver.lock().await;
    let json = drv.sessions_snapshot();
    Ok(warp::reply::json(&json))
}

async fn handle_dbus_dump(
    driver: Arc<Mutex<AlfenDriver>>,
) -> std::result::Result<impl Reply, Rejection> {
    let drv = driver.lock().await;
    let json = drv.get_dbus_cache_snapshot();
    Ok(warp::reply::json(&json))
}

async fn handle_update_status() -> std::result::Result<impl Reply, Rejection> {
    let updater = crate::updater::GitUpdater::new(
        "https://github.com/your-org/phaeton".to_string(),
        "main".to_string(),
    );
    let status = updater.get_status();
    let json = serde_json::to_value(status).unwrap_or(serde_json::json!({"error":"status"}));
    Ok(warp::reply::json(&json))
}

async fn handle_update_check() -> std::result::Result<impl Reply, Rejection> {
    let mut updater = crate::updater::GitUpdater::new(
        "https://github.com/your-org/phaeton".to_string(),
        "main".to_string(),
    );
    let res = updater.check_for_updates().await;
    match res {
        Ok(status) => {
            let json =
                serde_json::to_value(status).unwrap_or(serde_json::json!({"error":"status"}));
            Ok(warp::reply::with_status(
                warp::reply::json(&json),
                warp::http::StatusCode::OK,
            ))
        }
        Err(e) => {
            let json = serde_json::json!({"error": e.to_string()});
            Ok(warp::reply::with_status(
                warp::reply::json(&json),
                warp::http::StatusCode::INTERNAL_SERVER_ERROR,
            ))
        }
    }
}

async fn handle_update_apply() -> std::result::Result<impl Reply, Rejection> {
    let mut updater = crate::updater::GitUpdater::new(
        "https://github.com/your-org/phaeton".to_string(),
        "main".to_string(),
    );
    match updater.apply_updates().await {
        Ok(_) => {
            let json = serde_json::json!({"status":"ok"});
            Ok(warp::reply::with_status(
                warp::reply::json(&json),
                warp::http::StatusCode::OK,
            ))
        }
        Err(e) => {
            let json = serde_json::json!({"error": e.to_string()});
            Ok(warp::reply::with_status(
                warp::reply::json(&json),
                warp::http::StatusCode::INTERNAL_SERVER_ERROR,
            ))
        }
    }
}
