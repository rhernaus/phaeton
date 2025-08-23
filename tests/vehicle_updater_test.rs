use phaeton::updater::{GitUpdater, UpdateStatus};
use phaeton::vehicle::VehicleIntegration;

#[test]
fn updater_status_defaults() {
    let upd = GitUpdater::new(
        "https://example.com/repo.git".to_string(),
        "main".to_string(),
    );
    let st: UpdateStatus = upd.get_status();
    assert_eq!(st.current_version, "0.1.0");
    assert!(!st.update_available);
}

#[tokio::test]
async fn vehicle_integration_without_client_errors() {
    let vi = VehicleIntegration::new();
    let res = vi.fetch_vehicle_status().await;
    assert!(res.is_err());
}

#[tokio::test]
async fn updater_check_and_apply() {
    let mut upd = GitUpdater::new(
        "https://example.com/repo.git".to_string(),
        "main".to_string(),
    );
    let st = upd.check_for_updates().await.unwrap();
    assert_eq!(st.current_version, "0.1.0");
    assert!(!st.update_available);

    let err = upd.apply_updates().await.unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("Update functionality not yet implemented"));
}
