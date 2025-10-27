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
            stream_config: None,
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
        let endpoint = EndpointConfig {
            rate_limit: Some(RateLimitConfig {
                requests_per_minute: Some(10),
                burst: Some(20),
            }),
            ..Default::default()
        };
        config
            .endpoints
            .insert("/v1/test".to_string(), endpoint);
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
        let endpoint = EndpointConfig {
            rate_limit: Some(RateLimitConfig {
                requests_per_minute: Some(12),
                burst: None,
            }),
            ..Default::default()
        };
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

    #[test]
    fn token_bucket_refills_tokens_over_time() {
        use std::time::{Duration, Instant};

        let settings = RateLimitSettings {
            requests_per_minute: 60,
            burst: 2,
        };

        let mut bucket = TokenBucket::new(settings.clone(), Instant::now());
        assert!(bucket.try_consume(Instant::now()).is_ok());
        assert!(bucket.try_consume(Instant::now()).is_ok());
        assert!(bucket.try_consume(Instant::now()).is_err());

        let future_time = Instant::now() + Duration::from_secs(2);
        bucket.refill(future_time);
        assert!(bucket.tokens > 1.0);
    }

    #[test]
    fn token_bucket_caps_at_capacity() {
        use std::time::{Duration, Instant};

        let settings = RateLimitSettings {
            requests_per_minute: 60,
            burst: 5,
        };

        let mut bucket = TokenBucket::new(settings.clone(), Instant::now());

        let way_future = Instant::now() + Duration::from_secs(1000);
        bucket.refill(way_future);

        assert_eq!(bucket.tokens, 5.0);
    }

    #[test]
    fn rate_limit_decision_equality() {
        assert_eq!(RateLimitDecision::Allowed, RateLimitDecision::Allowed);
        assert_eq!(
            RateLimitDecision::Limited {
                retry_after_seconds: 5
            },
            RateLimitDecision::Limited {
                retry_after_seconds: 5
            }
        );
        assert_ne!(
            RateLimitDecision::Allowed,
            RateLimitDecision::Limited {
                retry_after_seconds: 1
            }
        );
    }

    #[test]
    fn rate_limiter_isolates_different_routes() {
        let limiter = RateLimiter::new();
        let settings = RateLimitSettings {
            requests_per_minute: 1,
            burst: 1,
        };

        assert!(matches!(
            limiter.check("/route/a", "client", &settings),
            RateLimitDecision::Allowed
        ));
        assert!(matches!(
            limiter.check("/route/b", "client", &settings),
            RateLimitDecision::Allowed
        ));
    }

    #[test]
    fn rate_limiter_isolates_different_clients() {
        let limiter = RateLimiter::new();
        let settings = RateLimitSettings {
            requests_per_minute: 1,
            burst: 1,
        };

        assert!(matches!(
            limiter.check("/route", "client-a", &settings),
            RateLimitDecision::Allowed
        ));
        assert!(matches!(
            limiter.check("/route", "client-b", &settings),
            RateLimitDecision::Allowed
        ));
    }

    #[test]
    fn rate_limit_settings_equality() {
        let settings1 = RateLimitSettings {
            requests_per_minute: 100,
            burst: 200,
        };
        let settings2 = RateLimitSettings {
            requests_per_minute: 100,
            burst: 200,
        };
        let settings3 = RateLimitSettings {
            requests_per_minute: 50,
            burst: 100,
        };

        assert_eq!(settings1, settings2);
        assert_ne!(settings1, settings3);
    }

    #[test]
    fn resolve_enforces_minimum_burst_of_one() {
        let mut config = base_config();
        config.rate_limit = Some(RateLimitConfig {
            requests_per_minute: Some(10),
            burst: Some(0),
        });

        let settings = resolve_rate_limit_settings("/v1/test", &config).expect("expected settings");
        assert_eq!(settings.burst, 1);
    }

    #[test]
    fn snapshot_handles_empty_limiter() {
        let limiter = RateLimiter::new();
        let snapshot = limiter.snapshot();
        assert_eq!(snapshot.active_buckets, 0);
        assert!(snapshot.routes.is_empty());
    }

    #[test]
    fn limiter_returns_retry_after_when_limited() {
        let limiter = RateLimiter::new();
        let settings = RateLimitSettings {
            requests_per_minute: 1,
            burst: 1,
        };

        let _ = limiter.check("/test", "client", &settings);
        match limiter.check("/test", "client", &settings) {
            RateLimitDecision::Limited {
                retry_after_seconds,
            } => {
                assert!(retry_after_seconds >= 1);
            }
            RateLimitDecision::Allowed => panic!("expected limited"),
        }
    }

    #[test]
    fn resolve_prioritizes_config_over_environment() {
        std::env::set_var("RATE_LIMIT_REQUESTS_PER_MINUTE", "50");
        std::env::set_var("RATE_LIMIT_BURST", "100");

        let mut config = base_config();
        config.rate_limit = Some(RateLimitConfig {
            requests_per_minute: Some(200),
            burst: Some(300),
        });

        let settings = resolve_rate_limit_settings("/v1/test", &config).expect("expected settings");
        assert_eq!(settings.requests_per_minute, 200);
        assert_eq!(settings.burst, 300);

        std::env::remove_var("RATE_LIMIT_REQUESTS_PER_MINUTE");
        std::env::remove_var("RATE_LIMIT_BURST");
    }
}
