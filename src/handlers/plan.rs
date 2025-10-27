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
