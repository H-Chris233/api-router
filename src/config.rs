use serde::Deserialize;
use std::collections::HashMap;

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
