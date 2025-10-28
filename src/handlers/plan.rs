use crate::config::{ApiConfig, EndpointConfig, StreamConfig};

use super::parser::ParsedRequest;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub(super) struct ForwardPlan {
    method: String,
    headers: HashMap<String, String>,
    base_url: String,
    path: String,
    stream_config: Option<StreamConfig>,
}

impl ForwardPlan {
    pub fn method(&self) -> &str {
        &self.method
    }

    pub fn headers(&self) -> &HashMap<String, String> {
        &self.headers
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn stream_config(&self) -> Option<&StreamConfig> {
        self.stream_config.as_ref()
    }

    pub fn full_url(&self) -> String {
        join_base_and_path(&self.base_url, &self.path)
    }
}

pub(super) fn map_model_name(config: &ApiConfig, model: &str) -> String {
    config
        .model_mapping
        .as_ref()
        .and_then(|mapping| mapping.get(model).cloned())
        .unwrap_or_else(|| model.to_string())
}

pub(super) fn prepare_forward_plan(
    route_path: &str,
    request: &ParsedRequest,
    config: &ApiConfig,
    default_api_key: &str,
    content_type: Option<&str>,
) -> ForwardPlan {
    let endpoint = config.endpoint(route_path);
    let base_url = normalized_base_url(&config.base_url);
    let path = compute_upstream_path(request.target(), &endpoint);
    let method = endpoint
        .method
        .as_deref()
        .unwrap_or("POST")
        .to_ascii_uppercase();
    let headers = build_upstream_headers(
        config,
        &endpoint,
        request.headers(),
        default_api_key,
        content_type,
    );
    let stream_config = endpoint
        .stream_config
        .clone()
        .or_else(|| config.stream_config.clone());

    ForwardPlan {
        method,
        headers,
        base_url,
        path,
        stream_config,
    }
}

pub(super) fn compute_upstream_path(request_target: &str, endpoint: &EndpointConfig) -> String {
    let normalize = |path: &str| {
        if path.starts_with("http://") || path.starts_with("https://") {
            path.to_string()
        } else if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{}", path)
        }
    };

    if let Some(upstream) = endpoint.upstream_path.as_deref() {
        let mut path = normalize(upstream);
        if let Some(query_index) = request_target.find('?') {
            let query = &request_target[query_index + 1..];
            if path.contains('?') {
                if !query.is_empty() {
                    if !path.ends_with('?') && !path.ends_with('&') {
                        path.push('&');
                    }
                    path.push_str(query);
                }
            } else if !query.is_empty() {
                path.push('?');
                path.push_str(query);
            } else {
                path.push('?');
            }
        }
        path
    } else if request_target.starts_with('/') {
        request_target.to_string()
    } else {
        format!("/{}", request_target)
    }
}

fn normalized_base_url(base: &str) -> String {
    if base.trim().is_empty() {
        return String::new();
    }
    let prefixed = if base.starts_with("http://") || base.starts_with("https://") {
        base.to_string()
    } else {
        format!("https://{}", base)
    };
    prefixed.trim_end_matches('/').to_string()
}

fn join_base_and_path(base: &str, path: &str) -> String {
    if path.starts_with("http://") || path.starts_with("https://") {
        return path.to_string();
    }
    if base.is_empty() {
        return path.to_string();
    }
    if path.is_empty() {
        base.to_string()
    } else if path.starts_with('/') {
        format!("{}{}", base, path)
    } else {
        format!("{}/{}", base, path)
    }
}

fn build_upstream_headers(
    config: &ApiConfig,
    endpoint: &EndpointConfig,
    client_headers: &HashMap<String, String>,
    default_api_key: &str,
    content_type: Option<&str>,
) -> HashMap<String, String> {
    let mut headers = HashMap::with_capacity(config.headers.len() + endpoint.headers.len() + 4);

    for (key, value) in &config.headers {
        headers.insert(key.clone(), value.clone());
    }
    for (key, value) in &endpoint.headers {
        headers.insert(key.clone(), value.clone());
    }

    if let Some(ct) = content_type {
        headers.insert("Content-Type".to_string(), ct.to_string());
    }

    if !has_header_case_insensitive(&headers, "authorization") {
        if let Some(auth) = client_headers.get("authorization") {
            headers.insert("Authorization".to_string(), auth.clone());
        } else {
            headers.insert(
                "Authorization".to_string(),
                format!("Bearer {}", default_api_key),
            );
        }
    }

    copy_header_if_present(&mut headers, client_headers, "accept", "Accept");
    copy_header_if_present(&mut headers, client_headers, "user-agent", "User-Agent");
    copy_header_if_present(&mut headers, client_headers, "x-request-id", "x-request-id");

    headers
}

fn has_header_case_insensitive(headers: &HashMap<String, String>, target: &str) -> bool {
    headers.keys().any(|key| key.eq_ignore_ascii_case(target))
}

fn copy_header_if_present(
    headers: &mut HashMap<String, String>,
    client_headers: &HashMap<String, String>,
    client_key: &str,
    canonical_key: &str,
) {
    if !has_header_case_insensitive(headers, canonical_key) {
        if let Some(value) = client_headers.get(client_key) {
            headers.insert(canonical_key.to_string(), value.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_config() -> ApiConfig {
        ApiConfig {
            base_url: "https://api.example.com".to_string(),
            headers: HashMap::new(),
            model_mapping: None,
            endpoints: HashMap::new(),
            port: 8000,
            rate_limit: None,
            stream_config: None,
        }
    }

    fn mock_parsed_request(target: &str) -> ParsedRequest {
        ParsedRequest::new_for_tests("POST", target, "HTTP/1.1", HashMap::new(), vec![])
    }

    #[test]
    fn map_model_name_uses_mapping() {
        let mut config = base_config();
        let mut mapping = HashMap::new();
        mapping.insert("gpt-4".to_string(), "claude-3-opus".to_string());
        mapping.insert("gpt-3.5".to_string(), "claude-3-sonnet".to_string());
        config.model_mapping = Some(mapping);

        assert_eq!(map_model_name(&config, "gpt-4"), "claude-3-opus");
        assert_eq!(map_model_name(&config, "gpt-3.5"), "claude-3-sonnet");
    }

    #[test]
    fn map_model_name_returns_original_when_not_mapped() {
        let config = base_config();
        assert_eq!(map_model_name(&config, "unknown-model"), "unknown-model");
    }

    #[test]
    fn map_model_name_returns_original_when_no_mapping() {
        let mut config = base_config();
        let mapping = HashMap::new();
        config.model_mapping = Some(mapping);
        assert_eq!(map_model_name(&config, "any-model"), "any-model");
    }

    #[test]
    fn normalized_base_url_trims_trailing_slash() {
        let result = normalized_base_url("https://api.example.com/");
        assert_eq!(result, "https://api.example.com");
    }

    #[test]
    fn normalized_base_url_adds_https_prefix() {
        let result = normalized_base_url("api.example.com");
        assert_eq!(result, "https://api.example.com");
    }

    #[test]
    fn normalized_base_url_preserves_http() {
        let result = normalized_base_url("http://localhost:8080");
        assert_eq!(result, "http://localhost:8080");
    }

    #[test]
    fn normalized_base_url_handles_empty() {
        let result = normalized_base_url("");
        assert_eq!(result, "");
    }

    #[test]
    fn join_base_and_path_combines_correctly() {
        let result = join_base_and_path("https://api.example.com", "/v1/chat");
        assert_eq!(result, "https://api.example.com/v1/chat");
    }

    #[test]
    fn join_base_and_path_handles_absolute_url_in_path() {
        let result = join_base_and_path("https://api.example.com", "https://other.com/path");
        assert_eq!(result, "https://other.com/path");
    }

    #[test]
    fn join_base_and_path_handles_empty_base() {
        let result = join_base_and_path("", "/v1/chat");
        assert_eq!(result, "/v1/chat");
    }

    #[test]
    fn join_base_and_path_handles_path_without_leading_slash() {
        let result = join_base_and_path("https://api.example.com", "v1/chat");
        assert_eq!(result, "https://api.example.com/v1/chat");
    }

    #[test]
    fn join_base_and_path_handles_empty_path() {
        let result = join_base_and_path("https://api.example.com", "");
        assert_eq!(result, "https://api.example.com");
    }

    #[test]
    fn compute_upstream_path_uses_override() {
        let mut endpoint = EndpointConfig::default();
        endpoint.upstream_path = Some("/v1/messages".to_string());
        let result = compute_upstream_path("/v1/chat/completions", &endpoint);
        assert_eq!(result, "/v1/messages");
    }

    #[test]
    fn compute_upstream_path_preserves_query_string() {
        let mut endpoint = EndpointConfig::default();
        endpoint.upstream_path = Some("/v1/messages".to_string());
        let result = compute_upstream_path("/v1/chat/completions?foo=bar&baz=qux", &endpoint);
        assert_eq!(result, "/v1/messages?foo=bar&baz=qux");
    }

    #[test]
    fn compute_upstream_path_merges_query_strings() {
        let mut endpoint = EndpointConfig::default();
        endpoint.upstream_path = Some("/v1/messages?api_version=2".to_string());
        let result = compute_upstream_path("/v1/chat?user=test", &endpoint);
        assert_eq!(result, "/v1/messages?api_version=2&user=test");
    }

    #[test]
    fn compute_upstream_path_falls_back_to_request_target() {
        let endpoint = EndpointConfig::default();
        let result = compute_upstream_path("/v1/chat/completions", &endpoint);
        assert_eq!(result, "/v1/chat/completions");
    }

    #[test]
    fn compute_upstream_path_adds_leading_slash() {
        let endpoint = EndpointConfig::default();
        let result = compute_upstream_path("v1/chat", &endpoint);
        assert_eq!(result, "/v1/chat");
    }

    #[test]
    fn prepare_forward_plan_builds_full_url() {
        let config = base_config();
        let request = mock_parsed_request("/v1/chat");
        let plan = prepare_forward_plan("/v1/chat", &request, &config, "test-key", None);
        assert_eq!(plan.full_url(), "https://api.example.com/v1/chat");
    }

    #[test]
    fn prepare_forward_plan_uses_endpoint_method() {
        let mut config = base_config();
        let mut endpoint = EndpointConfig::default();
        endpoint.method = Some("PATCH".to_string());
        config.endpoints.insert("/v1/test".to_string(), endpoint);
        let request = mock_parsed_request("/v1/test");
        let plan = prepare_forward_plan("/v1/test", &request, &config, "key", None);
        assert_eq!(plan.method(), "PATCH");
    }

    #[test]
    fn prepare_forward_plan_defaults_to_post() {
        let config = base_config();
        let request = mock_parsed_request("/v1/chat");
        let plan = prepare_forward_plan("/v1/chat", &request, &config, "key", None);
        assert_eq!(plan.method(), "POST");
    }

    #[test]
    fn prepare_forward_plan_includes_authorization() {
        let config = base_config();
        let request = mock_parsed_request("/v1/chat");
        let plan = prepare_forward_plan("/v1/chat", &request, &config, "my-api-key", None);
        assert_eq!(
            plan.headers().get("Authorization"),
            Some(&"Bearer my-api-key".to_string())
        );
    }

    #[test]
    fn prepare_forward_plan_merges_config_and_endpoint_headers() {
        let mut config = base_config();
        config
            .headers
            .insert("X-Global".to_string(), "global-value".to_string());
        let mut endpoint = EndpointConfig::default();
        endpoint
            .headers
            .insert("X-Endpoint".to_string(), "endpoint-value".to_string());
        config.endpoints.insert("/v1/test".to_string(), endpoint);
        let request = mock_parsed_request("/v1/test");
        let plan = prepare_forward_plan("/v1/test", &request, &config, "key", None);
        assert_eq!(
            plan.headers().get("X-Global"),
            Some(&"global-value".to_string())
        );
        assert_eq!(
            plan.headers().get("X-Endpoint"),
            Some(&"endpoint-value".to_string())
        );
    }

    #[test]
    fn prepare_forward_plan_includes_content_type() {
        let config = base_config();
        let request = mock_parsed_request("/v1/chat");
        let plan = prepare_forward_plan(
            "/v1/chat",
            &request,
            &config,
            "key",
            Some("application/json"),
        );
        assert_eq!(
            plan.headers().get("Content-Type"),
            Some(&"application/json".to_string())
        );
    }

    #[test]
    fn prepare_forward_plan_copies_client_headers() {
        let config = base_config();
        let mut headers = HashMap::new();
        headers.insert("accept".to_string(), "application/json".to_string());
        headers.insert("user-agent".to_string(), "TestClient/1.0".to_string());
        headers.insert("x-request-id".to_string(), "req-123".to_string());
        let request = ParsedRequest::new_for_tests("POST", "/v1/chat", "HTTP/1.1", headers, vec![]);
        let plan = prepare_forward_plan("/v1/chat", &request, &config, "key", None);
        assert_eq!(
            plan.headers().get("Accept"),
            Some(&"application/json".to_string())
        );
        assert_eq!(
            plan.headers().get("User-Agent"),
            Some(&"TestClient/1.0".to_string())
        );
        assert_eq!(
            plan.headers().get("x-request-id"),
            Some(&"req-123".to_string())
        );
    }

    #[test]
    fn prepare_forward_plan_preserves_client_authorization() {
        let config = base_config();
        let mut headers = HashMap::new();
        headers.insert("authorization".to_string(), "Bearer client-key".to_string());
        let request = ParsedRequest::new_for_tests("POST", "/v1/chat", "HTTP/1.1", headers, vec![]);
        let plan = prepare_forward_plan("/v1/chat", &request, &config, "default-key", None);
        assert_eq!(
            plan.headers().get("Authorization"),
            Some(&"Bearer client-key".to_string())
        );
    }

    #[test]
    fn has_header_case_insensitive_works() {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        headers.insert("authorization".to_string(), "Bearer token".to_string());
        assert!(has_header_case_insensitive(&headers, "content-type"));
        assert!(has_header_case_insensitive(&headers, "AUTHORIZATION"));
        assert!(has_header_case_insensitive(&headers, "Content-Type"));
        assert!(!has_header_case_insensitive(&headers, "Accept"));
    }

    #[test]
    fn forward_plan_accessors_work() {
        let plan = ForwardPlan {
            method: "POST".to_string(),
            headers: HashMap::new(),
            base_url: "https://api.test".to_string(),
            path: "/v1/chat".to_string(),
            stream_config: None,
        };
        assert_eq!(plan.method(), "POST");
        assert_eq!(plan.base_url(), "https://api.test");
        assert_eq!(plan.path(), "/v1/chat");
        assert!(plan.stream_config().is_none());
    }
}
