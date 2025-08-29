use axum::response::sse::{Event, KeepAlive, Sse};
use axum::{Json, Router, extract::Query, http::header, response::IntoResponse};
use axum::{http::StatusCode, response::Response};
use axum::{routing::get, routing::post};
use serde::Deserialize;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use super::AppState;
use axum::extract::State;

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, utoipa::IntoParams))]
pub struct TailParams {
    pub lines: Option<usize>,
}

#[cfg_attr(feature = "openapi", utoipa::path(get, path = "/api/logs/tail", params(TailParams), responses((status = 200))))]
pub async fn logs_tail(
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
pub async fn logs_head(
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
pub async fn logs_stream() -> impl IntoResponse {
    let rx = crate::logging::subscribe_log_lines();
    let stream = BroadcastStream::new(rx).filter_map(|res| match res {
        Ok(line) if crate::logging::should_emit_to_web(&line) => {
            Some(Ok::<Event, std::convert::Infallible>(
                Event::default().event("log").data(line),
            ))
        }
        _ => None,
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[cfg_attr(feature = "openapi", utoipa::path(get, path = "/api/logs/download", responses((status = 200))))]
pub async fn logs_download(State(state): State<AppState>) -> impl IntoResponse {
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

#[derive(Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, utoipa::IntoParams))]
struct WebLevelQuery {
    level: String,
}

#[cfg_attr(feature = "openapi", utoipa::path(post, path = "/api/logs/web_level", params(WebLevelQuery), responses((status = 200))))]
async fn set_web_log_level(Query(q): Query<WebLevelQuery>) -> impl IntoResponse {
    match crate::logging::set_web_log_level_str(&q.level) {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({"ok": true, "level": q.level})),
        ),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"ok": false, "error": e.to_string()})),
        ),
    }
}

#[cfg_attr(feature = "openapi", utoipa::path(get, path = "/api/logs/web_level", responses((status = 200))))]
async fn get_web_log_level() -> impl IntoResponse {
    let lvl = crate::logging::get_web_log_level();
    Json(serde_json::json!({"level": format!("{:?}", lvl)}))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/logs/tail", get(logs_tail))
        .route("/api/logs/head", get(logs_head))
        .route("/api/logs/download", get(logs_download))
        .route("/api/logs/stream", get(logs_stream))
        .route(
            "/api/logs/web_level",
            post(set_web_log_level).get(get_web_log_level),
        )
}
