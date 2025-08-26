use phaeton::vehicle::{VehicleIntegration, VehicleStatus, VehicleClient};

struct StubClient;

#[async_trait::async_trait]
impl VehicleClient for StubClient {
    async fn fetch_status(&self) -> phaeton::error::Result<VehicleStatus> {
        Ok(VehicleStatus {
            name: Some("TestCar".to_string()),
            vin: Some("VIN123".to_string()),
            soc: Some(50.0),
            lat: Some(1.23),
            lon: Some(4.56),
            asleep: Some(false),
            timestamp: Some(1234567890),
        })
    }
}

#[tokio::test]
async fn vehicle_integration_returns_status_when_client_set() {
    let mut vi = VehicleIntegration::new();
    vi.set_client(Box::new(StubClient));
    let st = vi.fetch_vehicle_status().await.unwrap();
    assert_eq!(st.name.as_deref(), Some("TestCar"));
    assert_eq!(st.vin.as_deref(), Some("VIN123"));
    assert_eq!(st.soc, Some(50.0));
}