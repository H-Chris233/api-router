//! 速率限制模块
//! 
//! 实现基于令牌桶算法的速率限制器，支持按 API Key 和路由的细粒度限流

use crate::config::ApiConfig;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::time::Instant;

/// 速率限制配置
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateLimitSettings {
    /// 每分钟允许的请求数
    pub requests_per_minute: u32,
    /// 突发容量
    pub burst: u32,
}

/// 速率限制决策结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RateLimitDecision {
    /// 允许通过
    Allowed,
    /// 已限流，需要等待指定秒数后重试
    Limited { retry_after_seconds: u64 },
}

/// 速率限制器快照，用于监控和调试
#[derive(Debug, Clone, Default)]
pub struct RateLimiterSnapshot {
    /// 活跃的令牌桶数量
    pub active_buckets: usize,
    /// 按路由分组的令牌桶数量
    pub routes: HashMap<String, usize>,
}

/// 速率限制器
/// 
/// 使用 DashMap 实现并发安全的令牌桶存储
/// 键为 (route, api_key) 元组，每个键对应一个独立的令牌桶
pub struct RateLimiter {
    buckets: DashMap<(String, String), TokenBucket>,
}

/// 令牌桶结构
/// 
/// 实现令牌桶算法，支持令牌的自动补充和消费
#[derive(Debug, Clone)]
struct TokenBucket {
    /// 当前令牌数
    tokens: f64,
    /// 桶容量（最大令牌数）
    capacity: f64,
    /// 每秒补充的令牌数
    refill_per_second: f64,
    /// 上次补充时间
    last_refill: Instant,
    /// 速率限制配置
    settings: RateLimitSettings,
}

/// 全局速率限制器单例
pub static RATE_LIMITER: Lazy<RateLimiter> = Lazy::new(|| RateLimiter::new());

/// 从环境变量读取每分钟请求数限制
fn env_requests_per_minute() -> Option<u32> {
    std::env::var("RATE_LIMIT_REQUESTS_PER_MINUTE")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
}

/// 从环境变量读取突发容量限制
fn env_burst() -> Option<u32> {
    std::env::var("RATE_LIMIT_BURST")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
}

/// 解析速率限制配置
/// 
/// 优先级：端点配置 > 全局配置 > 环境变量
/// 如果 requests_per_minute 为 0，则返回 None 表示不限流
pub fn resolve_rate_limit_settings(
    route_path: &str,
    config: &ApiConfig,
) -> Option<RateLimitSettings> {
    let endpoint_config = config.endpoints.get(route_path);

    // 解析每分钟请求数限制
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

    // 如果设置为 0，表示不限流
    if requests_per_minute == 0 {
        return None;
    }

    // 解析突发容量，默认等于 requests_per_minute
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
    /// 创建新的速率限制器实例
    pub fn new() -> Self {
        Self {
            buckets: DashMap::new(),
        }
    }

    /// 检查请求是否在速率限制内
    /// 
    /// # 参数
    /// - `route`: 请求路由路径
    /// - `api_key`: API 密钥
    /// - `settings`: 速率限制配置
    /// 
    /// # 返回
    /// - `RateLimitDecision::Allowed`: 请求被允许
    /// - `RateLimitDecision::Limited`: 请求被限流，包含重试等待时间
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

    #[test]
    fn token_bucket_refills_over_time() {
        let limiter = RateLimiter::new();
        let settings = RateLimitSettings {
            requests_per_minute: 60,
            burst: 2,
        };

        assert!(matches!(
            limiter.check("/v1/test", "client", &settings),
            RateLimitDecision::Allowed
        ));
        assert!(matches!(
            limiter.check("/v1/test", "client", &settings),
            RateLimitDecision::Allowed
        ));
        assert!(matches!(
            limiter.check("/v1/test", "client", &settings),
            RateLimitDecision::Limited { .. }
        ));
    }

    #[test]
    fn rate_limiter_isolates_routes() {
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
    fn rate_limiter_isolates_api_keys() {
        let limiter = RateLimiter::new();
        let settings = RateLimitSettings {
            requests_per_minute: 1,
            burst: 1,
        };

        assert!(matches!(
            limiter.check("/v1/test", "client-a", &settings),
            RateLimitDecision::Allowed
        ));
        assert!(matches!(
            limiter.check("/v1/test", "client-b", &settings),
            RateLimitDecision::Allowed
        ));
    }

    #[test]
    fn rate_limit_decision_equality() {
        assert_eq!(RateLimitDecision::Allowed, RateLimitDecision::Allowed);
        assert_eq!(
            RateLimitDecision::Limited {
                retry_after_seconds: 10
            },
            RateLimitDecision::Limited {
                retry_after_seconds: 10
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
    fn rate_limit_settings_equality() {
        let s1 = RateLimitSettings {
            requests_per_minute: 60,
            burst: 10,
        };
        let s2 = RateLimitSettings {
            requests_per_minute: 60,
            burst: 10,
        };
        let s3 = RateLimitSettings {
            requests_per_minute: 30,
            burst: 10,
        };
        assert_eq!(s1, s2);
        assert_ne!(s1, s3);
    }

    #[test]
    fn rate_limit_settings_clone() {
        let s1 = RateLimitSettings {
            requests_per_minute: 100,
            burst: 20,
        };
        let s2 = s1.clone();
        assert_eq!(s1, s2);
    }

    #[test]
    fn rate_limiter_snapshot_default() {
        let snapshot = RateLimiterSnapshot::default();
        assert_eq!(snapshot.active_buckets, 0);
        assert_eq!(snapshot.routes.len(), 0);
    }

    #[test]
    fn resolve_burst_minimum_is_one() {
        let mut config = base_config();
        config.rate_limit = Some(RateLimitConfig {
            requests_per_minute: Some(100),
            burst: Some(0),
        });
        let settings = resolve_rate_limit_settings("/v1/test", &config).unwrap();
        assert_eq!(settings.burst, 1);
    }

    #[test]
    fn resolve_endpoint_burst_overrides_global() {
        let mut config = base_config();
        let mut endpoint = EndpointConfig::default();
        endpoint.rate_limit = Some(RateLimitConfig {
            requests_per_minute: Some(10),
            burst: Some(5),
        });
        config.endpoints.insert("/v1/test".to_string(), endpoint);
        config.rate_limit = Some(RateLimitConfig {
            requests_per_minute: Some(100),
            burst: Some(50),
        });
        let settings = resolve_rate_limit_settings("/v1/test", &config).unwrap();
        assert_eq!(settings.requests_per_minute, 10);
        assert_eq!(settings.burst, 5);
    }

    #[test]
    fn token_bucket_debug_format() {
        let settings = RateLimitSettings {
            requests_per_minute: 60,
            burst: 10,
        };
        let debug_str = format!("{:?}", settings);
        assert!(debug_str.contains("requests_per_minute"));
        assert!(debug_str.contains("60"));
    }
}
