//! Tibber API integration for dynamic electricity pricing
//!
//! This module is split across smaller files to keep per-file size within budget.

// Feature-enabled submodules
#[cfg(feature = "tibber")]
pub mod api;
pub mod client;
#[cfg(feature = "tibber")]
pub mod types;

// Re-exports for the public API surface
#[cfg(feature = "tibber")]
pub use api::{check_tibber_schedule, check_tibber_schedule_blocking, get_hourly_overview_text};
pub use client::TibberClient;
#[cfg(feature = "tibber")]
pub use types::PriceLevel;

// Fallback stubs when Tibber feature is disabled
#[cfg(not(feature = "tibber"))]
pub async fn check_tibber_schedule(
    _cfg: &crate::config::TibberConfig,
) -> crate::error::Result<(bool, String)> {
    Ok((false, "Tibber integration disabled".to_string()))
}

#[cfg(not(feature = "tibber"))]
pub fn check_tibber_schedule_blocking(
    _cfg: &crate::config::TibberConfig,
) -> crate::error::Result<(bool, String)> {
    Ok((false, "Tibber integration disabled".to_string()))
}

#[cfg(not(feature = "tibber"))]
pub async fn get_hourly_overview_text(
    _cfg: &crate::config::TibberConfig,
) -> crate::error::Result<String> {
    Ok("Tibber overview: integration disabled".to_string())
}

// Helper used by refresh logic (feature-enabled)
#[cfg(feature = "tibber")]
pub(super) mod runtime_helper_time {
    pub fn now_monotonic_seconds_fallback() -> f64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_else(|_| std::time::Duration::from_secs(0));
        now.as_secs_f64()
    }
}
