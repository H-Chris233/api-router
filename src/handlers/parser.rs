use crate::errors::{RouterError, RouterResult};
use std::collections::HashMap;
use std::env;

pub(super) const DEFAULT_API_KEY_PLACEHOLDER: &str =
    "j88R1cKdHY1EcYk9hO5vJIrV3f4rrtI5I9NuFyyTiFLDCXRhY8ooddL72AT1NqyHKMf3iGvib2W9XBYV8duUtw";

#[derive(Debug, Clone)]
pub(super) struct ParsedRequest {
    method: String,
    target: String,
    version: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

impl ParsedRequest {
    pub fn method(&self) -> &str {
        &self.method
    }

    pub fn target(&self) -> &str {
        &self.target
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn headers(&self) -> &HashMap<String, String> {
        &self.headers
    }

    pub fn header(&self, key: &str) -> Option<&str> {
        self.headers.get(key).map(|value| value.as_str())
    }

    pub fn body(&self) -> &[u8] {
        &self.body
    }

    pub fn route_path(&self) -> &str {
        self.target
            .split_once('?')
            .map(|(path, _)| path)
            .unwrap_or_else(|| self.target.as_str())
    }

    pub fn has_body(&self) -> bool {
        !self.body.is_empty()
    }

    #[cfg(test)]
    pub fn new_for_tests(
        method: &str,
        target: &str,
        version: &str,
        headers: HashMap<String, String>,
        body: Vec<u8>,
    ) -> Self {
        let normalized_headers = headers
            .into_iter()
            .map(|(name, value)| (name.to_lowercase(), value))
            .collect();

        Self {
            method: method.to_string(),
            target: target.to_string(),
            version: version.to_string(),
            headers: normalized_headers,
            body,
        }
    }
}

pub(super) fn extract_content_length(headers: &str) -> Option<usize> {
    for line in headers.lines() {
        if line.to_lowercase().starts_with("content-length:") {
            return line[15..].trim().parse::<usize>().ok();
        }
    }
    None
}

pub(super) fn parse_http_request(request_bytes: &[u8]) -> RouterResult<ParsedRequest> {
    let header_end = request_bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| RouterError::BadRequest("Malformed HTTP request".to_string()))?;

    let header_bytes = &request_bytes[..header_end];
    let header_str = std::str::from_utf8(header_bytes)
        .map_err(|_| RouterError::BadRequest("Invalid HTTP headers".to_string()))?;

    let mut header_lines = header_str.split("\r\n");
    let request_line = header_lines
        .next()
        .ok_or_else(|| RouterError::BadRequest("Missing request line".to_string()))?;
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 3 {
        return Err(RouterError::BadRequest("Invalid request line".to_string()));
    }

    let mut headers = HashMap::with_capacity(16);
    for line in header_lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_lowercase(), value.trim().to_string());
        }
    }

    let body = request_bytes[header_end + 4..].to_vec();

    Ok(ParsedRequest {
        method: parts[0].to_string(),
        target: parts[1].to_string(),
        version: parts[2].to_string(),
        headers,
        body,
    })
}

pub(super) fn resolve_default_api_key() -> String {
    env::var("DEFAULT_API_KEY").unwrap_or_else(|_| DEFAULT_API_KEY_PLACEHOLDER.to_string())
}

pub(super) fn extract_client_api_key(
    headers: &HashMap<String, String>,
    default_api_key: &str,
) -> String {
    headers
        .get("authorization")
        .and_then(|raw| parse_authorization_header(raw))
        .filter(|token| !token.is_empty())
        .unwrap_or_else(|| default_api_key.to_string())
}

pub(super) fn anonymize_key(key: &str) -> String {
    if key.is_empty() {
        return "unknown".to_string();
    }
    let prefix_len = key.len().min(4);
    let suffix_len = key.len().saturating_sub(prefix_len).min(2);
    let prefix = &key[..prefix_len];
    let suffix = if suffix_len > 0 {
        &key[key.len() - suffix_len..]
    } else {
        ""
    };
    format!("{}***{}", prefix, suffix)
}

fn parse_authorization_header(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut parts = trimmed.split_whitespace();
    let scheme = parts.next().unwrap_or("");
    if scheme.eq_ignore_ascii_case("bearer") {
        parts.next().map(|token| token.to_string())
    } else {
        Some(trimmed.to_string())
    }
}
