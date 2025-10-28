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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_http_request_extracts_all_parts() {
        let raw = b"POST /v1/chat HTTP/1.1\r\nHost: example.com\r\nContent-Type: application/json\r\n\r\n{\"test\":1}";
        let parsed = parse_http_request(raw).unwrap();
        assert_eq!(parsed.method(), "POST");
        assert_eq!(parsed.target(), "/v1/chat");
        assert_eq!(parsed.version(), "HTTP/1.1");
        assert_eq!(parsed.header("host"), Some("example.com"));
        assert_eq!(parsed.header("content-type"), Some("application/json"));
        assert_eq!(parsed.body(), b"{\"test\":1}");
    }

    #[test]
    fn parse_http_request_handles_empty_body() {
        let raw = b"GET /health HTTP/1.1\r\nHost: localhost\r\n\r\n";
        let parsed = parse_http_request(raw).unwrap();
        assert_eq!(parsed.method(), "GET");
        assert_eq!(parsed.target(), "/health");
        assert_eq!(parsed.body(), b"");
        assert!(!parsed.has_body());
    }

    #[test]
    fn parse_http_request_normalizes_header_names() {
        let raw =
            b"GET / HTTP/1.1\r\nContent-Type: text/plain\r\nAuthorization: Bearer token\r\n\r\n";
        let parsed = parse_http_request(raw).unwrap();
        assert_eq!(parsed.header("content-type"), Some("text/plain"));
        assert_eq!(parsed.header("authorization"), Some("Bearer token"));
    }

    #[test]
    fn parse_http_request_fails_on_malformed_request() {
        let raw = b"GET /test HTTP/1.1";
        let result = parse_http_request(raw);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RouterError::BadRequest(_)));
    }

    #[test]
    fn parse_http_request_fails_on_invalid_request_line() {
        let raw = b"INVALID\r\n\r\n";
        let result = parse_http_request(raw);
        assert!(result.is_err());
    }

    #[test]
    fn route_path_strips_query_string() {
        let mut headers = HashMap::new();
        headers.insert("host".to_string(), "localhost".to_string());
        let parsed = ParsedRequest::new_for_tests(
            "GET",
            "/v1/chat/completions?stream=true&temp=0.7",
            "HTTP/1.1",
            headers,
            vec![],
        );
        assert_eq!(parsed.route_path(), "/v1/chat/completions");
    }

    #[test]
    fn route_path_returns_full_path_when_no_query() {
        let parsed =
            ParsedRequest::new_for_tests("GET", "/v1/models", "HTTP/1.1", HashMap::new(), vec![]);
        assert_eq!(parsed.route_path(), "/v1/models");
    }

    #[test]
    fn has_body_returns_true_for_non_empty_body() {
        let parsed = ParsedRequest::new_for_tests(
            "POST",
            "/test",
            "HTTP/1.1",
            HashMap::new(),
            b"data".to_vec(),
        );
        assert!(parsed.has_body());
    }

    #[test]
    fn extract_content_length_parses_valid_header() {
        let headers = "Host: example.com\r\nContent-Length: 42\r\nOther: value";
        assert_eq!(extract_content_length(headers), Some(42));
    }

    #[test]
    fn extract_content_length_handles_case_insensitive() {
        let headers = "content-length: 123";
        assert_eq!(extract_content_length(headers), Some(123));
    }

    #[test]
    fn extract_content_length_returns_none_when_missing() {
        let headers = "Host: example.com\r\nOther: value";
        assert_eq!(extract_content_length(headers), None);
    }

    #[test]
    fn extract_content_length_returns_none_on_invalid_value() {
        let headers = "Content-Length: invalid";
        assert_eq!(extract_content_length(headers), None);
    }

    #[test]
    fn resolve_default_api_key_uses_env_var() {
        std::env::set_var("DEFAULT_API_KEY", "test-key-123");
        let key = resolve_default_api_key();
        assert_eq!(key, "test-key-123");
        std::env::remove_var("DEFAULT_API_KEY");
    }

    #[test]
    fn resolve_default_api_key_falls_back_to_placeholder() {
        std::env::remove_var("DEFAULT_API_KEY");
        let key = resolve_default_api_key();
        assert_eq!(key, DEFAULT_API_KEY_PLACEHOLDER);
    }

    #[test]
    fn extract_client_api_key_parses_bearer_token() {
        let mut headers = HashMap::new();
        headers.insert(
            "authorization".to_string(),
            "Bearer client-key-xyz".to_string(),
        );
        let key = extract_client_api_key(&headers, "fallback");
        assert_eq!(key, "client-key-xyz");
    }

    #[test]
    fn extract_client_api_key_falls_back_to_default() {
        let headers = HashMap::new();
        let key = extract_client_api_key(&headers, "default-key");
        assert_eq!(key, "default-key");
    }

    #[test]
    fn extract_client_api_key_uses_raw_value_if_not_bearer() {
        let mut headers = HashMap::new();
        headers.insert(
            "authorization".to_string(),
            "ApiKey raw-key-value".to_string(),
        );
        let key = extract_client_api_key(&headers, "fallback");
        assert_eq!(key, "ApiKey raw-key-value");
    }

    #[test]
    fn anonymize_key_masks_middle_characters() {
        let key = "abcdefghij";
        let anon = anonymize_key(key);
        assert_eq!(anon, "abcd***ij");
    }

    #[test]
    fn anonymize_key_handles_short_key() {
        let key = "abc";
        let anon = anonymize_key(key);
        assert_eq!(anon, "abc***");
    }

    #[test]
    fn anonymize_key_handles_very_short_key() {
        let key = "ab";
        let anon = anonymize_key(key);
        assert_eq!(anon, "ab***");
    }

    #[test]
    fn anonymize_key_handles_empty_key() {
        let key = "";
        let anon = anonymize_key(key);
        assert_eq!(anon, "unknown");
    }

    #[test]
    fn parse_authorization_header_extracts_bearer_token() {
        let token = parse_authorization_header("Bearer token123");
        assert_eq!(token, Some("token123".to_string()));
    }

    #[test]
    fn parse_authorization_header_case_insensitive() {
        let token = parse_authorization_header("bearer token456");
        assert_eq!(token, Some("token456".to_string()));
    }

    #[test]
    fn parse_authorization_header_returns_raw_for_other_schemes() {
        let token = parse_authorization_header("ApiKey my-api-key");
        assert_eq!(token, Some("ApiKey my-api-key".to_string()));
    }

    #[test]
    fn parse_authorization_header_returns_none_for_empty() {
        let token = parse_authorization_header("");
        assert_eq!(token, None);
    }

    #[test]
    fn parse_authorization_header_trims_whitespace() {
        let token = parse_authorization_header("  Bearer  token789  ");
        assert_eq!(token, Some("token789".to_string()));
    }
}
