//! 轻量级错误追踪模块
//!
//! 使用 tracing 记录错误，无需外部依赖

use crate::errors::RouterError;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tracing::{error, warn};

const UPSTREAM_FAILURE_THRESHOLD: u64 = 5;
const UPSTREAM_FAILURE_WINDOW_SECS: u64 = 300;

static UPSTREAM_FAILURE_TRACKER: Lazy<DashMap<String, UpstreamFailureInfo>> =
    Lazy::new(DashMap::new);

#[derive(Debug)]
struct UpstreamFailureInfo {
    count: AtomicU64,
    first_failure: Instant,
    last_alerted: Option<Instant>,
}

pub fn capture_error_with_context(
    error: &RouterError,
    request_id: &str,
    client_api_key: &str,
    route: &str,
    provider: Option<&str>,
) {
    let api_key_prefix = if client_api_key.len() > 8 {
        &client_api_key[..8]
    } else {
        "***"
    };

    error!(
        request_id = %request_id,
        route = %route,
        api_key_prefix = %api_key_prefix,
        provider = ?provider,
        error = %error,
        "Request error"
    );
}

pub fn track_upstream_failure(provider: &str, error: &RouterError) {
    let key = provider.to_string();

    let should_alert = UPSTREAM_FAILURE_TRACKER
        .entry(key.clone())
        .or_insert_with(|| UpstreamFailureInfo {
            count: AtomicU64::new(0),
            first_failure: Instant::now(),
            last_alerted: None,
        })
        .value_mut()
        .register_failure();

    if should_alert {
        error!(
            provider = %provider,
            error = %error,
            threshold = UPSTREAM_FAILURE_THRESHOLD,
            window_secs = UPSTREAM_FAILURE_WINDOW_SECS,
            "ALERT: Repeated upstream failures detected"
        );
    }

    cleanup_old_failure_trackers();
}

impl UpstreamFailureInfo {
    fn register_failure(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.first_failure);

        if elapsed > Duration::from_secs(UPSTREAM_FAILURE_WINDOW_SECS) {
            self.count.store(1, Ordering::SeqCst);
            self.first_failure = now;
            self.last_alerted = None;
            return false;
        }

        let count = self.count.fetch_add(1, Ordering::SeqCst) + 1;

        if count >= UPSTREAM_FAILURE_THRESHOLD {
            let should_alert = match self.last_alerted {
                None => true,
                Some(last) => now.duration_since(last) > Duration::from_secs(60),
            };

            if should_alert {
                self.last_alerted = Some(now);
                return true;
            }
        }

        false
    }
}

fn cleanup_old_failure_trackers() {
    let cutoff = Instant::now() - Duration::from_secs(UPSTREAM_FAILURE_WINDOW_SECS * 2);
    UPSTREAM_FAILURE_TRACKER.retain(|_, info| info.first_failure > cutoff);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_failure_info_resets_after_window() {
        let mut info = UpstreamFailureInfo {
            count: AtomicU64::new(10),
            first_failure: Instant::now() - Duration::from_secs(UPSTREAM_FAILURE_WINDOW_SECS + 1),
            last_alerted: None,
        };

        let should_alert = info.register_failure();
        assert!(!should_alert);
        assert_eq!(info.count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn upstream_failure_info_alerts_at_threshold() {
        let mut info = UpstreamFailureInfo {
            count: AtomicU64::new(UPSTREAM_FAILURE_THRESHOLD - 1),
            first_failure: Instant::now(),
            last_alerted: None,
        };

        let should_alert = info.register_failure();
        assert!(should_alert);
    }

    #[test]
    fn upstream_failure_info_throttles_alerts() {
        let mut info = UpstreamFailureInfo {
            count: AtomicU64::new(UPSTREAM_FAILURE_THRESHOLD),
            first_failure: Instant::now(),
            last_alerted: Some(Instant::now()),
        };

        let should_alert = info.register_failure();
        assert!(!should_alert);
    }
}
