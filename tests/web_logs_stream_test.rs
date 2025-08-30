#[tokio::test]
async fn logs_stream_emits_named_log_events() {
    use axum::http::Request;
    use axum::routing::get;
    use http_body_util::BodyExt as _;
    use std::time::Duration;
    use tower::ServiceExt;

    // Ensure logging is initialized so broadcast layer is active in tests
    let _ = phaeton::logging::init_logging(&phaeton::config::LoggingConfig::default());

    // Build router for SSE logs endpoint
    let router = axum::Router::new().route("/api/logs/stream", get(phaeton::web::logs_stream));

    let response = router
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
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    assert!(ct.contains("text/event-stream"));

    // Spawn a log event shortly after to feed the stream
    tokio::spawn(async {
        tokio::time::sleep(Duration::from_millis(10)).await;
        let logger = phaeton::logging::get_logger("test_sse");
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
                        if buf
                            .windows(b"sse_test_line_123".len())
                            .any(|w| w == b"sse_test_line_123")
                        {
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
    assert!(
        s.contains("event: log"),
        "SSE should include named 'log' event: {}",
        s
    );
    assert!(s.contains("data:"), "SSE should include data line: {}", s);
    assert!(
        s.contains("sse_test_line_123"),
        "SSE data should contain the test line: {}",
        s
    );
}
