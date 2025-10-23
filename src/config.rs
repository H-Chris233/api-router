use crate::errors::{RouterError, RouterResult};
use log::{debug, warn};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::SystemTime;

const FALLBACK_CONFIG_PATH: &str = "./transformer/qwen.json";
const CONFIG_BUFFER_SIZE: usize = 128 * 1024;

#[derive(Default)]
struct ConfigCache {
    entry: Option<CachedConfig>,
}

struct CachedConfig {
    config: Arc<ApiConfig>,
    source: PathBuf,
    modified: Option<SystemTime>,
}

#[derive(Clone)]
struct ConfigPaths {
    primary: PathBuf,
    fallback: PathBuf,
}

static CONFIG_CACHE: OnceLock<RwLock<ConfigCache>> = OnceLock::new();

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

pub fn default_port() -> u16 {
    8000
}

fn cache_cell() -> &'static RwLock<ConfigCache> {
    CONFIG_CACHE.get_or_init(|| RwLock::new(ConfigCache::default()))
}

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

fn read_config_from_path(path: &Path) -> RouterResult<(ApiConfig, Option<SystemTime>)> {
    let file = File::open(path).map_err(|e| {
        RouterError::ConfigRead(format!(
            "failed to open {}: {}",
            path.display(),
            e
        ))
    })?;
    let modified = file.metadata().ok().and_then(|meta| meta.modified().ok());
    let mut reader = BufReader::with_capacity(CONFIG_BUFFER_SIZE, file);
    let config: ApiConfig = serde_json::from_reader(&mut reader).map_err(|e| {
        RouterError::ConfigParse(format!("{}: {}", path.display(), e))
    })?;
    Ok((config, modified))
}

fn load_config_with_paths(paths: &ConfigPaths) -> RouterResult<CachedConfig> {
    match read_config_from_path(&paths.primary) {
        Ok((config, modified)) => {
            debug!(
                "loaded API config from {}",
                paths.primary.display()
            );
            Ok(CachedConfig {
                config: Arc::new(config),
                source: paths.primary.clone(),
                modified,
            })
        }
        Err(err) => match err {
            RouterError::ConfigParse(_) => Err(err),
            RouterError::ConfigRead(msg) => {
                warn!(
                    "{}; falling back to {}",
                    msg,
                    paths.fallback.display()
                );
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

pub fn load_api_config() -> RouterResult<Arc<ApiConfig>> {
    let paths = resolve_config_paths();
    let cache = cache_cell();

    {
        let guard = cache.read().expect("config cache poisoned");
        if let Some(entry) = &guard.entry {
            if !needs_reload(entry, &paths) {
                debug!(
                    "using cached API config from {}",
                    entry.source.display()
                );
                return Ok(entry.config.clone());
            }
        }
    }

    let mut guard = cache.write().expect("config cache poisoned");
    if let Some(entry) = &guard.entry {
        if !needs_reload(entry, &paths) {
            debug!(
                "using cached API config from {}",
                entry.source.display()
            );
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
}
