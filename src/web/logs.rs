use axum::response::sse::{Event, KeepAlive, Sse};
use axum::{Json, Router, extract::Query, http::header, response::IntoResponse};
use axum::{http::StatusCode, response::Response};
use axum::{routing::get, routing::post};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use super::AppState;
use axum::extract::State;
use std::time::SystemTime;

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
    let (configured_path, max_lines) = {
        let drv = state.driver.lock().await;
        (
            drv.config().logging.file.clone(),
            params.lines.unwrap_or(200).min(10_000),
        )
    };
    let path = match resolve_log_file_path(&configured_path).await {
        Some(p) => p,
        None => return (StatusCode::NOT_FOUND, "Log file not available").into_response(),
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
    let (configured_path, max_lines) = {
        let drv = state.driver.lock().await;
        (
            drv.config().logging.file.clone(),
            params.lines.unwrap_or(200).min(10_000),
        )
    };
    let path = match resolve_log_file_path(&configured_path).await {
        Some(p) => p,
        None => return (StatusCode::NOT_FOUND, "Log file not available").into_response(),
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
    let configured_path = {
        let drv = state.driver.lock().await;
        drv.config().logging.file.clone()
    };
    let path = match resolve_log_file_path(&configured_path).await {
        Some(p) => p,
        None => return (StatusCode::NOT_FOUND, "Log file not available").into_response(),
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

fn name_matches(file_name: &str, prefix: &str, suffix: &str) -> bool {
    if file_name == format!("{}.{}", prefix, suffix) {
        return true;
    }
    (file_name.starts_with(prefix) && file_name.ends_with(&format!(".{suffix}")))
        || (file_name.starts_with(&format!("{}.", prefix))
            && file_name.contains(&format!(".{suffix}.")))
}

fn derive_search_spec(configured: &Path) -> (PathBuf, String, String) {
    if configured.extension().is_some() {
        let dir = configured.parent().unwrap_or_else(|| Path::new("."));
        let stem = configured
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("phaeton")
            .to_string();
        let ext = configured
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("log")
            .to_string();
        (dir.to_path_buf(), stem, ext)
    } else {
        (
            configured.to_path_buf(),
            "phaeton".to_string(),
            "log".to_string(),
        )
    }
}

async fn configured_file_if_exists(configured: &Path) -> Option<PathBuf> {
    if let Ok(md) = fs::metadata(configured).await
        && md.is_file()
    {
        Some(configured.to_path_buf())
    } else {
        None
    }
}

async fn find_latest_matching(search_dir: &Path, prefix: &str, suffix: &str) -> Option<PathBuf> {
    let mut best_path: Option<PathBuf> = None;
    let mut best_mtime: SystemTime = SystemTime::UNIX_EPOCH;
    let mut stack: Vec<PathBuf> = vec![search_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut rd = match fs::read_dir(&dir).await {
            Ok(v) => v,
            Err(_) => continue,
        };
        while let Ok(Some(entry)) = rd.next_entry().await {
            let ft = match entry.file_type().await {
                Ok(v) => v,
                Err(_) => continue,
            };
            if ft.is_file() {
                if let Some(name) = entry.file_name().to_str()
                    && name_matches(name, prefix, suffix)
                    && let Ok(md) = entry.metadata().await
                    && let Ok(modified) = md.modified()
                    && modified > best_mtime
                {
                    best_mtime = modified;
                    best_path = Some(entry.path());
                }
            } else if ft.is_dir() {
                stack.push(entry.path());
            }
        }
    }
    best_path
}

// Attempt to resolve the actual log file path taking rotation into account.
// If the configured path exists and is a file, use it. Otherwise, search the
// directory tree rooted at the configured path for files that match the
// configured file name pattern and pick the most recently modified one.
async fn resolve_log_file_path(configured_path: &str) -> Option<PathBuf> {
    let configured = Path::new(configured_path);
    if let Some(p) = configured_file_if_exists(configured).await {
        return Some(p);
    }
    let (search_dir, prefix, suffix) = derive_search_spec(configured);
    find_latest_matching(&search_dir, &prefix, &suffix).await
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
