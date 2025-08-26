use super::AlfenDriver;
use crate::error::Result;
use std::sync::Arc;

pub(crate) async fn run_on_arc_impl(driver: Arc<tokio::sync::Mutex<AlfenDriver>>) -> Result<()> {
    let mut drv = driver.lock().await;
    drv.run().await
}
