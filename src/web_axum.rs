//! Axum-based HTTP server with OpenAPI (utoipa) and Swagger UI

use crate::driver::AlfenDriver;
use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::{get, post}, Json, Router};
use serde::Deserialize;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use utoipa::{OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;

#[derive(Clone)]
pub struct AppState {
    pub driver: Arc<Mutex<AlfenDriver>>,
}

#[derive(Deserialize, ToSchema)]
pub struct ModeBody {
    pub mode: u8,
}

#[derive(Deserialize, ToSchema)]
pub struct StartStopBody {
    pub value: u8,
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

#[utoipa::path(get, path = "/api/status", responses(
    (status = 200, description = "Driver status")
))]
async fn status(State(state): State<AppState>) -> impl IntoResponse {
    let drv = state.driver.lock().await;
    let mut s = serde_json::json!({
        "mode": drv.current_mode_code(),
        "start_stop": drv.start_stop_code(),
        "set_current": drv.get_intended_set_current(),
        "station_max_current": drv.get_station_max_current(),
        "device_instance": drv.config().device_instance,
    });
    if let Some(v) = drv.get_db_value("/Ac/Power") { s["ac_power"] = v; }
    if let Some(v) = drv.get_db_value("/Ac/Energy/Forward") { s["energy_forward_kwh"] = v; }
    Json(s)
}

#[utoipa::path(post, path = "/api/mode", request_body = ModeBody, responses((status = 200)))]
async fn set_mode(State(state): State<AppState>, Json(body): Json<ModeBody>) -> impl IntoResponse {
    let mut drv = state.driver.lock().await;
    drv.set_mode(body.mode).await;
    (StatusCode::OK, Json(serde_json::json!({"ok":true})))
}

#[utoipa::path(post, path = "/api/startstop", request_body = StartStopBody, responses((status = 200)))]
async fn set_startstop(State(state): State<AppState>, Json(body): Json<StartStopBody>) -> impl IntoResponse {
    let mut drv = state.driver.lock().await;
    drv.set_start_stop(body.value).await;
    (StatusCode::OK, Json(serde_json::json!({"ok":true})))
}

#[utoipa::path(post, path = "/api/set_current", request_body = SetCurrentBody, responses((status = 200)))]
async fn set_current(State(state): State<AppState>, Json(body): Json<SetCurrentBody>) -> impl IntoResponse {
    let mut drv = state.driver.lock().await;
    drv.set_intended_current(body.amps).await;
    (StatusCode::OK, Json(serde_json::json!({"ok":true})))
}

#[utoipa::path(get, path = "/api/config", responses((status = 200)))]
async fn get_config(State(state): State<AppState>) -> impl IntoResponse {
    let drv = state.driver.lock().await;
    let json = serde_json::to_value(drv.config().clone()).unwrap_or(serde_json::json!({"error":"serialization"}));
    Json(json)
}

#[utoipa::path(get, path = "/api/config/schema", responses((status = 200)))]
async fn get_config_schema() -> impl IntoResponse {
    let schema = schemars::schema_for!(crate::config::Config);
    Json(serde_json::to_value(&schema).unwrap_or(serde_json::json!({"error":"schema"})))
}

#[derive(OpenApi)]
#[openapi(
    paths(health, status, set_mode, set_startstop, set_current, get_config, get_config_schema),
    components(schemas(ModeBody, StartStopBody, SetCurrentBody)),
    tags((name = "phaeton", description = "Phaeton EV Charger API"))
)]
pub struct ApiDoc;

pub async fn serve(driver: Arc<Mutex<AlfenDriver>>, host: &str, port: u16) -> anyhow::Result<()> {
    let state = AppState { driver };

    let openapi = ApiDoc::openapi();

    let router = Router::new()
        .route("/api/health", get(health))
        .route("/api/status", get(status))
        .route("/api/mode", post(set_mode))
        .route("/api/startstop", post(set_startstop))
        .route("/api/set_current", post(set_current))
        .route("/api/config", get(get_config))
        .route("/api/config/schema", get(get_config_schema))
        .merge(SwaggerUi::new("/ui/openapi").url("/openapi.json", openapi.clone()))
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let addr: SocketAddr = format!("{}:{}", host, port).parse().unwrap_or(([127,0,0,1],port).into());
    axum::serve(tokio::net::TcpListener::bind(addr).await?, router).await?;
    Ok(())
}


