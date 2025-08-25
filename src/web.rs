//! Axum-based HTTP server with OpenAPI (utoipa) and Swagger UI

use crate::driver::{AlfenDriver, DriverSnapshot};
use axum::response::Redirect;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::{
    Json, Router,
    extract::{Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, get_service, post},
};
use serde::Deserialize;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use tokio::sync::{Mutex, watch};
// no timeouts needed in current web handlers
use tokio_stream::StreamExt;
use tokio_stream::wrappers::WatchStream;
use tower_http::services::ServeDir;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use utoipa::{OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;

#[derive(Clone)]
pub struct AppState {
    pub driver: Arc<Mutex<AlfenDriver>>,
    pub snapshot_rx: watch::Receiver<Arc<DriverSnapshot>>,
}

#[derive(Deserialize, ToSchema)]
pub struct ModeBody {
    pub mode: u8,
}

#[derive(Deserialize, ToSchema)]
pub struct StartStopBody {
    /// Preferred numeric value: 1 = enable, 0 = stop
    #[serde(default)]
    pub value: Option<u8>,
    /// Back-compat boolean flag accepted by older UIs
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Deserialize, ToSchema)]
pub struct SetCurrentBody {
    pub amps: f32,
}

#[utoipa::path(get, path = "/api/health", responses(
    (status = 200, description = "Service is healthy")
))]
async fn health() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

#[utoipa::path(get, path = "/api/metrics", responses(
    (status = 200, description = "Driver metrics")
))]
async fn metrics(State(state): State<AppState>) -> impl IntoResponse {
    let snap = state.snapshot_rx.borrow().clone();
    // Compute age_ms from timestamp
    let age_ms = chrono::DateTime::parse_from_rfc3339(&snap.timestamp)
        .ok()
        .and_then(|ts| {
            chrono::Utc::now()
                .signed_duration_since(ts.with_timezone(&chrono::Utc))
                .to_std()
                .ok()
        })
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let body = serde_json::json!({
        "age_ms": age_ms,
        "poll_duration_ms": snap.poll_duration_ms,
        "total_polls": snap.total_polls,
        "overrun_count": snap.overrun_count,
        "poll_interval_ms": snap.poll_interval_ms,
    });
    Json(body)
}

#[utoipa::path(get, path = "/api/status", responses(
    (status = 200, description = "Driver status")
))]
async fn status(State(state): State<AppState>) -> impl IntoResponse {
    // Lock-free path: try to read the latest snapshot, else return unavailable
    let snapshot = state.snapshot_rx.borrow().clone();
    // Always returns something; initial snapshot is minimal but valid
    Json((*snapshot).clone()).into_response()
}

#[utoipa::path(post, path = "/api/mode", request_body = ModeBody, responses((status = 200)))]
async fn set_mode(State(state): State<AppState>, Json(body): Json<ModeBody>) -> impl IntoResponse {
    let mut drv = state.driver.lock().await;
    drv.set_mode(body.mode).await;
    (StatusCode::OK, Json(serde_json::json!({"ok":true})))
}

#[utoipa::path(post, path = "/api/startstop", request_body = StartStopBody, responses((status = 200)))]
async fn set_startstop(
    State(state): State<AppState>,
    Json(body): Json<StartStopBody>,
) -> impl IntoResponse {
    let mut drv = state.driver.lock().await;
    let v = body
        .value
        .or_else(|| body.enabled.map(|b| if b { 1 } else { 0 }))
        .unwrap_or(0);
    drv.set_start_stop(v).await;
    (StatusCode::OK, Json(serde_json::json!({"ok":true})))
}

#[utoipa::path(post, path = "/api/set_current", request_body = SetCurrentBody, responses((status = 200)))]
async fn set_current(
    State(state): State<AppState>,
    Json(body): Json<SetCurrentBody>,
) -> impl IntoResponse {
    let mut drv = state.driver.lock().await;
    drv.set_intended_current(body.amps).await;
    (StatusCode::OK, Json(serde_json::json!({"ok":true})))
}

#[utoipa::path(get, path = "/api/config", responses((status = 200)))]
async fn get_config(State(state): State<AppState>) -> impl IntoResponse {
    let drv = state.driver.lock().await;
    let mut json = serde_json::to_value(drv.config().clone())
        .unwrap_or(serde_json::json!({"error":"serialization"}));
    if let Some(obj) = json.as_object_mut() {
        obj.remove("vehicle");
        obj.remove("vehicles");
    }
    Json(json)
}

#[utoipa::path(put, path = "/api/config", responses((status = 200)))]
async fn put_config(
    State(state): State<AppState>,
    Json(new_cfg_value): Json<serde_json::Value>,
) -> impl IntoResponse {
    let new_cfg: crate::config::Config = match serde_json::from_value(new_cfg_value) {
        Ok(c) => c,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"bad request"})),
            );
        }
    };
    if new_cfg.validate().is_err() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"invalid config"})),
        );
    }
    let mut drv = state.driver.lock().await;
    if drv.update_config(new_cfg).is_err() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":"apply failed"})),
        );
    }
    (StatusCode::OK, Json(serde_json::json!({"ok":true})))
}

#[utoipa::path(get, path = "/api/config/schema", responses((status = 200)))]
async fn get_config_schema() -> impl IntoResponse {
    let schema = schemars::schema_for!(crate::config::Config);
    Json(serde_json::to_value(&schema).unwrap_or(serde_json::json!({"error":"schema"})))
}

#[derive(Debug, Deserialize, ToSchema, utoipa::IntoParams)]
pub struct TailParams {
    pub lines: Option<usize>,
}

#[utoipa::path(get, path = "/api/logs/tail", params(TailParams), responses((status = 200)))]
async fn logs_tail(
    State(state): State<AppState>,
    Query(params): Query<TailParams>,
) -> impl IntoResponse {
    let (path, max_lines) = {
        let drv = state.driver.lock().await;
        (
            drv.config().logging.file.clone(),
            params.lines.unwrap_or(200).min(10_000),
        )
    };
    match tokio::fs::read_to_string(&path).await {
        Ok(contents) => {
            let mut lines: Vec<&str> = contents.lines().collect();
            if lines.len() > max_lines {
                lines = lines.split_off(lines.len() - max_lines);
            }
            let body = lines.join("\n");
            let mut resp = Response::new(body.into());
            resp.headers_mut().insert(
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("text/plain; charset=utf-8"),
            );
            resp
        }
        Err(_) => (StatusCode::NOT_FOUND, "Log file not available").into_response(),
    }
}

#[utoipa::path(get, path = "/api/logs/head", params(TailParams), responses((status = 200)))]
async fn logs_head(
    State(state): State<AppState>,
    Query(params): Query<TailParams>,
) -> impl IntoResponse {
    let (path, max_lines) = {
        let drv = state.driver.lock().await;
        (
            drv.config().logging.file.clone(),
            params.lines.unwrap_or(200).min(10_000),
        )
    };
    match tokio::fs::read_to_string(&path).await {
        Ok(contents) => {
            let mut lines: Vec<&str> = contents.lines().collect();
            if lines.len() > max_lines {
                lines.truncate(max_lines);
            }
            let body = lines.join("\n");
            let mut resp = Response::new(body.into());
            resp.headers_mut().insert(
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("text/plain; charset=utf-8"),
            );
            resp
        }
        Err(_) => (StatusCode::NOT_FOUND, "Log file not available").into_response(),
    }
}

#[utoipa::path(get, path = "/api/logs/download", responses((status = 200)))]
async fn logs_download(State(state): State<AppState>) -> impl IntoResponse {
    let path = {
        let drv = state.driver.lock().await;
        drv.config().logging.file.clone()
    };
    match tokio::fs::read(&path).await {
        Ok(bytes) => {
            let mut resp = Response::new(bytes.into());
            resp.headers_mut().insert(
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("application/octet-stream"),
            );
            resp
        }
        Err(_) => (StatusCode::NOT_FOUND, "Log file not available").into_response(),
    }
}

#[utoipa::path(get, path = "/api/sessions", responses((status = 200)))]
async fn sessions(State(state): State<AppState>) -> impl IntoResponse {
    let drv = state.driver.lock().await;
    Json(drv.sessions_snapshot())
}

#[utoipa::path(get, path = "/api/dbus", responses((status = 200)))]
async fn dbus_dump(State(state): State<AppState>) -> impl IntoResponse {
    let drv = state.driver.lock().await;
    Json(drv.get_dbus_cache_snapshot())
}

#[utoipa::path(get, path = "/api/update/status", responses((status = 200)))]
async fn update_status() -> impl IntoResponse {
    let updater = crate::updater::GitUpdater::new(
        "https://github.com/your-org/phaeton".to_string(),
        "main".to_string(),
    );
    Json(
        serde_json::to_value(updater.get_status()).unwrap_or(serde_json::json!({"error":"status"})),
    )
}

#[utoipa::path(post, path = "/api/update/check", responses((status = 200)))]
async fn update_check() -> impl IntoResponse {
    let mut updater = crate::updater::GitUpdater::new(
        "https://github.com/your-org/phaeton".to_string(),
        "main".to_string(),
    );
    match updater.check_for_updates().await {
        Ok(st) => (StatusCode::OK, Json(serde_json::to_value(st).unwrap())),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

#[utoipa::path(post, path = "/api/update/apply", responses((status = 200)))]
async fn update_apply() -> impl IntoResponse {
    let mut updater = crate::updater::GitUpdater::new(
        "https://github.com/your-org/phaeton".to_string(),
        "main".to_string(),
    );
    match updater.apply_updates().await {
        Ok(_) => (StatusCode::OK, Json(serde_json::json!({"status":"ok"}))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

#[utoipa::path(get, path = "/api/events", responses((status = 200)))]
async fn events(State(state): State<AppState>) -> impl IntoResponse {
    let rx = state.snapshot_rx.clone();
    let stream = WatchStream::new(rx).map(|snapshot| {
        let payload = serde_json::to_string(&*snapshot).unwrap_or("{}".to_string());
        Ok::<Event, std::convert::Infallible>(Event::default().event("status").data(payload))
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[derive(OpenApi)]
#[openapi(
    paths(
        health, status, set_mode, set_startstop, set_current,
        get_config, put_config, get_config_schema,
        logs_tail, logs_head, logs_download,
        sessions, dbus_dump, update_status, update_check, update_apply,
        events, metrics,
    ),
    components(schemas(ModeBody, StartStopBody, SetCurrentBody, TailParams)),
    tags((name = "phaeton", description = "Phaeton EV Charger API"))
)]
pub struct ApiDoc;

pub fn build_router(state: AppState) -> Router {
    let openapi = ApiDoc::openapi();

    Router::new()
        .route("/", get(|| async { Redirect::to("/ui/index.html") }))
        .route("/api/health", get(health))
        .route("/api/metrics", get(metrics))
        .route("/api/status", get(status))
        .route("/api/mode", post(set_mode))
        .route("/api/startstop", post(set_startstop))
        .route("/api/set_current", post(set_current))
        .route("/api/config", get(get_config).put(put_config))
        .route("/api/config/schema", get(get_config_schema))
        .route("/api/logs/tail", get(logs_tail))
        .route("/api/logs/head", get(logs_head))
        .route("/api/logs/download", get(logs_download))
        .route("/api/sessions", get(sessions))
        .route("/api/dbus", get(dbus_dump))
        .route("/api/update/status", get(update_status))
        .route("/api/update/check", post(update_check))
        .route("/api/update/apply", post(update_apply))
        .route("/api/events", get(events))
        .nest_service(
            "/ui",
            get_service(ServeDir::new("./webui").append_index_html_on_directories(true))
                .handle_error(|_| async { StatusCode::INTERNAL_SERVER_ERROR }),
        )
        .nest_service(
            "/app",
            get_service(ServeDir::new("./webui").append_index_html_on_directories(true))
                .handle_error(|_| async { StatusCode::INTERNAL_SERVER_ERROR }),
        )
        .merge(SwaggerUi::new("/docs").url("/openapi.json", openapi))
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}

pub async fn serve(driver: Arc<Mutex<AlfenDriver>>, host: &str, port: u16) -> anyhow::Result<()> {
    let snapshot_rx = {
        let drv = driver.lock().await;
        drv.subscribe_snapshot()
    };
    let state = AppState {
        driver,
        snapshot_rx,
    };
    let router = build_router(state);

    // Structured logs for web server startup and binding
    let logger = crate::logging::get_logger("web");
    {
        let msg = format!(
            "Starting web server; requested host={}, port={}",
            host, port
        );
        logger.info(&msg);
    }

    let (addr, parsed_ok): (SocketAddr, bool) = match host.parse::<IpAddr>() {
        Ok(ip) => (SocketAddr::new(ip, port), true),
        Err(_) => (([127, 0, 0, 1], port).into(), false),
    };
    if !parsed_ok {
        let warn_msg = format!("Invalid host '{}'; falling back to 127.0.0.1", host);
        logger.warn(&warn_msg);
    }
    {
        let bind_msg = format!("Binding web server to {}:{}", addr.ip(), addr.port());
        logger.info(&bind_msg);
    }

    let listener = tokio::net::TcpListener::bind(addr).await?;
    let local_addr = listener.local_addr()?;
    {
        let listen_msg = format!(
            "Web server listening at http://{}:{} (UI /ui, API /api, docs /docs)",
            local_addr.ip(),
            local_addr.port()
        );
        logger.info(&listen_msg);
    }

    axum::serve(listener, router).await?;
    Ok(())
}
