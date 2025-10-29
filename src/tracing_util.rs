//! 追踪工具模块
//!
//! 提供请求追踪和日志记录的辅助函数

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// 全局请求计数器，用于生成唯一的请求 ID
static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(0);

/// 生成唯一的请求 ID
///
/// 请求 ID 由时间戳和递增计数器组成，格式为 32 位十六进制字符串
pub fn generate_request_id() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let counter = REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed) as u32;

    format!(
        "{:016x}{:08x}{:08x}",
        now.as_secs(),
        now.subsec_nanos(),
        counter
    )
}

/// 计算从指定时间开始经过的毫秒数
pub fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

/// 从基础 URL 提取提供商名称
///
/// 根据 URL 中的域名识别 API 提供商
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
        assert_eq!(id1.len(), 32);
        assert!(id1.chars().all(|c| c.is_ascii_hexdigit()));
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
