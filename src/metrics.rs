use lazy_static::lazy_static;
use prometheus::{
    register_counter_vec, register_gauge, register_histogram_vec, CounterVec, Encoder, Gauge,
    HistogramVec, TextEncoder,
};
use std::sync::atomic::{AtomicU64, Ordering};

lazy_static! {
    pub static ref REQUESTS_TOTAL: CounterVec = register_counter_vec!(
        "requests_total",
        "Total number of HTTP requests by route, method, and status",
        &["route", "method", "status"]
    )
    .expect("failed to register requests_total metric");
    pub static ref UPSTREAM_ERRORS_TOTAL: CounterVec = register_counter_vec!(
        "upstream_errors_total",
        "Total number of upstream errors by error type",
        &["error_type"]
    )
    .expect("failed to register upstream_errors_total metric");
    pub static ref REQUEST_LATENCY_SECONDS: HistogramVec = register_histogram_vec!(
        "request_latency_seconds",
        "Request latency in seconds by route",
        &["route"],
        vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
    )
    .expect("failed to register request_latency_seconds metric");
    pub static ref ACTIVE_CONNECTIONS: Gauge = register_gauge!(
        "active_connections",
        "Number of currently active connections"
    )
    .expect("failed to register active_connections metric");
    pub static ref RATE_LIMITER_BUCKETS: Gauge = register_gauge!(
        "rate_limiter_buckets",
        "Number of active rate limiter buckets"
    )
    .expect("failed to register rate_limiter_buckets metric");
}

static ACTIVE_CONNECTIONS_COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct ConnectionGuard;

impl ConnectionGuard {
    pub fn new() -> Self {
        let count = ACTIVE_CONNECTIONS_COUNTER.fetch_add(1, Ordering::SeqCst) + 1;
        ACTIVE_CONNECTIONS.set(count as f64);
        ConnectionGuard
    }
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        let count = ACTIVE_CONNECTIONS_COUNTER.fetch_sub(1, Ordering::SeqCst) - 1;
        ACTIVE_CONNECTIONS.set(count as f64);
    }
}

pub fn record_request(route: &str, method: &str, status: u16) {
    REQUESTS_TOTAL
        .with_label_values(&[route, method, &status.to_string()])
        .inc();
}

pub fn record_upstream_error(error_type: &str) {
    UPSTREAM_ERRORS_TOTAL.with_label_values(&[error_type]).inc();
}

pub fn observe_request_latency(route: &str, latency_seconds: f64) {
    REQUEST_LATENCY_SECONDS
        .with_label_values(&[route])
        .observe(latency_seconds);
}

pub fn update_rate_limiter_buckets(count: usize) {
    RATE_LIMITER_BUCKETS.set(count as f64);
}

pub fn gather_metrics() -> Result<String, String> {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    encoder
        .encode(&metric_families, &mut buffer)
        .map_err(|e| format!("failed to encode metrics: {}", e))?;
    String::from_utf8(buffer).map_err(|e| format!("failed to convert metrics to string: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_are_registered() {
        record_request("/test", "GET", 200);
        record_upstream_error("test_error");
        observe_request_latency("/test", 0.1);
        update_rate_limiter_buckets(5);

        let metrics_output = gather_metrics().expect("should gather metrics");

        assert!(metrics_output.contains("requests_total"));
        assert!(metrics_output.contains("upstream_errors_total"));
        assert!(metrics_output.contains("request_latency_seconds"));
        assert!(metrics_output.contains("active_connections"));
        assert!(metrics_output.contains("rate_limiter_buckets"));
    }

    #[test]
    fn record_request_increments_counter() {
        let before = gather_metrics().expect("should gather metrics");

        record_request("/test", "GET", 200);

        let after = gather_metrics().expect("should gather metrics");
        assert!(after.contains("requests_total"));
        assert_ne!(before, after);
    }

    #[test]
    fn record_upstream_error_increments_counter() {
        let before = gather_metrics().expect("should gather metrics");

        record_upstream_error("connection_timeout");

        let after = gather_metrics().expect("should gather metrics");
        assert!(after.contains("upstream_errors_total"));
        assert_ne!(before, after);
    }

    #[test]
    fn observe_request_latency_records_histogram() {
        let before = gather_metrics().expect("should gather metrics");

        observe_request_latency("/v1/chat/completions", 0.123);

        let after = gather_metrics().expect("should gather metrics");
        assert!(after.contains("request_latency_seconds"));
        assert_ne!(before, after);
    }

    #[test]
    fn connection_guard_updates_active_connections() {
        let initial = ACTIVE_CONNECTIONS_COUNTER.load(Ordering::SeqCst);

        {
            let _guard1 = ConnectionGuard::new();
            assert_eq!(
                ACTIVE_CONNECTIONS_COUNTER.load(Ordering::SeqCst),
                initial + 1
            );

            {
                let _guard2 = ConnectionGuard::new();
                assert_eq!(
                    ACTIVE_CONNECTIONS_COUNTER.load(Ordering::SeqCst),
                    initial + 2
                );
            }

            assert_eq!(
                ACTIVE_CONNECTIONS_COUNTER.load(Ordering::SeqCst),
                initial + 1
            );
        }

        assert_eq!(ACTIVE_CONNECTIONS_COUNTER.load(Ordering::SeqCst), initial);
    }

    #[test]
    fn update_rate_limiter_buckets_sets_gauge() {
        update_rate_limiter_buckets(42);

        let metrics = gather_metrics().expect("should gather metrics");
        assert!(metrics.contains("rate_limiter_buckets"));
        assert!(metrics.contains("42"));
    }
}
