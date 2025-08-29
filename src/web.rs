//! Axum-based HTTP server with OpenAPI (utoipa) and Swagger UI
use crate::driver::{AlfenDriver, DriverSnapshot};
#[cfg(feature = "tibber")]
use crate::tibber;
use crate::web_schema;
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
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::WatchStream;
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
#[cfg(feature = "openapi")]
use utoipa::OpenApi;
#[cfg(feature = "openapi")]
use utoipa_swagger_ui::SwaggerUi;

#[derive(Clone)]
pub struct AppState {
    pub driver: Arc<Mutex<AlfenDriver>>,
    pub snapshot_rx: watch::Receiver<Arc<DriverSnapshot>>,
}

#[derive(Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ModeBody {
    pub mode: u8,
}

#[derive(Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct StartStopBody {
    /// Preferred numeric value: 1 = enable, 0 = stop
    #[serde(default)]
    pub value: Option<u8>,
    /// Back-compat boolean flag accepted by older UIs
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SetCurrentBody {
    pub amps: f32,
}

#[cfg_attr(feature = "openapi", utoipa::path(get, path = "/api/health", responses(
    (status = 200, description = "Service is healthy")
)))]
async fn health() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

#[cfg_attr(feature = "openapi", utoipa::path(get, path = "/api/metrics", responses((status = 200))))]
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
        "modbus_connected": snap.modbus_connected,
        "driver_state": snap.driver_state,
    });
    Json(body)
}

#[cfg_attr(feature = "openapi", utoipa::path(get, path = "/api/status", responses(
    (status = 200, description = "Driver status")
)))]
async fn status(State(state): State<AppState>) -> impl IntoResponse {
    // Lock-free path: try to read the latest snapshot, else return unavailable
    let snapshot = state.snapshot_rx.borrow().clone();
    // Always returns something; initial snapshot is minimal but valid
    Json((*snapshot).clone()).into_response()
}

#[cfg_attr(feature = "openapi", utoipa::path(post, path = "/api/mode", request_body = ModeBody, responses((status = 200))))]
async fn set_mode(State(state): State<AppState>, Json(body): Json<ModeBody>) -> impl IntoResponse {
    let mut drv = state.driver.lock().await;
    drv.set_mode(body.mode).await;
    (StatusCode::OK, Json(serde_json::json!({"ok":true})))
}

#[cfg_attr(feature = "openapi", utoipa::path(post, path = "/api/startstop", request_body = StartStopBody, responses((status = 200))))]
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

#[cfg_attr(feature = "openapi", utoipa::path(post, path = "/api/set_current", request_body = SetCurrentBody, responses((status = 200))))]
async fn set_current(
    State(state): State<AppState>,
    Json(body): Json<SetCurrentBody>,
) -> impl IntoResponse {
    let mut drv = state.driver.lock().await;
    drv.set_intended_current(body.amps).await;
    (StatusCode::OK, Json(serde_json::json!({"ok":true})))
}

#[cfg_attr(feature = "openapi", utoipa::path(get, path = "/api/tibber/plan", responses((status = 200))))]
async fn tibber_plan(State(_state): State<AppState>) -> impl IntoResponse {
    #[cfg(feature = "tibber")]
    {
        let cfg = {
            let drv = _state.driver.lock().await;
            drv.config().tibber.clone()
        };
        match tibber::get_plan_json(&cfg).await {
            Ok(v) => Json(v).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
                .into_response(),
        }
    }
    #[cfg(not(feature = "tibber"))]
    {
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "error": "Tibber feature disabled",
                "points": []
            })),
        )
            .into_response()
    }
}

#[cfg_attr(feature = "openapi", utoipa::path(get, path = "/api/config", responses((status = 200))))]
async fn get_config(State(state): State<AppState>) -> impl IntoResponse {
    let drv = state.driver.lock().await;
    let mut json = serde_json::to_value(drv.config().clone())
        .unwrap_or(serde_json::json!({"error":"serialization"}));
    if let Some(obj) = json.as_object_mut() {
        obj.remove("vehicles");
    }
    Json(json)
}

#[cfg_attr(feature = "openapi", utoipa::path(put, path = "/api/config", responses((status = 200))))]
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

    // Apply and persist
    let cfg_to_save = new_cfg.clone();
    let mut drv = state.driver.lock().await;
    if drv.update_config(new_cfg).is_err() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":"apply failed"})),
        );
    }
    // Try to persist to disk (best-effort)
    let mut saved_path: Option<&'static str> = None;
    if cfg_to_save
        .save_to_file("/data/phaeton_config.yaml")
        .is_ok()
    {
        saved_path = Some("/data/phaeton_config.yaml");
    } else if cfg_to_save.save_to_file("phaeton_config.yaml").is_ok() {
        saved_path = Some("phaeton_config.yaml");
    }
    let body = match saved_path {
        Some(p) => serde_json::json!({"ok": true, "saved": true, "path": p}),
        None => serde_json::json!({"ok": true, "saved": false}),
    };
    (StatusCode::OK, Json(body))
}

#[cfg_attr(feature = "openapi", utoipa::path(get, path = "/api/config/schema", responses((status = 200))))]
async fn get_config_schema() -> impl IntoResponse {
    Json(web_schema::build_ui_schema())
}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, utoipa::IntoParams))]
pub struct TailParams {
    pub lines: Option<usize>,
}

#[cfg_attr(feature = "openapi", utoipa::path(get, path = "/api/logs/tail", params(TailParams), responses((status = 200))))]
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

#[cfg_attr(feature = "openapi", utoipa::path(get, path = "/api/logs/head", params(TailParams), responses((status = 200))))]
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

#[cfg_attr(feature = "openapi", utoipa::path(get, path = "/api/logs/stream", responses((status = 200))))]
async fn logs_stream() -> impl IntoResponse {
    let rx = crate::logging::subscribe_log_lines();
    let stream = BroadcastStream::new(rx).filter_map(|res| match res {
        Ok(line) => Some(Ok::<Event, std::convert::Infallible>(
            Event::default().event("log").data(line),
        )),
        Err(_) => None,
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[cfg_attr(feature = "openapi", utoipa::path(get, path = "/api/logs/download", responses((status = 200))))]
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

#[cfg_attr(feature = "openapi", utoipa::path(get, path = "/api/sessions", responses((status = 200))))]
async fn sessions(State(state): State<AppState>) -> impl IntoResponse {
    let drv = state.driver.lock().await;
    Json(drv.sessions_snapshot())
}

#[cfg_attr(feature = "openapi", utoipa::path(get, path = "/api/dbus", responses((status = 200))))]
async fn dbus_dump(State(state): State<AppState>) -> impl IntoResponse {
    let drv = state.driver.lock().await;
    Json(drv.get_dbus_cache_snapshot())
}

#[cfg_attr(feature = "openapi", utoipa::path(get, path = "/api/update/status", responses((status = 200))))]
async fn update_status(State(state): State<AppState>) -> impl IntoResponse {
    let (repo, include_prereleases) = {
        let drv = state.driver.lock().await;
        let cfg = drv.config();
        let repo = if cfg.updates.repository.trim().is_empty() {
            env!("CARGO_PKG_REPOSITORY").to_string()
        } else {
            cfg.updates.repository.clone()
        };
        (repo, cfg.updates.include_prereleases)
    };
    let _ = include_prereleases; // status does not use prereleases flag
    let updater = crate::updater::GitUpdater::new(repo, "main".to_string());
    Json(
        serde_json::to_value(updater.get_status()).unwrap_or(serde_json::json!({"error":"status"})),
    )
}

#[cfg_attr(feature = "openapi", utoipa::path(post, path = "/api/update/check", responses((status = 200))))]
async fn update_check(State(state): State<AppState>) -> impl IntoResponse {
    let (repo, include_prereleases) = {
        let drv = state.driver.lock().await;
        let cfg = drv.config();
        let repo = if cfg.updates.repository.trim().is_empty() {
            env!("CARGO_PKG_REPOSITORY").to_string()
        } else {
            cfg.updates.repository.clone()
        };
        (repo, cfg.updates.include_prereleases)
    };
    let mut updater = crate::updater::GitUpdater::new(repo, "main".to_string());
    match updater
        .check_for_updates_with_prereleases(include_prereleases)
        .await
    {
        Ok(st) => (StatusCode::OK, Json(serde_json::to_value(st).unwrap())),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

#[derive(Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
struct ApplyBody {
    version: Option<String>,
}

#[cfg_attr(feature = "openapi", utoipa::path(post, path = "/api/update/apply", responses((status = 200))))]
async fn update_apply(
    State(state): State<AppState>,
    Json(body): Json<ApplyBody>,
) -> impl IntoResponse {
    let logger = crate::logging::get_logger("web");
    let (repo, include_prereleases) = {
        let drv = state.driver.lock().await;
        let cfg = drv.config();
        let repo = if cfg.updates.repository.trim().is_empty() {
            env!("CARGO_PKG_REPOSITORY").to_string()
        } else {
            cfg.updates.repository.clone()
        };
        (repo, cfg.updates.include_prereleases)
    };
    let mut updater = crate::updater::GitUpdater::new(repo, "main".to_string());
    let tag = body.version;
    if let Some(ref t) = tag {
        logger.info(&format!("Update apply requested for tag {}", t));
    } else {
        logger.info("Update apply requested for latest suitable release");
    }
    let res = if tag.is_some() {
        updater
            .apply_release_with_prereleases(tag, include_prereleases)
            .await
    } else {
        updater
            .apply_updates_with_prereleases(include_prereleases)
            .await
    };
    match res {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({"ok": true, "restarting": true})),
        ),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, {
            logger.error(&format!("Update apply failed: {}", e));
            Json(serde_json::json!({"ok": false, "error": e.to_string()}))
        }),
    }
}

#[cfg_attr(feature = "openapi", utoipa::path(get, path = "/api/update/releases", responses((status = 200))))]
async fn update_releases(State(state): State<AppState>) -> impl IntoResponse {
    let (repo, include_prereleases) = {
        let drv = state.driver.lock().await;
        let cfg = drv.config();
        let repo = if cfg.updates.repository.trim().is_empty() {
            env!("CARGO_PKG_REPOSITORY").to_string()
        } else {
            cfg.updates.repository.clone()
        };
        (repo, cfg.updates.include_prereleases)
    };
    let updater = crate::updater::GitUpdater::new(repo, "main".to_string());
    match updater.list_releases(include_prereleases).await {
        Ok(list) => (StatusCode::OK, Json(serde_json::to_value(list).unwrap())),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

#[cfg_attr(feature = "openapi", utoipa::path(get, path = "/api/events", responses((status = 200))))]
async fn events(State(state): State<AppState>) -> impl IntoResponse {
    let rx = state.snapshot_rx.clone();
    let stream = WatchStream::new(rx).map(|snapshot| {
        let payload = serde_json::to_string(&*snapshot).unwrap_or("{}".to_string());
        Ok::<Event, std::convert::Infallible>(Event::default().event("status").data(payload))
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[cfg(feature = "openapi")]
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        health, status, set_mode, set_startstop, set_current,
        get_config, put_config, get_config_schema,
        logs_tail, logs_head, logs_download,
        logs_stream,
        sessions, dbus_dump, update_status, update_check, update_apply, update_releases,
        events, metrics, tibber_plan,
    ),
    components(schemas(ModeBody, StartStopBody, SetCurrentBody, TailParams)),
    tags((name = "phaeton", description = "Phaeton EV Charger API"))
)]
pub struct ApiDoc;

pub fn build_router(state: AppState) -> Router {
    #[cfg(feature = "openapi")]
    let openapi = ApiDoc::openapi();

    // Static UI routers
    let ui_router = Router::new()
        .fallback_service(
            get_service(ServeDir::new("./webui").append_index_html_on_directories(true))
                .handle_error(|_| async { StatusCode::INTERNAL_SERVER_ERROR }),
        )
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static("no-store, no-cache, must-revalidate, max-age=0"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::PRAGMA,
            header::HeaderValue::from_static("no-cache"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::EXPIRES,
            header::HeaderValue::from_static("0"),
        ));
    let app_router = Router::new()
        .fallback_service(
            get_service(ServeDir::new("./webui").append_index_html_on_directories(true))
                .handle_error(|_| async { StatusCode::INTERNAL_SERVER_ERROR }),
        )
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static("no-store, no-cache, must-revalidate, max-age=0"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::PRAGMA,
            header::HeaderValue::from_static("no-cache"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::EXPIRES,
            header::HeaderValue::from_static("0"),
        ));

    let router = Router::new()
        .route("/", get(|| async { Redirect::to("/ui/index.html") }))
        .route("/api/health", get(health))
        .route("/api/metrics", get(metrics))
        .route("/api/status", get(status))
        .route("/api/mode", post(set_mode))
        .route("/api/startstop", post(set_startstop))
        .route("/api/set_current", post(set_current))
        .route("/api/tibber/plan", get(tibber_plan))
        .route("/api/config", get(get_config).put(put_config))
        .route("/api/config/schema", get(get_config_schema))
        .route("/api/logs/tail", get(logs_tail))
        .route("/api/logs/head", get(logs_head))
        .route("/api/logs/download", get(logs_download))
        .route("/api/logs/stream", get(logs_stream))
        .route("/api/sessions", get(sessions))
        .route("/api/dbus", get(dbus_dump))
        .route("/api/update/status", get(update_status))
        .route("/api/update/check", post(update_check))
        .route("/api/update/apply", post(update_apply))
        .route("/api/update/releases", get(update_releases))
        .route("/api/events", get(events))
        .nest("/ui", ui_router)
        .nest("/app", app_router)
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    #[cfg(feature = "openapi")]
    let router = router.merge(SwaggerUi::new("/docs").url("/openapi.json", openapi));

    router
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

// Tests moved to `src/web_tests.rs` to keep file size within budget
