#[tokio::test]
async fn new_with_config_override_invalid_path_errors() {
    use phaeton::driver::AlfenDriver;
    use tokio::sync::mpsc;
    let (tx, rx) = mpsc::unbounded_channel();
    let res = AlfenDriver::new_with_config_override(
        rx,
        tx,
        Some(std::path::PathBuf::from("/definitely/missing/config.yaml")),
    )
    .await;
    assert!(res.is_err());
}
