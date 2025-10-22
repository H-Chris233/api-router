use crate::config::ApiConfig;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateLimitSettings {
    pub requests_per_minute: u32,
    pub burst: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RateLimitDecision {
    Allowed,
    Limited { retry_after_seconds: u64 },
}

#[derive(Debug, Clone, Default)]
pub struct RateLimiterSnapshot {
    pub active_buckets: usize,
    pub routes: HashMap<String, usize>,
}

pub struct RateLimiter {
    buckets: DashMap<(String, String), TokenBucket>,
}

#[derive(Debug, Clone)]
struct TokenBucket {
    tokens: f64,
    capacity: f64,
    refill_per_second: f64,
    last_refill: Instant,
    settings: RateLimitSettings,
}

pub static RATE_LIMITER: Lazy<RateLimiter> = Lazy::new(|| RateLimiter::new());

fn env_requests_per_minute() -> Option<u32> {
    std::env::var("RATE_LIMIT_REQUESTS_PER_MINUTE")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
}

fn env_burst() -> Option<u32> {
    std::env::var("RATE_LIMIT_BURST")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
}

pub fn resolve_rate_limit_settings(
    route_path: &str,
    config: &ApiConfig,
) -> Option<RateLimitSettings> {
    let endpoint_config = config.endpoints.get(route_path);

    let requests_per_minute = endpoint_config
        .and_then(|cfg| cfg.rate_limit.as_ref())
        .and_then(|rl| rl.requests_per_minute)
        .or_else(|| {
            config
                .rate_limit
                .as_ref()
                .and_then(|rl| rl.requests_per_minute)
        })
        .or_else(env_requests_per_minute)?;

    if requests_per_minute == 0 {
        return None;
    }

    let burst = endpoint_config
        .and_then(|cfg| cfg.rate_limit.as_ref())
        .and_then(|rl| rl.burst)
        .or_else(|| config.rate_limit.as_ref().and_then(|rl| rl.burst))
        .or_else(env_burst)
        .unwrap_or(requests_per_minute)
        .max(1);

    Some(RateLimitSettings {
        requests_per_minute,
        burst,
    })
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            buckets: DashMap::new(),
        }
    }

    pub fn check(
        &self,
        route: &str,
        api_key: &str,
        settings: &RateLimitSettings,
    ) -> RateLimitDecision {
        let now = Instant::now();
        let key = (route.to_string(), api_key.to_string());
        let mut entry = self
            .buckets
            .entry(key)
            .or_insert_with(|| TokenBucket::new(settings.clone(), now));

        entry.update_settings(settings, now);

        match entry.try_consume(now) {
            Ok(()) => RateLimitDecision::Allowed,
            Err(retry_after_seconds) => RateLimitDecision::Limited {
                retry_after_seconds,
            },
        }
    }

    pub fn snapshot(&self) -> RateLimiterSnapshot {
        let mut routes: HashMap<String, usize> = HashMap::new();
        for entry in self.buckets.iter() {
            let route = &entry.key().0;
            *routes.entry(route.clone()).or_insert(0) += 1;
        }

        RateLimiterSnapshot {
            active_buckets: self.buckets.len(),
            routes,
        }
    }
}

impl TokenBucket {
    fn new(settings: RateLimitSettings, now: Instant) -> Self {
        let capacity = settings.burst as f64;
        let refill_per_second = settings.requests_per_minute as f64 / 60.0;
        Self {
            tokens: capacity,
            capacity,
            refill_per_second,
            last_refill: now,
            settings,
        }
    }

    fn update_settings(&mut self, settings: &RateLimitSettings, now: Instant) {
        if &self.settings != settings {
            self.settings = settings.clone();
            self.capacity = settings.burst as f64;
            self.refill_per_second = settings.requests_per_minute as f64 / 60.0;
            self.tokens = self.capacity;
            self.last_refill = now;
        }
    }

    fn refill(&mut self, now: Instant) {
        if self.tokens >= self.capacity {
            self.last_refill = now;
            return;
        }
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        if elapsed <= 0.0 {
            return;
        }
        self.tokens = (self.tokens + elapsed * self.refill_per_second).min(self.capacity);
        self.last_refill = now;
    }

    fn try_consume(&mut self, now: Instant) -> Result<(), u64> {
        self.refill(now);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            Ok(())
        } else {
            let needed = 1.0 - self.tokens;
            let retry_after = if self.refill_per_second > 0.0 {
                (needed / self.refill_per_second).ceil() as u64
            } else {
                60
            };
            Err(retry_after.max(1))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{EndpointConfig, RateLimitConfig};
    use std::collections::HashMap;

    fn base_config() -> ApiConfig {
        ApiConfig {
            base_url: String::new(),
            headers: HashMap::new(),
            model_mapping: None,
            endpoints: HashMap::new(),
            port: 8000,
            rate_limit: None,
        }
    }

    #[test]
    fn enforces_basic_rate_limit() {
        let limiter = RateLimiter::new();
        let settings = RateLimitSettings {
            requests_per_minute: 2,
            burst: 2,
        };

        assert_eq!(
            limiter.check("/v1/test", "client", &settings),
            RateLimitDecision::Allowed
        );
        assert_eq!(
            limiter.check("/v1/test", "client", &settings),
            RateLimitDecision::Allowed
        );
        match limiter.check("/v1/test", "client", &settings) {
            RateLimitDecision::Limited {
                retry_after_seconds,
            } => assert!(retry_after_seconds >= 1),
            RateLimitDecision::Allowed => panic!("expected limited"),
        }
    }

    #[test]
    fn resets_bucket_when_settings_change() {
        let limiter = RateLimiter::new();
        let strict = RateLimitSettings {
            requests_per_minute: 1,
            burst: 1,
        };
        let relaxed = RateLimitSettings {
            requests_per_minute: 10,
            burst: 10,
        };

        assert!(matches!(
            limiter.check("/v1/test", "client", &strict),
            RateLimitDecision::Allowed
        ));
        assert!(matches!(
            limiter.check("/v1/test", "client", &strict),
            RateLimitDecision::Limited { .. }
        ));
        assert!(matches!(
            limiter.check("/v1/test", "client", &relaxed),
            RateLimitDecision::Allowed
        ));
    }

    #[test]
    fn snapshot_counts_routes() {
        let limiter = RateLimiter::new();
        let settings = RateLimitSettings {
            requests_per_minute: 5,
            burst: 5,
        };
        let _ = limiter.check("/route/a", "client-a", &settings);
        let _ = limiter.check("/route/a", "client-b", &settings);
        let _ = limiter.check("/route/b", "client-a", &settings);

        let snapshot = limiter.snapshot();
        assert_eq!(snapshot.active_buckets, 3);
        assert_eq!(snapshot.routes.get("/route/a"), Some(&2));
        assert_eq!(snapshot.routes.get("/route/b"), Some(&1));
    }

    #[test]
    fn resolve_uses_endpoint_first() {
        let mut config = base_config();
        let mut endpoint = EndpointConfig::default();
        endpoint.rate_limit = Some(RateLimitConfig {
            requests_per_minute: Some(10),
            burst: Some(20),
        });
        config
            .endpoints
            .insert("/v1/test".to_string(), endpoint.clone());
        config.rate_limit = Some(RateLimitConfig {
            requests_per_minute: Some(1),
            burst: Some(2),
        });

        let settings = resolve_rate_limit_settings("/v1/test", &config).expect("expected settings");
        assert_eq!(settings.requests_per_minute, 10);
        assert_eq!(settings.burst, 20);
    }

    #[test]
    fn resolve_defaults_burst_to_requests_per_minute() {
        let mut config = base_config();
        let mut endpoint = EndpointConfig::default();
        endpoint.rate_limit = Some(RateLimitConfig {
            requests_per_minute: Some(12),
            burst: None,
        });
        config.endpoints.insert("/v1/test".to_string(), endpoint);

        let settings = resolve_rate_limit_settings("/v1/test", &config).expect("expected settings");
        assert_eq!(settings.requests_per_minute, 12);
        assert_eq!(settings.burst, 12);
    }

    #[test]
    fn resolve_returns_none_when_missing_config() {
        std::env::remove_var("RATE_LIMIT_REQUESTS_PER_MINUTE");
        std::env::remove_var("RATE_LIMIT_BURST");
        let config = base_config();
        assert!(resolve_rate_limit_settings("/v1/test", &config).is_none());
    }

    #[test]
    fn resolve_uses_environment_defaults() {
        std::env::set_var("RATE_LIMIT_REQUESTS_PER_MINUTE", "6");
        std::env::set_var("RATE_LIMIT_BURST", "3");
        let config = base_config();
        let settings = resolve_rate_limit_settings("/v1/test", &config).expect("expected settings");
        assert_eq!(settings.requests_per_minute, 6);
        assert_eq!(settings.burst, 3);
        std::env::remove_var("RATE_LIMIT_REQUESTS_PER_MINUTE");
        std::env::remove_var("RATE_LIMIT_BURST");
    }

    #[test]
    fn resolve_treats_zero_requests_as_unlimited() {
        let mut config = base_config();
        config.rate_limit = Some(RateLimitConfig {
            requests_per_minute: Some(0),
            burst: Some(10),
        });
        assert!(resolve_rate_limit_settings("/v1/test", &config).is_none());
    }
}
