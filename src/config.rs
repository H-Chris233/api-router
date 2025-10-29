//! 配置管理模块
//! 
//! 提供 API Router 的配置加载、缓存和热重载功能。
//! 支持从 JSON 文件加载配置，并自动检测文件变更进行热重载。

use crate::errors::{RouterError, RouterResult};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::SystemTime;
use tracing::{debug, warn};

/// 默认配置文件路径（当主配置不存在时使用）
const FALLBACK_CONFIG_PATH: &str = "./transformer/qwen.json";
/// 配置文件读取缓冲区大小（128 KB）
const CONFIG_BUFFER_SIZE: usize = 128 * 1024;

/// 配置缓存结构
#[derive(Default)]
struct ConfigCache {
    entry: Option<CachedConfig>,
}

/// 缓存的配置条目
struct CachedConfig {
    /// 配置内容（使用 Arc 进行引用计数共享）
    config: Arc<ApiConfig>,
    /// 配置文件路径
    source: PathBuf,
    /// 配置文件最后修改时间
    modified: Option<SystemTime>,
}

/// 配置文件路径集合
#[derive(Clone)]
struct ConfigPaths {
    /// 主配置文件路径
    primary: PathBuf,
    /// 回退配置文件路径
    fallback: PathBuf,
}

/// 全局配置缓存单例
static CONFIG_CACHE: OnceLock<RwLock<ConfigCache>> = OnceLock::new();

/// 速率限制配置
#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
pub struct RateLimitConfig {
    /// 每分钟允许的最大请求数
    #[serde(rename = "requestsPerMinute")]
    pub requests_per_minute: Option<u32>,
    /// 允许的突发请求数
    #[serde(default)]
    pub burst: Option<u32>,
}

/// 流式传输配置
#[derive(Debug, Clone, Deserialize, Default)]
pub struct StreamConfig {
    /// 缓冲区大小（字节），默认 8192
    #[serde(rename = "bufferSize", default = "default_buffer_size")]
    pub buffer_size: usize,
    /// 心跳间隔（秒），默认 30 秒
    #[serde(
        rename = "heartbeatIntervalSecs",
        default = "default_heartbeat_interval"
    )]
    pub heartbeat_interval_secs: u64,
}

/// 返回默认缓冲区大小
fn default_buffer_size() -> usize {
    8192
}

/// 返回默认心跳间隔
fn default_heartbeat_interval() -> u64 {
    30
}

/// 端点级别的配置
#[derive(Debug, Clone, Deserialize, Default)]
pub struct EndpointConfig {
    /// 上游路径（可选，用于路径重写）
    #[serde(rename = "upstreamPath")]
    pub upstream_path: Option<String>,
    /// HTTP 方法（可选，用于方法覆写）
    #[serde(default)]
    pub method: Option<String>,
    /// 端点特定的请求头
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// 是否支持流式传输
    #[serde(rename = "streamSupport", default)]
    pub stream_support: bool,
    /// 是否需要 multipart 格式（用于文件上传）
    #[serde(rename = "requiresMultipart", default)]
    pub requires_multipart: bool,
    /// 端点级别的速率限制配置
    #[serde(rename = "rateLimit", default)]
    pub rate_limit: Option<RateLimitConfig>,
    /// 端点级别的流式传输配置
    #[serde(rename = "streamConfig", default)]
    pub stream_config: Option<StreamConfig>,
}

/// API 配置主结构
#[derive(Debug, Clone, Deserialize)]
pub struct ApiConfig {
    /// 上游 API 的基础 URL
    #[serde(rename = "baseUrl")]
    pub base_url: String,
    /// 全局请求头
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// 模型名称映射（客户端模型名 -> 上游模型名）
    #[serde(rename = "modelMapping", default)]
    pub model_mapping: Option<HashMap<String, String>>,
    /// 端点配置映射
    #[serde(default)]
    pub endpoints: HashMap<String, EndpointConfig>,
    /// 监听端口，默认 8000
    #[serde(rename = "port", default = "default_port")]
    pub port: u16,
    /// 全局速率限制配置
    #[serde(rename = "rateLimit", default)]
    pub rate_limit: Option<RateLimitConfig>,
    /// 全局流式传输配置
    #[serde(rename = "streamConfig", default)]
    pub stream_config: Option<StreamConfig>,
}

impl ApiConfig {
    /// 获取指定路径的端点配置，如果不存在则返回默认配置
    pub fn endpoint(&self, path: &str) -> EndpointConfig {
        self.endpoints.get(path).cloned().unwrap_or_default()
    }
}

/// 返回默认端口号
pub fn default_port() -> u16 {
    8000
}

/// 获取或初始化全局配置缓存
fn cache_cell() -> &'static RwLock<ConfigCache> {
    CONFIG_CACHE.get_or_init(|| RwLock::new(ConfigCache::default()))
}

/// 解析配置文件路径
/// 
/// 优先级：
/// 1. 环境变量 API_ROUTER_CONFIG_PATH 指定的路径
/// 2. 命令行参数指定的配置名称（在 transformer/ 目录下查找）
/// 3. 默认配置：transformer/qwen.json
fn resolve_config_paths() -> ConfigPaths {
    if let Ok(explicit_path) = std::env::var("API_ROUTER_CONFIG_PATH") {
        let primary = PathBuf::from(explicit_path);
        return ConfigPaths {
            primary,
            fallback: PathBuf::from(FALLBACK_CONFIG_PATH),
        };
    }

    let args: Vec<String> = std::env::args().collect();
    let config_basename = if args.len() > 1 {
        args[1].clone()
    } else {
        "qwen".to_string()
    };

    let primary = PathBuf::from(format!("./transformer/{}.json", config_basename));
    ConfigPaths {
        primary,
        fallback: PathBuf::from(FALLBACK_CONFIG_PATH),
    }
}

/// 检查配置是否需要重新加载
/// 
/// 通过比较文件的最后修改时间来判断是否需要重新加载配置
fn needs_reload(entry: &CachedConfig, paths: &ConfigPaths) -> bool {
    match fs::metadata(&paths.primary) {
        Ok(meta) => {
            let modified = meta.modified().ok();
            if entry.source == paths.primary {
                entry.modified != modified
            } else {
                true
            }
        }
        Err(_) => {
            if entry.source == paths.primary {
                return true;
            }
            match fs::metadata(&paths.fallback) {
                Ok(meta) => {
                    let modified = meta.modified().ok();
                    entry.modified != modified
                }
                Err(_) => true,
            }
        }
    }
}

/// 从指定路径读取配置文件
/// 
/// 返回配置内容和文件的最后修改时间
fn read_config_from_path(path: &Path) -> RouterResult<(ApiConfig, Option<SystemTime>)> {
    let file = File::open(path).map_err(|e| {
        RouterError::ConfigRead(format!("无法打开配置文件 {}: {}", path.display(), e))
    })?;
    let modified = file.metadata().ok().and_then(|meta| meta.modified().ok());
    let mut reader = BufReader::with_capacity(CONFIG_BUFFER_SIZE, file);
    let config: ApiConfig = serde_json::from_reader(&mut reader)
        .map_err(|e| RouterError::ConfigParse(format!("{}: {}", path.display(), e)))?;
    Ok((config, modified))
}

/// 使用指定的路径集合加载配置
/// 
/// 尝试加载主配置，如果失败则尝试回退配置
fn load_config_with_paths(paths: &ConfigPaths) -> RouterResult<CachedConfig> {
    match read_config_from_path(&paths.primary) {
        Ok((config, modified)) => {
            debug!("从 {} 加载 API 配置", paths.primary.display());
            Ok(CachedConfig {
                config: Arc::new(config),
                source: paths.primary.clone(),
                modified,
            })
        }
        Err(err) => match err {
            RouterError::ConfigParse(_) => Err(err),
            RouterError::ConfigRead(msg) => {
                warn!("{}; 回退到 {}", msg, paths.fallback.display());
                let (config, modified) = read_config_from_path(&paths.fallback)?;
                Ok(CachedConfig {
                    config: Arc::new(config),
                    source: paths.fallback.clone(),
                    modified,
                })
            }
            _ => Err(err),
        },
    }
}

/// 加载 API 配置
/// 
/// 该函数会：
/// 1. 首先尝试从缓存中获取配置
/// 2. 检查配置文件是否有更新
/// 3. 如果需要，重新加载配置文件
/// 
/// 配置文件支持热重载，修改配置文件后会自动重新加载
pub fn load_api_config() -> RouterResult<Arc<ApiConfig>> {
    let paths = resolve_config_paths();
    let cache = cache_cell();

    // 尝试使用读锁快速返回缓存的配置
    {
        let guard = cache.read().expect("配置缓存损坏");
        if let Some(entry) = &guard.entry {
            if !needs_reload(entry, &paths) {
                debug!("使用缓存的 API 配置，来自 {}", entry.source.display());
                return Ok(entry.config.clone());
            }
        }
    }

    // 获取写锁并重新加载配置
    let mut guard = cache.write().expect("配置缓存损坏");
    // 双重检查：其他线程可能已经更新了配置
    if let Some(entry) = &guard.entry {
        if !needs_reload(entry, &paths) {
            debug!("使用缓存的 API 配置，来自 {}", entry.source.display());
            return Ok(entry.config.clone());
        }
    }

    let new_entry = load_config_with_paths(&paths)?;
    let config = new_entry.config.clone();
    guard.entry = Some(new_entry);
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use std::path::Path;
    use std::sync::Arc;
    use std::thread::sleep;
    use std::time::{Duration, SystemTime};

    fn reset_cache() {
        if let Some(cache) = CONFIG_CACHE.get() {
            let mut guard = cache.write().unwrap();
            guard.entry = None;
        }
    }

    fn temp_config_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "api-router-config-{}-{}-{}.json",
            name,
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        path
    }

    fn write_temp_config(path: &Path, port: u16) {
        let mut file = File::create(path).unwrap();
        write!(
            file,
            "{{\n  \"baseUrl\": \"https://example.com\",\n  \"headers\": {{}},\n  \"port\": {}\n}}",
            port
        )
        .unwrap();
        file.flush().unwrap();
    }

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

    #[test]
    #[serial_test::serial]
    fn load_api_config_uses_cache_until_file_changes() {
        reset_cache();
        let path = temp_config_path("cache-hit");
        write_temp_config(&path, 9100);
        std::env::set_var("API_ROUTER_CONFIG_PATH", &path);

        let first = load_api_config().expect("config should load");
        assert_eq!(first.port, 9100);
        let second = load_api_config().expect("cached config should load");
        assert!(Arc::ptr_eq(&first, &second));

        sleep(Duration::from_millis(10));
        write_temp_config(&path, 9200);

        let refreshed = load_api_config().expect("config should reload after change");
        assert_eq!(refreshed.port, 9200);
        assert!(!Arc::ptr_eq(&second, &refreshed));

        std::env::remove_var("API_ROUTER_CONFIG_PATH");
        fs::remove_file(&path).ok();
        reset_cache();
    }

    #[test]
    #[serial_test::serial]
    fn load_api_config_falls_back_when_primary_missing() {
        reset_cache();
        std::env::set_var("API_ROUTER_CONFIG_PATH", "./does-not-exist.json");

        let config = load_api_config().expect("fallback config should load");
        assert_eq!(config.port, 8000);
        assert_eq!(config.base_url, "https://portal.qwen.ai");

        std::env::remove_var("API_ROUTER_CONFIG_PATH");
        reset_cache();
    }

    #[test]
    #[serial_test::serial]
    fn load_api_config_propagates_parse_errors() {
        reset_cache();
        let path = temp_config_path("invalid");
        fs::write(&path, "{invalid json").unwrap();
        std::env::set_var("API_ROUTER_CONFIG_PATH", &path);

        let err = load_api_config().expect_err("invalid json should error");
        match err {
            RouterError::ConfigParse(message) => {
                assert!(message.contains("invalid"))
            }
            other => panic!("unexpected error: {:?}", other),
        }

        std::env::remove_var("API_ROUTER_CONFIG_PATH");
        fs::remove_file(&path).ok();
        reset_cache();
    }

    #[test]
    fn rate_limit_config_deserializes_with_all_fields() {
        let config: RateLimitConfig = serde_json::from_str(
            r#"{
                "requestsPerMinute": 60,
                "burst": 10
            }"#,
        )
        .unwrap();
        assert_eq!(config.requests_per_minute, Some(60));
        assert_eq!(config.burst, Some(10));
    }

    #[test]
    fn rate_limit_config_defaults_work() {
        let config: RateLimitConfig = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(config.requests_per_minute, None);
        assert_eq!(config.burst, None);
    }

    #[test]
    fn stream_config_uses_defaults() {
        let config: StreamConfig = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(config.buffer_size, 8192);
        assert_eq!(config.heartbeat_interval_secs, 30);
    }

    #[test]
    fn stream_config_custom_values() {
        let config: StreamConfig = serde_json::from_str(
            r#"{
                "bufferSize": 16384,
                "heartbeatIntervalSecs": 60
            }"#,
        )
        .unwrap();
        assert_eq!(config.buffer_size, 16384);
        assert_eq!(config.heartbeat_interval_secs, 60);
    }

    #[test]
    fn endpoint_config_deserializes_all_fields() {
        let config: EndpointConfig = serde_json::from_str(
            r#"{
                "upstreamPath": "/v1/messages",
                "method": "POST",
                "headers": {"X-API-Version": "2023"},
                "streamSupport": true,
                "requiresMultipart": false,
                "rateLimit": {
                    "requestsPerMinute": 30,
                    "burst": 5
                },
                "streamConfig": {
                    "bufferSize": 4096,
                    "heartbeatIntervalSecs": 15
                }
            }"#,
        )
        .unwrap();
        assert_eq!(config.upstream_path, Some("/v1/messages".to_string()));
        assert_eq!(config.method, Some("POST".to_string()));
        assert_eq!(
            config.headers.get("X-API-Version"),
            Some(&"2023".to_string())
        );
        assert!(config.stream_support);
        assert!(!config.requires_multipart);
        assert!(config.rate_limit.is_some());
        assert!(config.stream_config.is_some());
    }

    #[test]
    fn api_config_with_rate_limit_and_stream_config() {
        let config: ApiConfig = serde_json::from_str(
            r#"{
                "baseUrl": "https://api.example.com",
                "rateLimit": {
                    "requestsPerMinute": 100
                },
                "streamConfig": {
                    "bufferSize": 32768
                }
            }"#,
        )
        .unwrap();
        assert_eq!(config.base_url, "https://api.example.com");
        assert!(config.rate_limit.is_some());
        assert_eq!(config.rate_limit.unwrap().requests_per_minute, Some(100));
        assert!(config.stream_config.is_some());
        assert_eq!(config.stream_config.unwrap().buffer_size, 32768);
    }

    #[test]
    fn api_config_model_mapping() {
        let config: ApiConfig = serde_json::from_str(
            r#"{
                "baseUrl": "https://api.example.com",
                "modelMapping": {
                    "gpt-4": "claude-3-opus",
                    "gpt-3.5": "claude-3-sonnet"
                }
            }"#,
        )
        .unwrap();
        let mapping = config.model_mapping.as_ref().unwrap();
        assert_eq!(mapping.get("gpt-4"), Some(&"claude-3-opus".to_string()));
        assert_eq!(mapping.get("gpt-3.5"), Some(&"claude-3-sonnet".to_string()));
    }

    #[test]
    fn default_port_is_8000() {
        assert_eq!(default_port(), 8000);
    }

    #[test]
    fn api_config_uses_default_port() {
        let config: ApiConfig = serde_json::from_str(
            r#"{
                "baseUrl": "https://api.example.com"
            }"#,
        )
        .unwrap();
        assert_eq!(config.port, 8000);
    }

    #[test]
    fn api_config_custom_port() {
        let config: ApiConfig = serde_json::from_str(
            r#"{
                "baseUrl": "https://api.example.com",
                "port": 9000
            }"#,
        )
        .unwrap();
        assert_eq!(config.port, 9000);
    }

    #[test]
    fn rate_limit_config_equality() {
        let config1 = RateLimitConfig {
            requests_per_minute: Some(60),
            burst: Some(10),
        };
        let config2 = RateLimitConfig {
            requests_per_minute: Some(60),
            burst: Some(10),
        };
        let config3 = RateLimitConfig {
            requests_per_minute: Some(30),
            burst: Some(10),
        };
        assert_eq!(config1, config2);
        assert_ne!(config1, config3);
    }
}
