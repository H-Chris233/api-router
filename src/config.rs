use std::collections::HashMap;
use serde::{Deserialize, Serialize};

// 简化的API配置结构
#[derive(Debug, Clone, Deserialize)]
pub struct ApiConfig {
    #[serde(rename = "baseUrl")]
    pub base_url: String,
    pub headers: HashMap<String, String>,
    #[serde(rename = "modelMapping", default)]
    pub model_mapping: Option<HashMap<String, String>>,
    #[serde(rename = "port", default = "default_port")]
    pub port: u16,
}

// 默认端口函数
pub fn default_port() -> u16 {
    8000
}