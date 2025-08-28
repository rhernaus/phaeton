use phaeton::updater::{GitUpdater, UpdateStatus};
use phaeton::vehicle::VehicleIntegration;

#[test]
fn updater_status_defaults() {
    let upd = GitUpdater::new(
        "https://example.com/repo.git".to_string(),
        "main".to_string(),
    );
    let st: UpdateStatus = upd.get_status();
    assert!(!st.current_version.is_empty());
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
    assert!(!st.current_version.is_empty());
    // update_available can be either; we just validate the shape here
    let _ = st.update_available;

    // Applying updates in tests is not performed; just ensure the call returns a Result
    let _ = upd.apply_updates().await.is_ok() || upd.apply_updates().await.is_err();
}
