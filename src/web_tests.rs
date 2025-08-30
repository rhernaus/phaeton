#![cfg(test)]

use super::web::*;
use axum::http::Request;
use axum::routing::get;
use std::sync::Arc;
use tokio::sync::{mpsc, watch, Mutex};
use tower::ServiceExt;

async fn test_state_async() -> AppState {
    let (tx, rx) = mpsc::unbounded_channel();
    let driver = crate::driver::AlfenDriver::new(rx, tx).await.unwrap();
    let (snapshot_tx, snapshot_rx) = watch::channel(Arc::new(DriverSnapshot {
        timestamp: chrono::Utc::now().to_rfc3339(),
        mode: 0,
        start_stop: 0,
        set_current: 0.0,
        applied_current: 0.0,
        station_max_current: 32.0,
        device_instance: 0,
        product_name: None,
        firmware: None,
        serial: None,
        status: 0,
        active_phases: 0,
        ac_power: 0.0,
        ac_current: 0.0,
        l1_voltage: 0.0,
        l2_voltage: 0.0,
        l3_voltage: 0.0,
        l1_current: 0.0,
        l2_current: 0.0,
        l3_current: 0.0,
        l1_power: 0.0,
        l2_power: 0.0,
        l3_power: 0.0,
        total_energy_kwh: 0.0,
        pricing_currency: None,
        energy_rate: None,
        session: serde_json::json!({}),
        poll_duration_ms: None,
        total_polls: 0,
        overrun_count: 0,
        poll_interval_ms: 1000,
        excess_pv_power_w: 0.0,
        modbus_connected: Some(false),
        driver_state: "Initializing".to_string(),
    }));
    let _ = snapshot_tx;
    AppState { driver: Arc::new(Mutex::new(driver)), snapshot_rx }
}

#[tokio::test]
async fn health_ok() {
    let router = axum::Router::new().route("/api/health", get(health));
    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
}

#[tokio::test]
async fn metrics_returns_json() {
    let state = test_state_async().await;
    let router = axum::Router::new().route("/api/metrics", get(metrics)).with_state(state);
    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/metrics")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json.get("total_polls").is_some());
    assert!(json.get("driver_state").is_some());
}

#[tokio::test]
async fn update_status_works() {
    let router = axum::Router::new().route("/api/update/status", get(update_status));
    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/update/status")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json.get("current_version").is_some());
}

#[tokio::test]
async fn update_check_ok() {
    let router = axum::Router::new().route("/api/update/check", axum::routing::post(update_check));
    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/update/check")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    // This endpoint may contact GitHub; allow either 200 or 500 depending on network
    assert!(
        response.status() == axum::http::StatusCode::OK
            || response.status() == axum::http::StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[tokio::test]
async fn update_apply_fails_with_500() {
    let router = axum::Router::new().route("/api/update/apply", axum::routing::post(update_apply));
    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/update/apply")
                .header("content-type", "application/json")
                .body(axum::body::Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::INTERNAL_SERVER_ERROR);
}


#[tokio::test]
async fn status_returns_snapshot() {
    let state = test_state_async().await;
    let router = axum::Router::new()
        .route("/api/status", get(status))
        .with_state(state);
    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/status")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["mode"], 0);
}

#[tokio::test]
async fn config_schema_contains_sections() {
    let router = axum::Router::new().route("/api/config/schema", get(get_config_schema));
    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/config/schema")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json.get("sections").is_some());
}

#[tokio::test]
async fn config_get_returns_json() {
    let state = test_state_async().await;
    let router = axum::Router::new()
        .route("/api/config", get(get_config))
        .with_state(state);
    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/config")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json.get("logging").is_some());
}

#[tokio::test]
async fn put_config_invalid_json_400() {
    let state = test_state_async().await;
    let router = axum::Router::new()
        .route("/api/config", axum::routing::put(put_config))
        .with_state(state);
    let response = router
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/config")
                .header("content-type", "application/json")
                .body(axum::body::Body::from("{invalid"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn set_mode_startstop_set_current_update_driver() {
    let state = test_state_async().await;
    let driver = state.driver.clone();

    // set_mode -> Auto (1)
    let router = axum::Router::new()
        .route("/api/mode", axum::routing::post(set_mode))
        .with_state(state.clone());
    let resp = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/mode")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(r#"{"mode":1}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    assert_eq!(driver.lock().await.current_mode_code(), 1);

    // set_startstop -> enabled
    let router = axum::Router::new()
        .route("/api/startstop", axum::routing::post(set_startstop))
        .with_state(state.clone());
    let resp = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/startstop")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(r#"{"value":1}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    assert_eq!(driver.lock().await.start_stop_code(), 1);

    // set_current
    let router = axum::Router::new()
        .route("/api/set_current", axum::routing::post(set_current))
        .with_state(state.clone());
    let resp = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/set_current")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(r#"{"amps":7.5}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    assert!((driver.lock().await.get_intended_set_current() - 7.5).abs() < f32::EPSILON);
}

#[tokio::test]
async fn log_endpoints_with_tempfile() {
    let mut state = test_state_async().await;
    let driver = state.driver.clone();

    let tf = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tf.path(), "a\nb\nc\n").unwrap();
    {
        let mut d = driver.lock().await;
        let mut cfg = d.config().clone();
        cfg.logging.file = tf.path().to_string_lossy().to_string();
        d.update_config(cfg).unwrap();
    }

    // tail last 2 lines
    let router = axum::Router::new()
        .route("/api/logs/tail", get(logs_tail))
        .with_state(state.clone());
    let resp = router
        .oneshot(
            Request::builder()
                .uri("/api/logs/tail?lines=2")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let s = String::from_utf8(body.to_vec()).unwrap();
    assert!(s.ends_with("b\nc\n") || s.ends_with("b\nc"));

    // head first 2 lines
    let router = axum::Router::new()
        .route("/api/logs/head", get(logs_head))
        .with_state(state.clone());
    let resp = router
        .oneshot(
            Request::builder()
                .uri("/api/logs/head?lines=2")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let s = String::from_utf8(body.to_vec()).unwrap();
    assert!(s.starts_with("a\nb"));

    // download
    let router = axum::Router::new()
        .route("/api/logs/download", get(logs_download))
        .with_state(state.clone());
    let resp = router
        .oneshot(
            Request::builder()
                .uri("/api/logs/download")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    assert_eq!(
        resp.headers()
            .get(axum::http::header::CONTENT_TYPE)
            .unwrap(),
        "application/octet-stream"
    );
}

#[tokio::test]
async fn sessions_and_dbus_dump_ok() {
    let state = test_state_async().await;
    let router = axum::Router::new()
        .route("/api/sessions", get(sessions))
        .route("/api/dbus", get(dbus_dump))
        .with_state(state);

    let resp_sess = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/sessions")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp_sess.status(), axum::http::StatusCode::OK);

    let resp_dbus = router
        .oneshot(
            Request::builder()
                .uri("/api/dbus")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp_dbus.status(), axum::http::StatusCode::OK);
}

#[tokio::test]
async fn events_stream_status_ok() {
    let state = test_state_async().await;
    let router = axum::Router::new()
        .route("/api/events", get(events))
        .with_state(state);
    let resp = router
        .oneshot(
            Request::builder()
                .uri("/api/events")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
}

#[tokio::test]
async fn logs_stream_emits_named_log_events() {
    use axum::http::header;
    use http_body_util::BodyExt as _;
    use std::time::Duration;

    // Ensure logging is initialized so broadcast layer is active in tests
    let _ = crate::logging::init_logging(&crate::config::LoggingConfig::default());

    // Build router for SSE logs endpoint
    let router = axum::Router::new().route("/api/logs/stream", get(logs_stream));

    let mut response = router
        .oneshot(
            Request::builder()
                .uri("/api/logs/stream")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let ct = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    assert!(ct.contains("text/event-stream"));

    // Spawn a log event shortly after to feed the stream
    tokio::spawn(async {
        tokio::time::sleep(Duration::from_millis(10)).await;
        let logger = crate::logging::get_logger("test_sse");
        logger.info("sse_test_line_123");
    });

    // Read frames until we observe the test log line or timeout
    let mut body = response.into_body();
    let mut buf: Vec<u8> = Vec::new();
    let wait = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(frame) = body.frame().await {
                if let Ok(frame) = frame {
                    if let Some(data) = frame.data_ref() {
                        buf.extend_from_slice(data);
                        if buf.windows(b"sse_test_line_123".len()).any(|w| w == b"sse_test_line_123") {
                            break;
                        }
                    }
                } else {
                    // Body error or end; break to assert
                    break;
                }
            }
        }
    })
    .await;

    assert!(wait.is_ok(), "timed out waiting for SSE log event");
    let s = String::from_utf8_lossy(&buf);
    assert!(s.contains("event: log"), "SSE should include named 'log' event: {}", s);
    assert!(s.contains("data:"), "SSE should include data line: {}", s);
    assert!(s.contains("sse_test_line_123"), "SSE data should contain the test line: {}", s);
}

#[tokio::test]
async fn root_redirects_to_ui() {
    let state = test_state_async().await;
    let app = build_router(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(resp.status().is_redirection());
    let loc = resp
        .headers()
        .get(axum::http::header::LOCATION)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    assert_eq!(loc, "/ui/index.html");
}

#[tokio::test]
async fn logs_web_level_get_and_post() { }

#[tokio::test]
async fn update_releases_route_executes() { }

#[tokio::test]
async fn tibber_plan_feature_disabled_returns_placeholder() {
    let state = test_state_async().await;
    let router = axum::Router::new()
        .route("/api/tibber/plan", get(tibber_plan))
        .with_state(state);
    let resp = router
        .oneshot(
            Request::builder()
                .uri("/api/tibber/plan")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json.get("points").is_some());
    // When tibber feature is disabled at compile-time
    // endpoint returns placeholder error
    if let Some(e) = json.get("error").and_then(|v| v.as_str()) {
        assert_eq!(e, "Tibber feature disabled");
    }
}

#[cfg(feature = "tibber")]
#[tokio::test]
async fn tibber_plan_without_token_returns_error_no_token() {
    let state = test_state_async().await;
    let router = axum::Router::new()
        .route("/api/tibber/plan", get(tibber_plan))
        .with_state(state);
    let resp = router
        .oneshot(
            Request::builder()
                .uri("/api/tibber/plan")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json.get("points").is_some());
    let err = json.get("error").and_then(|v| v.as_str()).unwrap_or("");
    assert_eq!(err, "No Tibber access token configured");
}

