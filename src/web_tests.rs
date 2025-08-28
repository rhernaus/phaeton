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
    assert_eq!(response.status(), axum::http::StatusCode::OK);
}

#[tokio::test]
async fn update_apply_fails_with_500() {
    let router = axum::Router::new().route("/api/update/apply", axum::routing::post(update_apply));
    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/update/apply")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::INTERNAL_SERVER_ERROR);
}


