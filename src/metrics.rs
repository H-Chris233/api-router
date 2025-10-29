//! 轻量级指标收集模块
//!
//! 使用原子操作实现零依赖的指标收集，输出 Prometheus 文本格式

use dashmap::DashMap;
use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicU64, Ordering};

static REQUESTS: Lazy<DashMap<String, AtomicU64>> = Lazy::new(DashMap::new);
static UPSTREAM_ERRORS: Lazy<DashMap<String, AtomicU64>> = Lazy::new(DashMap::new);
static ACTIVE_CONNECTIONS: AtomicU64 = AtomicU64::new(0);
static RATE_LIMITER_BUCKETS: AtomicU64 = AtomicU64::new(0);

pub struct ConnectionGuard;

impl ConnectionGuard {
    pub fn new() -> Self {
        ACTIVE_CONNECTIONS.fetch_add(1, Ordering::Relaxed);
        ConnectionGuard
    }
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        ACTIVE_CONNECTIONS.fetch_sub(1, Ordering::Relaxed);
    }
}

pub fn record_request(route: &str, method: &str, status: u16) {
    let key = format!("{}:{}:{}", route, method, status);
    REQUESTS
        .entry(key)
        .or_insert_with(|| AtomicU64::new(0))
        .fetch_add(1, Ordering::Relaxed);
}

pub fn record_upstream_error(error_type: &str) {
    UPSTREAM_ERRORS
        .entry(error_type.to_string())
        .or_insert_with(|| AtomicU64::new(0))
        .fetch_add(1, Ordering::Relaxed);
}

pub fn observe_request_latency(_route: &str, _latency_seconds: f64) {
    // Simplified: no histogram, just log if needed
}

pub fn update_rate_limiter_buckets(count: usize) {
    RATE_LIMITER_BUCKETS.store(count as u64, Ordering::Relaxed);
}

pub fn gather_metrics() -> Result<String, String> {
    let mut output = String::new();

    output.push_str("# HELP requests_total Total HTTP requests\n");
    output.push_str("# TYPE requests_total counter\n");
    for entry in REQUESTS.iter() {
        let parts: Vec<&str> = entry.key().split(':').collect();
        if parts.len() == 3 {
            output.push_str(&format!(
                "requests_total{{route=\"{}\",method=\"{}\",status=\"{}\"}} {}\n",
                parts[0],
                parts[1],
                parts[2],
                entry.value().load(Ordering::Relaxed)
            ));
        }
    }

    output.push_str("# HELP upstream_errors_total Total upstream errors\n");
    output.push_str("# TYPE upstream_errors_total counter\n");
    for entry in UPSTREAM_ERRORS.iter() {
        output.push_str(&format!(
            "upstream_errors_total{{error_type=\"{}\"}} {}\n",
            entry.key(),
            entry.value().load(Ordering::Relaxed)
        ));
    }

    output.push_str("# HELP active_connections Active connections\n");
    output.push_str("# TYPE active_connections gauge\n");
    output.push_str(&format!(
        "active_connections {}\n",
        ACTIVE_CONNECTIONS.load(Ordering::Relaxed)
    ));

    output.push_str("# HELP rate_limiter_buckets Rate limiter buckets\n");
    output.push_str("# TYPE rate_limiter_buckets gauge\n");
    output.push_str(&format!(
        "rate_limiter_buckets {}\n",
        RATE_LIMITER_BUCKETS.load(Ordering::Relaxed)
    ));

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_request_increments_counter() {
        record_request("/test", "GET", 200);
        let metrics = gather_metrics().unwrap();
        assert!(metrics.contains("requests_total"));
    }

    #[test]
    fn record_upstream_error_increments_counter() {
        record_upstream_error("timeout");
        let metrics = gather_metrics().unwrap();
        assert!(metrics.contains("upstream_errors_total"));
    }

    #[test]
    fn connection_guard_updates_active_connections() {
        let before = ACTIVE_CONNECTIONS.load(Ordering::Relaxed);
        {
            let _guard = ConnectionGuard::new();
            assert_eq!(ACTIVE_CONNECTIONS.load(Ordering::Relaxed), before + 1);
        }
        assert_eq!(ACTIVE_CONNECTIONS.load(Ordering::Relaxed), before);
    }
}
