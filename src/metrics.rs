use lazy_static::lazy_static;
use prometheus::{
    register_histogram_vec, register_int_counter_vec, register_int_gauge, Encoder, HistogramVec,
    IntCounterVec, IntGauge, TextEncoder,
};
use std::time::Instant;

lazy_static! {
    pub static ref REQUESTS_TOTAL: IntCounterVec = register_int_counter_vec!(
        "requests_total",
        "Total number of HTTP requests processed by the router",
        &["method", "route", "status"]
    )
    .expect("failed to register requests_total counter");
    pub static ref UPSTREAM_ERRORS_TOTAL: IntCounterVec = register_int_counter_vec!(
        "upstream_errors_total",
        "Total number of upstream errors returned while forwarding requests",
        &["route"]
    )
    .expect("failed to register upstream_errors_total counter");
    pub static ref REQUEST_LATENCY_SECONDS: HistogramVec = register_histogram_vec!(
        "request_latency_seconds",
        "Histogram of request latencies in seconds",
        &["method", "route"],
        prometheus::exponential_buckets(0.005, 2.0, 12).expect("invalid histogram buckets")
    )
    .expect("failed to register request_latency_seconds histogram");
    pub static ref ACTIVE_CONNECTIONS: IntGauge = register_int_gauge!(
        "active_connections",
        "Number of active TCP connections currently being served"
    )
    .expect("failed to register active_connections gauge");
    pub static ref CACHE_HITS: IntGauge =
        register_int_gauge!("cache_hits", "Total number of configuration cache hits")
            .expect("failed to register cache_hits gauge");
}

pub struct ConnectionGuard;

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        ACTIVE_CONNECTIONS.dec();
    }
}

pub fn track_connection() -> ConnectionGuard {
    ACTIVE_CONNECTIONS.inc();
    ConnectionGuard
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestStatus {
    Success,
    RateLimited,
    BadRequest,
    UpstreamError,
    ConfigError,
    InternalError,
    NotFound,
}

impl RequestStatus {
    fn as_label(self) -> &'static str {
        match self {
            RequestStatus::Success => "ok",
            RequestStatus::RateLimited => "rate_limited",
            RequestStatus::BadRequest => "bad_request",
            RequestStatus::UpstreamError => "upstream_error",
            RequestStatus::ConfigError => "config_error",
            RequestStatus::InternalError => "internal_error",
            RequestStatus::NotFound => "not_found",
        }
    }
}

pub struct RequestMetricsGuard {
    method: String,
    route: String,
    start: Instant,
    status: RequestStatus,
    observed: bool,
}

impl RequestMetricsGuard {
    pub fn new(method: &str, route: &str) -> Self {
        Self {
            method: method.to_string(),
            route: route.to_string(),
            start: Instant::now(),
            status: RequestStatus::Success,
            observed: false,
        }
    }

    pub fn set_status(&mut self, status: RequestStatus) {
        self.status = status;
    }

    fn observe(&mut self) {
        if self.observed {
            return;
        }
        let elapsed = self.start.elapsed().as_secs_f64();
        REQUESTS_TOTAL
            .with_label_values(&[
                self.method.as_str(),
                self.route.as_str(),
                self.status.as_label(),
            ])
            .inc();
        REQUEST_LATENCY_SECONDS
            .with_label_values(&[self.method.as_str(), self.route.as_str()])
            .observe(elapsed);
        self.observed = true;
    }
}

impl Drop for RequestMetricsGuard {
    fn drop(&mut self) {
        self.observe();
    }
}

pub fn record_upstream_error(route: &str) {
    UPSTREAM_ERRORS_TOTAL.with_label_values(&[route]).inc();
}

pub fn record_cache_hit() {
    CACHE_HITS.inc();
}

pub fn render() -> Result<Vec<u8>, prometheus::Error> {
    let metric_families = prometheus::gather();
    let mut buffer = Vec::with_capacity(1024);
    TextEncoder::new().encode(&metric_families, &mut buffer)?;
    Ok(buffer)
}

#[cfg(test)]
pub fn reset_for_tests() {
    REQUESTS_TOTAL.reset();
    REQUEST_LATENCY_SECONDS.reset();
    UPSTREAM_ERRORS_TOTAL.reset();
    ACTIVE_CONNECTIONS.set(0);
    CACHE_HITS.set(0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn registers_expected_metrics() {
        reset_for_tests();

        REQUESTS_TOTAL
            .with_label_values(&["GET", "/health", "ok"])
            .inc();
        REQUEST_LATENCY_SECONDS
            .with_label_values(&["GET", "/health"])
            .observe(0.01);
        UPSTREAM_ERRORS_TOTAL
            .with_label_values(&["/health"])
            .inc();
        ACTIVE_CONNECTIONS.set(1);
        CACHE_HITS.set(1);

        let families = prometheus::gather();
        assert!(families.iter().any(|f| f.get_name() == "requests_total"));
        assert!(families
            .iter()
            .any(|f| f.get_name() == "upstream_errors_total"));
        assert!(families
            .iter()
            .any(|f| f.get_name() == "request_latency_seconds"));
        assert!(families
            .iter()
            .any(|f| f.get_name() == "active_connections"));
        assert!(families.iter().any(|f| f.get_name() == "cache_hits"));

        reset_for_tests();
    }

    #[test]
    #[serial]
    fn renders_sample_output() {
        reset_for_tests();

        {
            let mut tracker = RequestMetricsGuard::new("GET", "/health");
            tracker.set_status(RequestStatus::Success);
        }
        record_cache_hit();

        let encoded = render().expect("metrics should render");
        let text = String::from_utf8(encoded).expect("metrics output is utf8");
        assert!(text.contains("# HELP requests_total"));
        assert!(text.contains("requests_total{method=\"GET\",route=\"/health\",status=\"ok\"} 1"));
        assert!(text.contains("cache_hits"));
    }
}
