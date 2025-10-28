use std::time::Instant;
use uuid::Uuid;

/// Generate a unique request ID
pub fn generate_request_id() -> String {
    Uuid::new_v4().to_string()
}

/// Helper to calculate elapsed time in milliseconds
pub fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

/// Extract the provider name from a base URL
pub fn extract_provider(base_url: &str) -> &str {
    if base_url.contains("dashscope.aliyuncs.com") {
        "qwen"
    } else if base_url.contains("openai.com") {
        "openai"
    } else if base_url.contains("anthropic.com") {
        "anthropic"
    } else if base_url.contains("cohere.com") {
        "cohere"
    } else if base_url.contains("generativelanguage.googleapis.com") {
        "gemini"
    } else {
        "unknown"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_request_id() {
        let id1 = generate_request_id();
        let id2 = generate_request_id();
        assert_ne!(id1, id2);
        assert_eq!(id1.len(), 36); // UUID v4 format
    }

    #[test]
    fn test_extract_provider() {
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
        assert_eq!(extract_provider("https://custom-provider.com"), "unknown");
    }

    #[test]
    fn test_elapsed_ms() {
        let start = Instant::now();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let elapsed = elapsed_ms(start);
        assert!(elapsed >= 10.0);
        assert!(elapsed < 100.0); // Should be much less than 100ms
    }
}
