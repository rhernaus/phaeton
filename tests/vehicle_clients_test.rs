use phaeton::vehicle::VehicleClient;
#[tokio::test]
async fn tesla_client_returns_unimplemented_error() {
    let c = phaeton::vehicle::TeslaVehicleClient::new("token".into(), None, None);
    let err = c.fetch_status().await.unwrap_err();
    assert!(
        err.to_string()
            .to_lowercase()
            .contains("not yet implemented")
    );
}

#[tokio::test]
async fn kia_client_returns_unimplemented_error() {
    let c = phaeton::vehicle::KiaVehicleClient::new(
        "user".into(),
        "pass".into(),
        "1234".into(),
        "EU".into(),
        "Kia".into(),
        None,
    );
    let err = c.fetch_status().await.unwrap_err();
    assert!(
        err.to_string()
            .to_lowercase()
            .contains("not yet implemented")
    );
}
