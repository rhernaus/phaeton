use crate::error::Result;

#[cfg(feature = "tibber")]
use crate::logging::get_logger;
#[cfg(feature = "tibber")]
use crate::tibber::runtime_helper_time;
#[cfg(feature = "tibber")]
use crate::tibber::types::{PriceLevel, PricePoint};

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
    pub fn upcoming_prices(&self) -> &[PricePoint] {
        &self.cached_upcoming
    }

    /// Compute a percentile threshold over upcoming prices
    #[cfg(feature = "tibber")]
    pub fn determine_percentile_threshold(&self, percentile: f64) -> Option<f64> {
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
            }
            "percentile" => {
                if let (Some(total), Some(thr)) = (
                    current_total,
                    self.determine_percentile_threshold(cfg.cheap_percentile),
                ) {
                    return total <= thr;
                }
            }
            _ => {}
        }

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

    /// Refresh cached data from Tibber API if needed; returns current level if available
    #[cfg(feature = "tibber")]
    pub async fn refresh_if_due(&mut self) -> Result<Option<PriceLevel>> {
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
            let cur_level = PriceLevel::from_label(
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
                        let level = PriceLevel::from_label(
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
            // Sort by actual epoch time to handle differing timezone offsets/DST correctly
            upcoming.sort_by(|a, b| {
                let ta = chrono::DateTime::parse_from_rfc3339(&a.starts_at)
                    .ok()
                    .map(|dt| dt.timestamp());
                let tb = chrono::DateTime::parse_from_rfc3339(&b.starts_at)
                    .ok()
                    .map(|dt| dt.timestamp());
                match (ta, tb) {
                    (Some(x), Some(y)) => x.cmp(&y),
                    // Fallback to lexicographic order if parsing fails for either side
                    _ => a.starts_at.cmp(&b.starts_at),
                }
            });
            self.cached_upcoming = upcoming;

            let mut next_refresh = 0.0;
            let parse_ts = |s: &str| -> Option<f64> {
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
                    next_refresh = cur_ts + 3600.0;
                }
            }
            if next_refresh == 0.0 {
                next_refresh = now + 900.0;
            }
            self.cache_next_refresh_epoch = next_refresh + 1.0;
            Ok(self.cached_current.as_ref().map(|p| p.level))
        }
    }
}

impl TibberClient {
    /// Human-readable hourly overview helper
    #[cfg(feature = "tibber")]
    pub async fn get_hourly_overview(&self) -> Result<String> {
        let cfg = crate::config::TibberConfig {
            access_token: self.access_token.clone(),
            home_id: self.home_id.clone().unwrap_or_default(),
            charge_on_cheap: true,
            charge_on_very_cheap: true,
            strategy: "level".to_string(),
            max_price_total: 0.0,
            cheap_percentile: 0.3,
        };
        crate::tibber::api::get_hourly_overview_text(&cfg).await
    }

    /// Human-readable hourly overview when tibber feature is disabled
    #[cfg(not(feature = "tibber"))]
    pub async fn get_hourly_overview(&self) -> Result<String> {
        Ok("Tibber integration not yet implemented".to_string())
    }

    /// Legacy stub for compatibility with existing tests
    pub async fn should_charge(&self, _strategy: &str) -> Result<bool> {
        Ok(true)
    }
}
