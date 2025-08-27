#[cfg(feature = "tibber")]
use once_cell::sync::Lazy;
#[cfg(feature = "tibber")]
use std::sync::Arc;

use crate::error::Result;

#[cfg(feature = "tibber")]
use crate::tibber::client::TibberClient;
// Shared client across calls for caching
#[cfg(feature = "tibber")]
type Shared = Arc<tokio::sync::Mutex<TibberClient>>;
#[cfg(feature = "tibber")]
type ClientKey = (String, String);
#[cfg(feature = "tibber")]
type SharedClientSlot = Option<(ClientKey, Shared)>;
#[cfg(feature = "tibber")]
type SharedClientState = tokio::sync::Mutex<SharedClientSlot>;
#[cfg(feature = "tibber")]
static SHARED_CLIENT: Lazy<SharedClientState> = Lazy::new(|| tokio::sync::Mutex::new(None));

#[cfg(feature = "tibber")]
async fn get_shared_client(cfg: &crate::config::TibberConfig) -> Shared {
    let mut guard = SHARED_CLIENT.lock().await;
    let key = (cfg.access_token.clone(), cfg.home_id.clone());
    if let Some((existing_key, client)) = guard.as_ref()
        && existing_key == &key
    {
        return client.clone();
    }
    let client = Arc::new(tokio::sync::Mutex::new(TibberClient::new(
        cfg.access_token.clone(),
        if cfg.home_id.is_empty() {
            None
        } else {
            Some(cfg.home_id.clone())
        },
    )));
    *guard = Some((key, client.clone()));
    client
}

/// Check if charging should be enabled based on Tibber pricing and strategy
#[cfg(feature = "tibber")]
pub async fn check_tibber_schedule(cfg: &crate::config::TibberConfig) -> Result<(bool, String)> {
    if cfg.access_token.trim().is_empty() {
        return Ok((false, "No Tibber access token configured".to_string()));
    }

    let shared = get_shared_client(cfg).await;
    let mut client = shared.lock().await;
    let price_level = client.refresh_if_due().await?;

    if price_level.is_none() {
        return Ok((false, "Could not fetch Tibber price".to_string()));
    }

    let should = client.decide_should_charge(cfg, price_level);

    let mut parts: Vec<String> = Vec::new();
    if let Some(pl) = price_level
        && cfg.strategy == "level"
    {
        parts.push(format!("level={}", pl.as_str()));
    }
    if let Some(t) = client.current_total() {
        parts.push(format!("total={:.4}", t));
    }
    if cfg.strategy == "threshold" && cfg.max_price_total > 0.0 {
        parts.push(format!("strategy=threshold<= {:.4}", cfg.max_price_total));
    } else if cfg.strategy == "percentile" {
        if let Some(thr) = client.determine_percentile_threshold(cfg.cheap_percentile) {
            parts.push(format!(
                "strategy=percentile p={:.2} thr={:.4}",
                cfg.cheap_percentile, thr
            ));
        } else {
            parts.push(format!(
                "strategy=percentile p={:.2} (thr n/a)",
                cfg.cheap_percentile
            ));
        }
    }
    let suffix = if should {
        " - charging enabled"
    } else {
        " - waiting for cheaper price"
    };
    let explanation = if parts.is_empty() {
        format!("Tibber decision{}", suffix)
    } else {
        format!("{}{}", parts.join(", "), suffix)
    };
    Ok((should, explanation))
}

/// Synchronous wrapper for `check_tibber_schedule` for non-async call sites
#[cfg(feature = "tibber")]
pub fn check_tibber_schedule_blocking(cfg: &crate::config::TibberConfig) -> Result<(bool, String)> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(check_tibber_schedule(cfg))
}

/// Convenience wrapper to get a textual overview (refreshes cache)
#[cfg(feature = "tibber")]
pub async fn get_hourly_overview_text(cfg: &crate::config::TibberConfig) -> Result<String> {
    if cfg.access_token.trim().is_empty() {
        return Ok("Tibber overview: token missing".to_string());
    }
    let shared = get_shared_client(cfg).await;
    {
        let mut client = shared.lock().await;
        let _ = client.refresh_if_due().await?;
    }
    let client = shared.lock().await;
    let upcoming = client.upcoming_prices();
    if upcoming.is_empty() {
        return Ok("Tibber overview: no upcoming price data available".to_string());
    }
    let header = format!("Tibber hourly overview | strategy={}", cfg.strategy);
    let mut lines = vec![header];
    for p in upcoming {
        lines.push(format!(
            "  {}  total={:.4}  level={}",
            p.starts_at,
            p.total,
            p.level.as_str()
        ));
    }
    Ok(lines.join("\n"))
}
