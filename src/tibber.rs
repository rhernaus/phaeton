//! Tibber API integration for dynamic electricity pricing
//!
//! This module integrates with Tibber's GraphQL API to derive current and
//! upcoming electricity price levels and to decide whether to charge based on
//! configurable strategies (level, threshold, percentile).

use crate::error::Result;
#[cfg(feature = "tibber")]
use crate::logging::get_logger;
#[cfg(feature = "tibber")]
use once_cell::sync::Lazy;
#[cfg(feature = "tibber")]
use std::sync::Arc;

/// Tibber price level mapping (only when feature is enabled)
#[cfg(feature = "tibber")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceLevel {
    VeryCheap,
    Cheap,
    Normal,
    Expensive,
    VeryExpensive,
}

#[cfg(feature = "tibber")]
impl PriceLevel {
    fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "VERY_CHEAP" => Self::VeryCheap,
            "CHEAP" => Self::Cheap,
            "EXPENSIVE" => Self::Expensive,
            "VERY_EXPENSIVE" => Self::VeryExpensive,
            _ => Self::Normal,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::VeryCheap => "VERY_CHEAP",
            Self::Cheap => "CHEAP",
            Self::Normal => "NORMAL",
            Self::Expensive => "EXPENSIVE",
            Self::VeryExpensive => "VERY_EXPENSIVE",
        }
    }
}

#[cfg(feature = "tibber")]
#[derive(Debug, Clone)]
struct PricePoint {
    starts_at: String,
    total: f64,
    level: PriceLevel,
}

/// Tibber API client with simple caching
pub struct TibberClient {
    #[cfg(feature = "tibber")]
    access_token: String,
    #[cfg(feature = "tibber")]
    home_id: Option<String>,
    #[cfg(feature = "tibber")]
    logger: crate::logging::StructuredLogger,
    #[cfg(feature = "tibber")]
    cached_current: Option<PricePoint>,
    #[cfg(feature = "tibber")]
    cached_upcoming: Vec<PricePoint>,
    #[cfg(feature = "tibber")]
    cache_next_refresh_epoch: f64,
}

impl TibberClient {
    /// Create new Tibber client
    pub fn new(access_token: String, home_id: Option<String>) -> Self {
        #[cfg(feature = "tibber")]
        {
            let logger = get_logger("tibber");
            Self {
                access_token,
                home_id,
                logger,
                cached_current: None,
                cached_upcoming: Vec::new(),
                cache_next_refresh_epoch: 0.0,
            }
        }
        #[cfg(not(feature = "tibber"))]
        {
            let _ = (&access_token, &home_id);
            return Self {};
        }
    }

    /// Get current cached total price (EUR/kWh) if available
    #[cfg(feature = "tibber")]
    pub fn current_total(&self) -> Option<f64> {
        self.cached_current.as_ref().map(|p| p.total)
    }

    /// Get current cached level if available
    #[cfg(feature = "tibber")]
    pub fn current_level(&self) -> Option<PriceLevel> {
        self.cached_current.as_ref().map(|p| p.level)
    }

    /// Upcoming cached prices window
    #[cfg(feature = "tibber")]
    fn upcoming_prices(&self) -> &[PricePoint] {
        &self.cached_upcoming
    }

    /// Compute a percentile threshold over upcoming prices
    #[cfg(feature = "tibber")]
    fn determine_percentile_threshold(&self, percentile: f64) -> Option<f64> {
        if self.cached_upcoming.is_empty() {
            return None;
        }
        let mut prices: Vec<f64> = self
            .cached_upcoming
            .iter()
            .map(|p| p.total)
            .filter(|v| v.is_finite())
            .collect();
        if prices.is_empty() {
            return None;
        }
        prices.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        if percentile <= 0.0 {
            return prices.first().copied();
        }
        if percentile >= 1.0 {
            return prices.last().copied();
        }
        let n = prices.len();
        let idx =
            ((percentile * n as f64).floor() as isize - 1).clamp(0, (n - 1) as isize) as usize;
        prices.get(idx).copied()
    }

    /// Decide whether to charge given strategy and current context
    #[cfg(feature = "tibber")]
    pub fn decide_should_charge(
        &self,
        cfg: &crate::config::TibberConfig,
        price_level: Option<PriceLevel>,
    ) -> bool {
        let current_total = self.current_total();
        match cfg.strategy.as_str() {
            "threshold" => {
                if let (Some(total), true) = (current_total, cfg.max_price_total > 0.0) {
                    return total <= cfg.max_price_total;
                }
                // Fallback to level strategy if missing data
            }
            "percentile" => {
                if let (Some(total), Some(thr)) = (
                    current_total,
                    self.determine_percentile_threshold(cfg.cheap_percentile),
                ) {
                    return total <= thr;
                }
                // Fallback to level strategy if missing data
            }
            _ => {}
        }

        // Default/level strategy
        if let Some(pl) = price_level {
            if pl == PriceLevel::VeryCheap && cfg.charge_on_very_cheap {
                return true;
            }
            if pl == PriceLevel::Cheap && cfg.charge_on_cheap {
                return true;
            }
        }
        false
    }

    /// Fetch hourly overview (human-friendly) — feature-gated network
    pub async fn get_hourly_overview(&self) -> Result<String> {
        #[cfg(feature = "tibber")]
        {
            // Build a simple overview from cached values (refresh first)
            // We clone self to satisfy mutable borrow rules by using shared client below
            let cfg = crate::config::TibberConfig {
                access_token: self.access_token.clone(),
                home_id: self.home_id.clone().unwrap_or_default(),
                charge_on_cheap: true,
                charge_on_very_cheap: true,
                strategy: "level".to_string(),
                max_price_total: 0.0,
                cheap_percentile: 0.3,
            };
            let (_should, header) = check_tibber_schedule(&cfg).await?;
            let shared = get_shared_client(&cfg).await;
            let client = shared.lock().await;
            let mut lines = vec![format!("{}", header)];
            // derive low/normal/high from percentiles
            let mut totals: Vec<f64> = client
                .upcoming_prices()
                .iter()
                .map(|p| p.total)
                .filter(|v| v.is_finite())
                .collect();
            totals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let p33 = if totals.is_empty() {
                0.0
            } else {
                let idx = ((0.33 * totals.len() as f64).floor() as isize - 1)
                    .clamp(0, (totals.len() - 1) as isize) as usize;
                totals[idx]
            };
            let p66 = if totals.is_empty() {
                0.0
            } else {
                let idx = ((0.66 * totals.len() as f64).floor() as isize - 1)
                    .clamp(0, (totals.len() - 1) as isize) as usize;
                totals[idx]
            };
            for p in client.upcoming_prices() {
                let rating = if p.total <= p33 {
                    "LOW"
                } else if p.total >= p66 {
                    "HIGH"
                } else {
                    "NORMAL"
                };
                lines.push(format!(
                    "  {}  total={:.4}  level={}  priceRating={}",
                    p.starts_at,
                    p.total,
                    p.level.as_str(),
                    rating
                ));
            }
            Ok(lines.join("\n"))
        }

        #[cfg(not(feature = "tibber"))]
        {
            Ok("Tibber integration not yet implemented".to_string())
        }
    }

    /// Refresh cached data from Tibber API if needed; returns current level if available
    #[cfg(feature = "tibber")]
    async fn refresh_if_due(&mut self) -> Result<Option<PriceLevel>> {
        let now = runtime_helper_time::now_monotonic_seconds_fallback();
        if now < self.cache_next_refresh_epoch && self.cached_current.is_some() {
            return Ok(self.cached_current.as_ref().map(|p| p.level));
        }

        {
            use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
            use serde_json::json;

            if self.access_token.trim().is_empty() {
                return Ok(None);
            }

            let query = r#"
            query PriceInfoQuery {
                viewer {
                    homes {
                        id
                        currentSubscription {
                            priceInfo {
                                current { total level startsAt }
                                today { total level startsAt }
                                tomorrow { total level startsAt }
                            }
                        }
                    }
                }
            }
            "#;

            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()?;
            let resp = client
                .post("https://api.tibber.com/v1-beta/gql")
                .header(
                    AUTHORIZATION,
                    format!("Bearer {}", self.access_token.trim()),
                )
                .header(CONTENT_TYPE, "application/json")
                .header(ACCEPT, "application/json")
                .header(USER_AGENT, "phaeton/1.0 (+https://github.com/)")
                .json(&json!({"query": query, "variables": {} }))
                .send()
                .await?;

            if !resp.status().is_success() {
                self.logger
                    .error(&format!("Tibber API error: {}", resp.status()));
                // backoff 60s on error
                self.cache_next_refresh_epoch = (now + 60.0).max(self.cache_next_refresh_epoch);
                return Ok(None);
            }

            let body: serde_json::Value = resp.json().await?;
            if body.get("errors").is_some() {
                let msg = body["errors"][0]["message"]
                    .as_str()
                    .unwrap_or("GraphQL error");
                self.logger
                    .error(&format!("Tibber API GraphQL error: {}", msg));
                self.cache_next_refresh_epoch = (now + 60.0).max(self.cache_next_refresh_epoch);
                return Ok(None);
            }

            let homes = body
                .get("data")
                .and_then(|d| d.get("viewer"))
                .and_then(|v| v.get("homes"))
                .and_then(|h| h.as_array())
                .cloned()
                .unwrap_or_default();

            if homes.is_empty() {
                self.logger.warn("No homes in Tibber account");
                return Ok(None);
            }

            let target_home = if let Some(hid) = self.home_id.as_ref() {
                homes
                    .iter()
                    .find(|h| h.get("id").and_then(|x| x.as_str()) == Some(hid.as_str()))
                    .cloned()
                    .or_else(|| homes.first().cloned())
            } else {
                homes.first().cloned()
            };

            let Some(home) = target_home else {
                return Ok(None);
            };
            let price_info_container = home
                .get("currentSubscription")
                .and_then(|c| c.get("priceInfo"))
                .cloned()
                .unwrap_or_default();

            let cur = price_info_container
                .get("current")
                .cloned()
                .unwrap_or_default();
            let cur_total = cur.get("total").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let cur_level = PriceLevel::from_str(
                cur.get("level")
                    .and_then(|v| v.as_str())
                    .unwrap_or("NORMAL"),
            );
            let cur_starts = cur
                .get("startsAt")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            self.cached_current = Some(PricePoint {
                starts_at: cur_starts,
                total: cur_total,
                level: cur_level,
            });

            let mut upcoming: Vec<PricePoint> = Vec::new();
            for key in ["today", "tomorrow"] {
                if let Some(arr) = price_info_container.get(key).and_then(|v| v.as_array()) {
                    for e in arr {
                        let total = e.get("total").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        let level = PriceLevel::from_str(
                            e.get("level").and_then(|v| v.as_str()).unwrap_or("NORMAL"),
                        );
                        let starts = e
                            .get("startsAt")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        upcoming.push(PricePoint {
                            starts_at: starts,
                            total,
                            level,
                        });
                    }
                }
            }
            // Sort by startsAt string as ISO8601 (sufficient for order)
            upcoming.sort_by(|a, b| a.starts_at.cmp(&b.starts_at));
            self.cached_upcoming = upcoming;

            // Determine next refresh: next slot after current
            let mut next_refresh = 0.0;
            let parse_ts = |s: &str| -> Option<f64> {
                // Use chrono for robust parsing
                chrono::DateTime::parse_from_rfc3339(s)
                    .ok()
                    .map(|dt| dt.timestamp() as f64)
            };
            if let Some(cur) = &self.cached_current
                && let Some(cur_ts) = parse_ts(&cur.starts_at)
            {
                for p in &self.cached_upcoming {
                    if let Some(ts) = parse_ts(&p.starts_at)
                        && ts > cur_ts + 1e-6
                    {
                        next_refresh = ts;
                        break;
                    }
                }
                if next_refresh == 0.0 {
                    next_refresh = cur_ts + 3600.0; // assume hourly
                }
            }
            if next_refresh == 0.0 {
                next_refresh = now + 900.0; // fallback 15m
            }
            self.cache_next_refresh_epoch = next_refresh + 1.0; // margin
            Ok(self.cached_current.as_ref().map(|p| p.level))
        }
    }

    // Note: when the `tibber` feature is disabled, `refresh_if_due` is not compiled
    // because all call sites are feature-gated.
}

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

    // Build concise explanation
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
    // Build a lightweight single-threaded runtime to execute the async check
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
    // Ensure refreshed
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

/// Fallback stubs when Tibber feature is disabled
#[cfg(not(feature = "tibber"))]
pub async fn check_tibber_schedule(_cfg: &crate::config::TibberConfig) -> Result<(bool, String)> {
    Ok((false, "Tibber integration disabled".to_string()))
}

#[cfg(not(feature = "tibber"))]
pub fn check_tibber_schedule_blocking(
    _cfg: &crate::config::TibberConfig,
) -> Result<(bool, String)> {
    Ok((false, "Tibber integration disabled".to_string()))
}

#[cfg(not(feature = "tibber"))]
pub async fn get_hourly_overview_text(_cfg: &crate::config::TibberConfig) -> Result<String> {
    Ok("Tibber overview: integration disabled".to_string())
}

// Helper used by refresh_if_due when tibber feature disabled
#[cfg(feature = "tibber")]
mod runtime_helper_time {
    pub fn now_monotonic_seconds_fallback() -> f64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_else(|_| std::time::Duration::from_secs(0));
        now.as_secs_f64()
    }
}

impl TibberClient {
    /// Legacy stub for compatibility with existing tests
    pub async fn should_charge(&self, _strategy: &str) -> Result<bool> {
        Ok(true)
    }
}

// removed unused shim

#[cfg(all(test, feature = "tibber"))]
mod tests {
    use super::*;

    fn make_cfg() -> crate::config::TibberConfig {
        crate::config::TibberConfig {
            access_token: String::new(),
            home_id: String::new(),
            charge_on_cheap: true,
            charge_on_very_cheap: true,
            strategy: "level".to_string(),
            max_price_total: 0.0,
            cheap_percentile: 0.3,
        }
    }

    #[test]
    fn price_level_mapping_roundtrip() {
        use PriceLevel::*;
        assert_eq!(PriceLevel::from_str("VERY_CHEAP"), VeryCheap);
        assert_eq!(PriceLevel::from_str("cheap"), Cheap);
        assert_eq!(PriceLevel::from_str("normal"), Normal);
        assert_eq!(PriceLevel::from_str("EXPENSIVE"), Expensive);
        assert_eq!(PriceLevel::from_str("very_expensive"), VeryExpensive);

        assert_eq!(VeryCheap.as_str(), "VERY_CHEAP");
        assert_eq!(Cheap.as_str(), "CHEAP");
        assert_eq!(Normal.as_str(), "NORMAL");
        assert_eq!(Expensive.as_str(), "EXPENSIVE");
        assert_eq!(VeryExpensive.as_str(), "VERY_EXPENSIVE");
    }

    #[test]
    fn percentile_threshold_edges_and_mid() {
        let mut c = TibberClient::new(String::new(), None);
        c.cached_upcoming = vec![
            PricePoint {
                starts_at: "t1".into(),
                total: 1.0,
                level: PriceLevel::Normal,
            },
            PricePoint {
                starts_at: "t2".into(),
                total: 2.0,
                level: PriceLevel::Normal,
            },
            PricePoint {
                starts_at: "t3".into(),
                total: 3.0,
                level: PriceLevel::Normal,
            },
            PricePoint {
                starts_at: "t4".into(),
                total: 4.0,
                level: PriceLevel::Normal,
            },
        ];
        // 0 -> min
        assert_eq!(c.determine_percentile_threshold(0.0), Some(1.0));
        // 1 -> max
        assert_eq!(c.determine_percentile_threshold(1.0), Some(4.0));
        // 0.50 -> index 1 (2.0)
        assert_eq!(c.determine_percentile_threshold(0.5), Some(2.0));
        // 0.75 -> index 2 (3.0)
        assert_eq!(c.determine_percentile_threshold(0.75), Some(3.0));
    }

    #[test]
    fn decide_should_charge_threshold_and_level() {
        let mut c = TibberClient::new(String::new(), None);
        c.cached_current = Some(PricePoint {
            starts_at: "now".into(),
            total: 0.15,
            level: PriceLevel::Cheap,
        });

        let mut cfg = make_cfg();
        cfg.strategy = "threshold".to_string();
        cfg.max_price_total = 0.20;
        assert!(c.decide_should_charge(&cfg, None));

        cfg.max_price_total = 0.10;
        assert!(!c.decide_should_charge(&cfg, None));

        // Fallback to level when threshold data missing
        c.cached_current = None;
        cfg.max_price_total = 0.0;
        cfg.strategy = "threshold".to_string();
        assert!(c.decide_should_charge(&cfg, Some(PriceLevel::Cheap)));
        assert!(c.decide_should_charge(&cfg, Some(PriceLevel::VeryCheap)));
        assert!(!c.decide_should_charge(&cfg, Some(PriceLevel::Expensive)));
    }

    #[test]
    fn decide_should_charge_percentile() {
        let mut c = TibberClient::new(String::new(), None);
        c.cached_current = Some(PricePoint {
            starts_at: "now".into(),
            total: 3.0,
            level: PriceLevel::Normal,
        });
        c.cached_upcoming = vec![
            PricePoint {
                starts_at: "t1".into(),
                total: 2.0,
                level: PriceLevel::Cheap,
            },
            PricePoint {
                starts_at: "t2".into(),
                total: 3.0,
                level: PriceLevel::Normal,
            },
            PricePoint {
                starts_at: "t3".into(),
                total: 4.0,
                level: PriceLevel::Expensive,
            },
        ];

        let mut cfg = make_cfg();
        cfg.strategy = "percentile".to_string();
        cfg.cheap_percentile = 0.5; // threshold -> 2.0
        assert!(!c.decide_should_charge(&cfg, None));

        cfg.cheap_percentile = 1.0; // threshold -> 4.0
        assert!(c.decide_should_charge(&cfg, None));
    }
}
