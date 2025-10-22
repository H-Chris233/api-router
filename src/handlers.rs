use crate::config::{ApiConfig, EndpointConfig};
use crate::errors::{RouterError, RouterResult};
use crate::http_client::{handle_streaming_request, send_http_request};
use crate::models::{ChatCompletionRequest, CompletionRequest, EmbeddingRequest};
use crate::rate_limit::{resolve_rate_limit_settings, RateLimitDecision, RATE_LIMITER};
use futures_lite::{AsyncReadExt, AsyncWriteExt};
use log::{debug, warn};
use serde_json::json;
use smol::net::TcpStream;
use std::collections::HashMap;
use std::env;
use std::fs;
#[cfg(test)]
use std::sync::{Mutex, OnceLock};

const DEFAULT_API_KEY_PLACEHOLDER: &str =
    "j88R1cKdHY1EcYk9hO5vJIrV3f4rrtI5I9NuFyyTiFLDCXRhY8ooddL72AT1NqyHKMf3iGvib2W9XBYV8duUtw";

// 从HTTP头部提取Content-Length
pub fn extract_content_length(headers: &str) -> Option<usize> {
    for line in headers.lines() {
        if line.to_lowercase().starts_with("content-length:") {
            return line[15..].trim().parse::<usize>().ok();
        }
    }
    None
}

pub fn build_error_response(status_code: u16, reason: &str, message: &str) -> String {
    build_error_response_with_headers(status_code, reason, message, &[])
}

pub fn build_error_response_with_headers(
    status_code: u16,
    reason: &str,
    message: &str,
    extra_headers: &[(&str, String)],
) -> String {
    let body = serde_json::json!({
        "error": {
            "message": message,
        }
    })
    .to_string();
    let mut response = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
        status_code,
        reason,
        body.len()
    );
    for (key, value) in extra_headers {
        response.push_str(&format!("{}: {}\r\n", key, value));
    }
    response.push_str("\r\n");
    response.push_str(&body);
    response
}

pub fn map_error_to_response(err: &RouterError) -> String {
    match err {
        RouterError::BadRequest(msg) => build_error_response(400, "BAD REQUEST", msg),
        RouterError::ConfigRead(msg) | RouterError::ConfigParse(msg) => {
            build_error_response(500, "INTERNAL SERVER ERROR", msg)
        }
        RouterError::Url(msg) | RouterError::Tls(msg) | RouterError::Upstream(msg) => {
            build_error_response(502, "BAD GATEWAY", msg)
        }
        RouterError::Io(msg) => {
            build_error_response(500, "INTERNAL SERVER ERROR", &msg.to_string())
        }
        RouterError::Json(msg) => build_error_response(400, "BAD REQUEST", &msg.to_string()),
    }
}

struct ParsedRequest {
    method: String,
    target: String,
    version: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

fn parse_http_request(request_bytes: &[u8]) -> RouterResult<ParsedRequest> {
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

    let mut headers = HashMap::new();
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

fn load_api_config() -> RouterResult<ApiConfig> {
    let args: Vec<String> = env::args().collect();
    let config_basename = if args.len() > 1 {
        args[1].clone()
    } else {
        "qwen".to_string()
    };
    let config_file = format!("./transformer/{}.json", config_basename);
    let config_content = fs::read_to_string(&config_file).or_else(|e| {
        warn!(
            "Failed to read config {}: {}. Falling back to transformer/qwen.json",
            config_file, e
        );
        fs::read_to_string("./transformer/qwen.json")
            .map_err(|e2| RouterError::ConfigRead(e2.to_string()))
    })?;

    serde_json::from_str(&config_content).map_err(|e| RouterError::ConfigParse(e.to_string()))
}

fn resolve_default_api_key() -> String {
    env::var("DEFAULT_API_KEY").unwrap_or_else(|_| DEFAULT_API_KEY_PLACEHOLDER.to_string())
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

fn extract_client_api_key(headers: &HashMap<String, String>, default_api_key: &str) -> String {
    headers
        .get("authorization")
        .and_then(|raw| parse_authorization_header(raw))
        .filter(|token| !token.is_empty())
        .unwrap_or_else(|| default_api_key.to_string())
}

fn anonymize_key(key: &str) -> String {
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

fn compute_upstream_path(request_target: &str, endpoint: &EndpointConfig) -> String {
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

fn build_upstream_headers(
    config: &ApiConfig,
    endpoint: &EndpointConfig,
    client_headers: &HashMap<String, String>,
    default_api_key: &str,
    content_type: Option<&str>,
) -> HashMap<String, String> {
    let mut headers = HashMap::new();

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

#[cfg(test)]
type MockHttpHandler = Box<
    dyn Fn(&str, &str, &HashMap<String, String>, Option<&[u8]>) -> RouterResult<Vec<u8>>
        + Send
        + Sync,
>;

#[cfg(test)]
static HTTP_CLIENT_OVERRIDE: OnceLock<Mutex<Option<MockHttpHandler>>> = OnceLock::new();

#[cfg(test)]
fn with_mock_http_client<F, R>(mock: MockHttpHandler, f: F) -> R
where
    F: FnOnce() -> R,
{
    let cell = HTTP_CLIENT_OVERRIDE.get_or_init(|| Mutex::new(None));
    {
        let mut guard = cell.lock().unwrap();
        *guard = Some(mock);
    }
    let result = f();
    {
        let mut guard = cell.lock().unwrap();
        *guard = None;
    }
    result
}

fn map_model_name(config: &ApiConfig, model: &str) -> String {
    config
        .model_mapping
        .as_ref()
        .and_then(|mapping| mapping.get(model).cloned())
        .unwrap_or_else(|| model.to_string())
}

fn find_model_value_bounds(body: &[u8]) -> Option<(usize, usize)> {
    let marker = b"name=\"model\"";
    let marker_index = body
        .windows(marker.len())
        .position(|window| window == marker)?;
    let after_marker = marker_index + marker.len();
    let separator = b"\r\n\r\n";
    let remainder = &body[after_marker..];
    let separator_index = remainder
        .windows(separator.len())
        .position(|window| window == separator)?;
    let value_start = after_marker + separator_index + separator.len();
    let rest = &body[value_start..];
    let value_end_relative = rest
        .windows(2)
        .position(|window| window == b"\r\n")
        .unwrap_or(rest.len());
    let value_end = value_start + value_end_relative;
    Some((value_start, value_end))
}

fn extract_model_from_multipart(body: &[u8]) -> Option<String> {
    let (start, end) = find_model_value_bounds(body)?;
    std::str::from_utf8(&body[start..end])
        .ok()
        .map(|s| s.to_string())
}

fn replace_model_in_multipart(body: &[u8], new_model: &str) -> Vec<u8> {
    if let Some((start, end)) = find_model_value_bounds(body) {
        let mut result = Vec::with_capacity(body.len() - (end - start) + new_model.len());
        result.extend_from_slice(&body[..start]);
        result.extend_from_slice(new_model.as_bytes());
        result.extend_from_slice(&body[end..]);
        result
    } else {
        body.to_vec()
    }
}

async fn forward_to_upstream(
    url: &str,
    method: &str,
    headers: &HashMap<String, String>,
    body: Option<&[u8]>,
) -> RouterResult<Vec<u8>> {
    #[cfg(test)]
    {
        if let Some(lock) = HTTP_CLIENT_OVERRIDE.get() {
            if let Some(ref handler) = *lock.lock().unwrap() {
                return (handler)(url, method, headers, body);
            }
        }
    }

    send_http_request(url, method, headers, body).await
}

async fn write_response(
    stream: &mut TcpStream,
    content_type: &str,
    payload: &[u8],
) -> RouterResult<()> {
    let mut response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n",
        content_type,
        payload.len()
    )
    .into_bytes();
    response.extend_from_slice(payload);
    stream.write_all(&response).await?;
    stream.flush().await?;
    Ok(())
}

pub async fn handle_request(mut stream: TcpStream, addr: std::net::SocketAddr) {
    debug!("New connection from {}", addr);

    let mut request_bytes = Vec::new();
    let mut buffer = [0; 4096];

    for _ in 0..1000 {
        match stream.read(&mut buffer).await {
            Ok(0) => break,
            Ok(n) => {
                request_bytes.extend_from_slice(&buffer[..n]);
                let request_str = String::from_utf8_lossy(&request_bytes);
                if let Some(body_start) = request_str.find("\r\n\r\n") {
                    let header_end = body_start + 4;
                    let headers = &request_str[..body_start];
                    if let Some(content_length) = extract_content_length(headers) {
                        if request_bytes.len() >= header_end + content_length {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }
            Err(e) => {
                warn!("Failed to read from {}: {}", addr, e);
                return;
            }
        }
    }

    if request_bytes.is_empty() {
        return;
    }

    let parsed_request = match parse_http_request(&request_bytes) {
        Ok(req) => req,
        Err(err) => {
            let response = map_error_to_response(&err);
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.flush().await;
            return;
        }
    };

    let route_path = parsed_request
        .target
        .split('?')
        .next()
        .unwrap_or(parsed_request.target.as_str());

    match (parsed_request.method.as_str(), route_path) {
        ("GET", "/health") => {
            let snapshot = RATE_LIMITER.snapshot();
            let body = json!({
                "status": "ok",
                "message": "Light API Router running",
                "rateLimiter": {
                    "activeBuckets": snapshot.active_buckets,
                    "routes": snapshot.routes,
                }
            });
            let payload = body.to_string();
            let _ = write_response(&mut stream, "application/json", payload.as_bytes()).await;
        }
        ("GET", "/v1/models") => {
            let body = b"{\"object\": \"list\", \"data\": [{\"id\": \"qwen3-coder-plus\", \"object\": \"model\", \"created\": 1677610602, \"owned_by\": \"organization-owner\"}]}";
            let _ = write_response(&mut stream, "application/json", body).await;
        }
        ("POST", "/v1/chat/completions")
        | ("POST", "/v1/completions")
        | ("POST", "/v1/embeddings")
        | ("POST", "/v1/audio/transcriptions")
        | ("POST", "/v1/audio/translations") => {
            let config = match load_api_config() {
                Ok(cfg) => cfg,
                Err(err) => {
                    let response = map_error_to_response(&err);
                    let _ = stream.write_all(response.as_bytes()).await;
                    let _ = stream.flush().await;
                    return;
                }
            };
            let default_api_key = resolve_default_api_key();
            let client_api_key = extract_client_api_key(&parsed_request.headers, &default_api_key);

            if let Some(settings) = resolve_rate_limit_settings(route_path, &config) {
                match RATE_LIMITER.check(route_path, &client_api_key, &settings) {
                    RateLimitDecision::Allowed => {}
                    RateLimitDecision::Limited {
                        retry_after_seconds,
                    } => {
                        warn!(
                            "Rate limit exceeded for route {} and client {}",
                            route_path,
                            anonymize_key(&client_api_key)
                        );
                        let response = build_error_response_with_headers(
                            429,
                            "TOO MANY REQUESTS",
                            "Rate limit exceeded",
                            &[("Retry-After", retry_after_seconds.to_string())],
                        );
                        let _ = stream.write_all(response.as_bytes()).await;
                        let _ = stream.flush().await;
                        return;
                    }
                }
            }

            let result = match route_path {
                "/v1/chat/completions" => {
                    handle_chat_completions(&parsed_request, &mut stream, &config, &default_api_key)
                        .await
                }
                "/v1/completions" => {
                    handle_completions(&parsed_request, &mut stream, &config, &default_api_key)
                        .await
                }
                "/v1/embeddings" => {
                    handle_embeddings(&parsed_request, &mut stream, &config, &default_api_key).await
                }
                "/v1/audio/transcriptions" | "/v1/audio/translations" => {
                    handle_audio(
                        &parsed_request,
                        &mut stream,
                        &config,
                        &default_api_key,
                        route_path,
                    )
                    .await
                }
                _ => Err(RouterError::BadRequest("Unsupported route".to_string())),
            };

            if let Err(err) = result {
                let response = map_error_to_response(&err);
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.flush().await;
            }
        }
        _ => {
            let response = "HTTP/1.1 404 NOT FOUND\r\nContent-Length: 9\r\n\r\nNot Found";
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.flush().await;
        }
    }
}

async fn handle_chat_completions(
    request: &ParsedRequest,
    stream: &mut TcpStream,
    config: &ApiConfig,
    default_api_key: &str,
) -> RouterResult<()> {
    if request.body.is_empty() {
        return Err(RouterError::BadRequest("Empty request body".to_string()));
    }

    let mut chat_request: ChatCompletionRequest =
        serde_json::from_slice(&request.body).map_err(RouterError::from)?;
    chat_request.model = map_model_name(config, &chat_request.model);
    let body_bytes = serde_json::to_vec(&chat_request)
        .map_err(|_| RouterError::BadRequest("Invalid request body".to_string()))?;

    let endpoint = config.endpoint("/v1/chat/completions");
    let upstream_path = compute_upstream_path(&request.target, &endpoint);
    let base_url = normalized_base_url(&config.base_url);
    let method = endpoint.method.as_deref().unwrap_or("POST").to_uppercase();
    let headers = build_upstream_headers(
        config,
        &endpoint,
        &request.headers,
        default_api_key,
        Some("application/json"),
    );

    let is_streaming = chat_request.stream.unwrap_or(false);
    if is_streaming {
        handle_streaming_request(
            stream,
            &base_url,
            &method,
            &upstream_path,
            &headers,
            &body_bytes,
        )
        .await?
    } else {
        let full_url = join_base_and_path(&base_url, &upstream_path);
        let response_body =
            forward_to_upstream(&full_url, &method, &headers, Some(&body_bytes)).await?;
        write_response(stream, "application/json", &response_body).await?;
    }

    Ok(())
}

async fn handle_completions(
    request: &ParsedRequest,
    stream: &mut TcpStream,
    config: &ApiConfig,
    default_api_key: &str,
) -> RouterResult<()> {
    if request.body.is_empty() {
        return Err(RouterError::BadRequest("Empty request body".to_string()));
    }

    let mut completion_request: CompletionRequest =
        serde_json::from_slice(&request.body).map_err(RouterError::from)?;
    completion_request.model = map_model_name(config, &completion_request.model);
    let body_bytes = serde_json::to_vec(&completion_request)
        .map_err(|_| RouterError::BadRequest("Invalid request body".to_string()))?;

    let endpoint = config.endpoint("/v1/completions");
    let upstream_path = compute_upstream_path(&request.target, &endpoint);
    let base_url = normalized_base_url(&config.base_url);
    let method = endpoint.method.as_deref().unwrap_or("POST").to_uppercase();
    let headers = build_upstream_headers(
        config,
        &endpoint,
        &request.headers,
        default_api_key,
        Some("application/json"),
    );

    let is_streaming = completion_request.stream.unwrap_or(false);
    if is_streaming {
        handle_streaming_request(
            stream,
            &base_url,
            &method,
            &upstream_path,
            &headers,
            &body_bytes,
        )
        .await?
    } else {
        let full_url = join_base_and_path(&base_url, &upstream_path);
        let response_body =
            forward_to_upstream(&full_url, &method, &headers, Some(&body_bytes)).await?;
        write_response(stream, "application/json", &response_body).await?;
    }

    Ok(())
}

async fn handle_embeddings(
    request: &ParsedRequest,
    stream: &mut TcpStream,
    config: &ApiConfig,
    default_api_key: &str,
) -> RouterResult<()> {
    if request.body.is_empty() {
        return Err(RouterError::BadRequest("Empty request body".to_string()));
    }

    let mut embedding_request: EmbeddingRequest =
        serde_json::from_slice(&request.body).map_err(RouterError::from)?;
    embedding_request.model = map_model_name(config, &embedding_request.model);
    let body_bytes = serde_json::to_vec(&embedding_request)
        .map_err(|_| RouterError::BadRequest("Invalid request body".to_string()))?;

    let endpoint = config.endpoint("/v1/embeddings");
    let upstream_path = compute_upstream_path(&request.target, &endpoint);
    let base_url = normalized_base_url(&config.base_url);
    let method = endpoint.method.as_deref().unwrap_or("POST").to_uppercase();
    let headers = build_upstream_headers(
        config,
        &endpoint,
        &request.headers,
        default_api_key,
        Some("application/json"),
    );

    let full_url = join_base_and_path(&base_url, &upstream_path);
    let response_body =
        forward_to_upstream(&full_url, &method, &headers, Some(&body_bytes)).await?;
    write_response(stream, "application/json", &response_body).await
}

async fn handle_audio(
    request: &ParsedRequest,
    stream: &mut TcpStream,
    config: &ApiConfig,
    default_api_key: &str,
    route_path: &str,
) -> RouterResult<()> {
    if request.body.is_empty() {
        return Err(RouterError::BadRequest("Empty request body".to_string()));
    }

    let content_type = request
        .headers
        .get("content-type")
        .ok_or_else(|| RouterError::BadRequest("Missing Content-Type header".to_string()))?
        .clone();

    let endpoint = config.endpoint(route_path);
    let upstream_path = compute_upstream_path(&request.target, &endpoint);
    let base_url = normalized_base_url(&config.base_url);
    let method = endpoint.method.as_deref().unwrap_or("POST").to_uppercase();

    let mut body_bytes = request.body.clone();
    if let Some(original_model) = extract_model_from_multipart(&body_bytes) {
        if let Some(mapping) = &config.model_mapping {
            if let Some(target_model) = mapping.get(&original_model) {
                if target_model != &original_model {
                    body_bytes = replace_model_in_multipart(&body_bytes, target_model);
                }
            }
        }
    }

    let headers = build_upstream_headers(
        config,
        &endpoint,
        &request.headers,
        default_api_key,
        Some(&content_type),
    );

    let full_url = join_base_and_path(&base_url, &upstream_path);
    let response_body =
        forward_to_upstream(&full_url, &method, &headers, Some(&body_bytes)).await?;
    write_response(stream, "application/json", &response_body).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_lite::AsyncReadExt;
    use serde_json::json;
    use smol::net::TcpListener;
    use std::collections::HashMap;
    use std::sync::Arc;

    async fn tcp_pair() -> std::io::Result<(TcpStream, TcpStream)> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let addr = listener.local_addr()?;
        let client = TcpStream::connect(addr).await?;
        let (server, _) = listener.accept().await?;
        Ok((server, client))
    }

    #[test]
    fn compute_path_uses_override_and_preserves_query() {
        let endpoint = EndpointConfig {
            upstream_path: Some("/v1/messages".to_string()),
            ..Default::default()
        };
        let result = compute_upstream_path("/v1/chat/completions?foo=bar", &endpoint);
        assert_eq!(result, "/v1/messages?foo=bar");
    }

    #[test]
    fn compute_path_falls_back_to_request_target_when_no_override() {
        let endpoint = EndpointConfig::default();
        let result = compute_upstream_path("/v1/chat/completions?foo=bar", &endpoint);
        assert_eq!(result, "/v1/chat/completions?foo=bar");
    }

    #[test]
    fn compute_path_merges_query_with_configured_query() {
        let endpoint = EndpointConfig {
            upstream_path: Some("/v1/messages?mode=raw".to_string()),
            ..Default::default()
        };
        let result = compute_upstream_path("/v1/chat/completions?foo=bar", &endpoint);
        assert_eq!(result, "/v1/messages?mode=raw&foo=bar");
    }

    #[test]
    fn chat_completions_respects_endpoint_overrides() {
        let expected_body = b"{\"id\":\"anthropic\"}".to_vec();
        let send_called = Arc::new(Mutex::new(false));
        let send_called_clone = Arc::clone(&send_called);

        let response_bytes = with_mock_http_client(
            Box::new(move |url, method, headers, body| {
                *send_called_clone.lock().unwrap() = true;
                assert_eq!(url, "https://api.override/v1/messages?mode=test");
                assert_eq!(method, "PATCH");
                assert_eq!(
                    headers.get("Accept"),
                    Some(&"application/json".to_string())
                );
                let payload: ChatCompletionRequest =
                    serde_json::from_slice(body.expect("body")).unwrap();
                assert_eq!(payload.model, "claude-3-haiku");
                Ok(expected_body.clone())
            }),
            || {
                smol::block_on(async {
                    let config: ApiConfig = serde_json::from_str(
                        r#"{
                            "baseUrl": "https://api.override",
                            "modelMapping": {"gpt-3.5-turbo": "claude-3-haiku"},
                            "endpoints": {
                                "/v1/chat/completions": {
                                    "upstreamPath": "/v1/messages",
                                    "method": "patch",
                                    "headers": {"Accept": "application/json"}
                                }
                            },
                            "port": 8000
                        }"#,
                    )
                    .unwrap();

                    let body = serde_json::json!({
                        "model": "gpt-3.5-turbo",
                        "messages": [
                            {"role": "user", "content": "ping"}
                        ]
                    });
                    let mut headers = HashMap::new();
                    headers.insert(
                        "authorization".to_string(),
                        "Bearer client-key".to_string(),
                    );
                    headers.insert(
                        "content-type".to_string(),
                        "application/json".to_string(),
                    );

                    let parsed_request = ParsedRequest {
                        method: "POST".to_string(),
                        target: "/v1/chat/completions?mode=test".to_string(),
                        version: "HTTP/1.1".to_string(),
                        headers,
                        body: serde_json::to_vec(&body).unwrap(),
                    };

                    let (mut server_stream, mut client_stream) = tcp_pair().await.unwrap();
                    handle_chat_completions(
                        &parsed_request,
                        &mut server_stream,
                        &config,
                        "default-key",
                    )
                    .await
                    .unwrap();
                    drop(server_stream);

                    let mut buf = vec![0u8; 512];
                    let n = client_stream.read(&mut buf).await.unwrap();
                    buf.truncate(n);
                    buf
                })
            },
        );

        let response = String::from_utf8(response_bytes).unwrap();
        assert!(response.contains("\"id\":\"anthropic\""));
        assert!(*send_called.lock().unwrap());
    }

    #[test]
    fn embeddings_route_forwards_with_mocked_upstream() {
        let expected_body = b"{\"ok\":true}".to_vec();
        let send_called = Arc::new(Mutex::new(false));
        let send_called_clone = Arc::clone(&send_called);
        let expected_input = json!("hello");

        let response_bytes = with_mock_http_client(
            Box::new(move |url, method, headers, body| {
                *send_called_clone.lock().unwrap() = true;
                assert_eq!(url, "https://api.test/v1/embeddings");
                assert_eq!(method, "POST");
                assert_eq!(
                    headers.get("Content-Type"),
                    Some(&"application/json".to_string())
                );
                assert_eq!(
                    headers.get("Authorization"),
                    Some(&"Bearer client-key".to_string())
                );
                let payload: EmbeddingRequest =
                    serde_json::from_slice(body.expect("body")).unwrap();
                assert_eq!(payload.model, "qwen3");
                assert_eq!(payload.input, expected_input);
                Ok(expected_body.clone())
            }),
            || {
                smol::block_on(async {
                    let config: ApiConfig = serde_json::from_str(
                        r#"{
                        "baseUrl": "https://api.test",
                        "headers": {"Content-Type": "application/json"},
                        "modelMapping": {"gpt-3.5-turbo": "qwen3"},
                        "endpoints": {"/v1/embeddings": {}},
                        "port": 8000
                    }"#,
                    )
                    .unwrap();

                    let embedding_request = EmbeddingRequest {
                        model: "gpt-3.5-turbo".to_string(),
                        input: json!("hello"),
                        user: None,
                        encoding_format: None,
                        dimensions: None,
                    };
                    let body = serde_json::to_vec(&embedding_request).unwrap();
                    let mut headers = HashMap::new();
                    headers.insert("content-type".to_string(), "application/json".to_string());
                    headers.insert("authorization".to_string(), "Bearer client-key".to_string());

                    let parsed_request = ParsedRequest {
                        method: "POST".to_string(),
                        target: "/v1/embeddings".to_string(),
                        version: "HTTP/1.1".to_string(),
                        headers,
                        body,
                    };

                    let (mut server_stream, mut client_stream) = tcp_pair().await.unwrap();
                    handle_embeddings(&parsed_request, &mut server_stream, &config, "unused-key")
                        .await
                        .unwrap();
                    drop(server_stream);

                    let mut buf = vec![0u8; 512];
                    let n = client_stream.read(&mut buf).await.unwrap();
                    buf.truncate(n);
                    buf
                })
            },
        );

        let response = String::from_utf8(response_bytes).unwrap();
        assert!(response.contains("\"ok\":true"));
        assert!(*send_called.lock().unwrap());
    }

    #[test]
    fn audio_route_rewrites_model_and_forwards() {
        let expected_body = b"{\"text\":\"hi\"}".to_vec();
        let send_called = Arc::new(Mutex::new(false));
        let send_called_clone = Arc::clone(&send_called);
        let boundary = "----router-boundary".to_string();
        let boundary_for_mock = boundary.clone();

        let response_bytes = with_mock_http_client(
            Box::new(move |url, method, headers, body| {
                *send_called_clone.lock().unwrap() = true;
                assert_eq!(url, "https://api.test/v1/audio/transcriptions");
                assert_eq!(method, "POST");
                assert_eq!(
                    headers.get("Content-Type"),
                    Some(&format!(
                        "multipart/form-data; boundary={}",
                        boundary_for_mock
                    ))
                );
                assert_eq!(
                    headers.get("Authorization"),
                    Some(&"Bearer test-key".to_string())
                );
                let payload = std::str::from_utf8(body.expect("body"))
                    .unwrap()
                    .to_string();
                assert!(payload.contains("qwen-voice"));
                assert!(!payload.contains("whisper-1"));
                Ok(expected_body.clone())
            }),
            || {
                let boundary = boundary.clone();
                smol::block_on(async move {
                    let config: ApiConfig = serde_json::from_str(
                        r#"{
                        "baseUrl": "https://api.test",
                        "headers": {"Accept": "application/json"},
                        "modelMapping": {"whisper-1": "qwen-voice"},
                        "endpoints": {
                            "/v1/audio/transcriptions": {"requiresMultipart": true}
                        },
                        "port": 8000
                    }"#,
                    )
                    .unwrap();

                    let body = format!(
                        "--{b}\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\nwhisper-1\r\n--{b}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"audio.wav\"\r\nContent-Type: application/octet-stream\r\n\r\nDATA\r\n--{b}--\r\n",
                        b = boundary
                    )
                    .into_bytes();

                    let mut headers = HashMap::new();
                    headers.insert(
                        "content-type".to_string(),
                        format!("multipart/form-data; boundary={}", boundary),
                    );

                    let parsed_request = ParsedRequest {
                        method: "POST".to_string(),
                        target: "/v1/audio/transcriptions".to_string(),
                        version: "HTTP/1.1".to_string(),
                        headers,
                        body,
                    };

                    let (mut server_stream, mut client_stream) = tcp_pair().await.unwrap();
                    handle_audio(
                        &parsed_request,
                        &mut server_stream,
                        &config,
                        "test-key",
                        "/v1/audio/transcriptions",
                    )
                    .await
                    .unwrap();
                    drop(server_stream);

                    let mut buf = vec![0u8; 512];
                    let n = client_stream.read(&mut buf).await.unwrap();
                    buf.truncate(n);
                    buf
                })
            },
        );

        let response = String::from_utf8(response_bytes).unwrap();
        assert!(response.contains("\"text\":\"hi\""));
        assert!(*send_called.lock().unwrap());
    }
}
