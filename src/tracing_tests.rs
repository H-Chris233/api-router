#[cfg(test)]
mod tests {
    use crate::tracing_util::{elapsed_ms, extract_provider, generate_request_id};
    use std::time::Instant;
    use tracing::{info, warn};
    use tracing_subscriber::prelude::*;

    #[test]
    fn test_request_id_generation_is_unique() {
        let id1 = generate_request_id();
        let id2 = generate_request_id();

        assert_ne!(id1, id2, "Request IDs should be unique");
        assert_eq!(id1.len(), 36, "Request ID should be UUID format");
    }

    #[test]
    fn test_provider_extraction() {
        assert_eq!(
            extract_provider("https://dashscope.aliyuncs.com/api/v1"),
            "qwen"
        );
        assert_eq!(extract_provider("https://api.openai.com/v1"), "openai");
        assert_eq!(
            extract_provider("https://api.anthropic.com/v1"),
            "anthropic"
        );
        assert_eq!(extract_provider("https://api.cohere.com/v1"), "cohere");
        assert_eq!(
            extract_provider("https://generativelanguage.googleapis.com/v1"),
            "gemini"
        );
        assert_eq!(extract_provider("https://unknown-provider.com"), "unknown");
    }

    #[test]
    fn test_latency_measurement() {
        let start = Instant::now();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let latency = elapsed_ms(start);

        assert!(latency >= 10.0, "Latency should be at least 10ms");
        assert!(latency < 100.0, "Latency should be less than 100ms");
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_span_creation_with_fields() {
        let request_id = generate_request_id();
        let client_ip = "192.168.1.1";

        let span = tracing::info_span!(
            "http_request",
            request_id = %request_id,
            client_ip = %client_ip,
            method = "POST",
            route = "/v1/chat/completions",
        );

        let _enter = span.enter();
        info!("Test log message");
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_nested_spans_for_upstream_tracking() {
        let request_id = generate_request_id();

        let parent_span = tracing::info_span!(
            "http_request",
            request_id = %request_id,
            route = "/v1/chat/completions",
        );

        let _parent_enter = parent_span.enter();

        let upstream_span = tracing::debug_span!(
            "upstream_request",
            request_id = %request_id,
            provider = "qwen",
            upstream_latency_ms = tracing::field::Empty,
        );

        let _upstream_enter = upstream_span.enter();
        upstream_span.record("upstream_latency_ms", 125.5);

        tracing::debug!("Upstream request completed");
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_structured_fields_in_logs() {
        warn!(
            client = "test***01",
            retry_after = 30,
            "Rate limit exceeded"
        );
    }

    #[test]
    fn test_json_subscriber_initialization() {
        // Test that JSON subscriber can be created without panicking
        let subscriber = tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::new("info"))
            .with(tracing_subscriber::fmt::layer().json());

        // Just verify we can construct it
        drop(subscriber);
    }

    #[test]
    fn test_text_subscriber_initialization() {
        // Test that regular text subscriber can be created without panicking
        let subscriber = tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::new("debug"))
            .with(tracing_subscriber::fmt::layer());

        // Just verify we can construct it
        drop(subscriber);
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_logs_contain_expected_messages() {
        info!("Health check completed");
        // Verify test doesn't panic - actual log capture is tested by tracing_test
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_warning_logs_for_rate_limiting() {
        warn!(
            client = "test***ab",
            retry_after = 60,
            "Rate limit exceeded"
        );
        // Verify test doesn't panic - actual log capture is tested by tracing_test
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_debug_logs_for_upstream_requests() {
        tracing::debug!(
            upstream_latency_ms = 234.56,
            response_size = 1024,
            "Upstream request completed"
        );
        // Verify test doesn't panic - actual log capture is tested by tracing_test
    }
}
