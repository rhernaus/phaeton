use phaeton::tibber::TibberClient;

#[tokio::test]
async fn tibber_stub_behaviour() {
    let client = TibberClient::new("".to_string(), None);
    let overview = client.get_hourly_overview().await.unwrap();
    assert!(overview.contains("not yet implemented"));
    let should = client.should_charge("level").await.unwrap();
    assert!(should);
}
