use phaeton::tibber::TibberClient;

#[cfg(not(feature = "tibber"))]
#[tokio::test]
async fn tibber_stub_behaviour() {
    let client = TibberClient::new("".to_string(), None);
    let overview = client.get_hourly_overview().await.unwrap();
    assert!(overview.contains("not yet implemented"));
    let should = client.should_charge("level").await.unwrap();
    assert!(should);
}

#[cfg(feature = "tibber")]
#[tokio::test]
async fn tibber_feature_enabled_empty_token() {
    let client = TibberClient::new("".to_string(), None);
    let overview = client.get_hourly_overview().await.unwrap();
    // With feature enabled and no token, we should surface a clear message
    assert!(
        overview.contains("token") || overview.contains("overview") || overview.contains("Tibber")
    );
    let should = client.should_charge("level").await.unwrap();
    assert!(should);
}
