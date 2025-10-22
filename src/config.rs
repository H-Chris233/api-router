use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
pub struct RateLimitConfig {
    #[serde(rename = "requestsPerMinute")]
    pub requests_per_minute: Option<u32>,
    #[serde(default)]
    pub burst: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct EndpointConfig {
    #[serde(rename = "upstreamPath")]
    pub upstream_path: Option<String>,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(rename = "streamSupport", default)]
    pub stream_support: bool,
    #[serde(rename = "requiresMultipart", default)]
    pub requires_multipart: bool,
    #[serde(rename = "rateLimit", default)]
    pub rate_limit: Option<RateLimitConfig>,
}

// 简化的API配置结构
#[derive(Debug, Clone, Deserialize)]
pub struct ApiConfig {
    #[serde(rename = "baseUrl")]
    pub base_url: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(rename = "modelMapping", default)]
    pub model_mapping: Option<HashMap<String, String>>,
    #[serde(default)]
    pub endpoints: HashMap<String, EndpointConfig>,
    #[serde(rename = "port", default = "default_port")]
    pub port: u16,
    #[serde(rename = "rateLimit", default)]
    pub rate_limit: Option<RateLimitConfig>,
}

impl ApiConfig {
    pub fn endpoint(&self, path: &str) -> EndpointConfig {
        self.endpoints.get(path).cloned().unwrap_or_default()
    }
}

// 默认端口函数
pub fn default_port() -> u16 {
    8000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_endpoint_overrides_and_methods() {
        let config: ApiConfig = serde_json::from_str(
            r#"{
                "baseUrl": "https://api.test",
                "endpoints": {
                    "/v1/chat/completions": {
                        "upstreamPath": "/v1/messages",
                        "method": "patch",
                        "headers": {"X-Test": "1"}
                    }
                }
            }"#,
        )
        .unwrap();

        let endpoint = config.endpoint("/v1/chat/completions");
        assert_eq!(endpoint.upstream_path.as_deref(), Some("/v1/messages"));
        assert_eq!(endpoint.method.as_deref(), Some("patch"));
        assert_eq!(endpoint.headers.get("X-Test"), Some(&"1".to_string()));
    }

    #[test]
    fn endpoint_defaults_when_not_configured() {
        let config: ApiConfig = serde_json::from_str(
            r#"{
                "baseUrl": "https://api.test"
            }"#,
        )
        .unwrap();

        let endpoint = config.endpoint("/v1/chat/completions");
        assert!(endpoint.upstream_path.is_none());
        assert!(endpoint.method.is_none());
        assert!(endpoint.headers.is_empty());
        assert!(!endpoint.stream_support);
        assert!(!endpoint.requires_multipart);
    }
}
