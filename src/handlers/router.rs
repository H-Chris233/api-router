use crate::config::load_api_config;
use crate::metrics::{
    gather_metrics, observe_request_latency, record_request, update_rate_limiter_buckets,
    ConnectionGuard,
};
use crate::rate_limit::{resolve_rate_limit_settings, RateLimitDecision, RATE_LIMITER};
use log::{debug, warn};
use serde_json::json;
use smol::io::{AsyncReadExt, AsyncWriteExt};
use smol::net::TcpStream;
use std::net::SocketAddr;
use std::time::Instant;

use super::parser::{
    anonymize_key, extract_client_api_key, extract_content_length, parse_http_request,
    resolve_default_api_key,
};
use super::response::{build_error_response_with_headers, map_error_to_response, write_success};
use super::routes::handle_route;

pub async fn handle_request(mut stream: TcpStream, addr: SocketAddr) {
    let _connection_guard = ConnectionGuard::new();
    let start_time = Instant::now();
    debug!("New connection from {}", addr);

    let mut request_bytes = Vec::new();
    let mut buffer = [0u8; 4096];

    for _ in 0..1000 {
        match stream.read(&mut buffer).await {
            Ok(0) => break,
            Ok(n) => {
                request_bytes.extend_from_slice(&buffer[..n]);
                if let Some(pos) = request_bytes
                    .windows(4)
                    .position(|window| window == b"\r\n\r\n")
                {
                    let header_end = pos + 4;
                    if let Ok(headers_str) = std::str::from_utf8(&request_bytes[..pos]) {
                        if let Some(content_length) = extract_content_length(headers_str) {
                            if request_bytes.len() >= header_end + content_length {
                                break;
                            }
                        } else {
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
            let _ = stream.write_all(&response).await;
            let _ = stream.flush().await;
            let latency = start_time.elapsed().as_secs_f64();
            observe_request_latency("/unknown", latency);
            record_request("/unknown", "UNKNOWN", 400);
            return;
        }
    };

    let route_path = parsed_request.route_path();

    match (parsed_request.method(), route_path) {
        ("GET", "/health") => {
            let snapshot = RATE_LIMITER.snapshot();
            update_rate_limiter_buckets(snapshot.active_buckets);
            let payload = json!({
                "status": "ok",
                "message": "Light API Router running",
                "rateLimiter": {
                    "activeBuckets": snapshot.active_buckets,
                    "routes": snapshot.routes,
                }
            });
            if let Ok(body) = serde_json::to_vec(&payload) {
                let _ = write_success(&mut stream, "application/json", &body).await;
            }
            let latency = start_time.elapsed().as_secs_f64();
            observe_request_latency("/health", latency);
            record_request("/health", "GET", 200);
        }
        ("GET", "/metrics") => {
            let snapshot = RATE_LIMITER.snapshot();
            update_rate_limiter_buckets(snapshot.active_buckets);
            match gather_metrics() {
                Ok(metrics_output) => {
                    let _ = write_success(&mut stream, "text/plain; version=0.0.4", metrics_output.as_bytes()).await;
                    let latency = start_time.elapsed().as_secs_f64();
                    observe_request_latency("/metrics", latency);
                    record_request("/metrics", "GET", 200);
                }
                Err(e) => {
                    warn!("Failed to gather metrics: {}", e);
                    let response = b"HTTP/1.1 500 INTERNAL SERVER ERROR\r\nContent-Length: 21\r\n\r\nFailed to get metrics";
                    let _ = stream.write_all(response).await;
                    let _ = stream.flush().await;
                    let latency = start_time.elapsed().as_secs_f64();
                    observe_request_latency("/metrics", latency);
                    record_request("/metrics", "GET", 500);
                }
            }
        }
        ("GET", "/v1/models") => {
            let body = b"{\"object\": \"list\", \"data\": [{\"id\": \"qwen3-coder-plus\", \"object\": \"model\", \"created\": 1677610602, \"owned_by\": \"organization-owner\"}]}";
            let _ = write_success(&mut stream, "application/json", body).await;
            let latency = start_time.elapsed().as_secs_f64();
            observe_request_latency("/v1/models", latency);
            record_request("/v1/models", "GET", 200);
        }
        ("POST", "/v1/chat/completions")
        | ("POST", "/v1/completions")
        | ("POST", "/v1/embeddings")
        | ("POST", "/v1/audio/transcriptions")
        | ("POST", "/v1/audio/translations")
        | ("POST", "/v1/messages") => {
            let config = match load_api_config() {
                Ok(cfg) => cfg,
                Err(err) => {
                    let response = map_error_to_response(&err);
                    let _ = stream.write_all(&response).await;
                    let _ = stream.flush().await;
                    let latency = start_time.elapsed().as_secs_f64();
                    observe_request_latency(route_path, latency);
                    record_request(route_path, "POST", 500);
                    return;
                }
            };
            let default_api_key = resolve_default_api_key();
            let client_api_key = extract_client_api_key(parsed_request.headers(), &default_api_key);

            if let Some(settings) = resolve_rate_limit_settings(route_path, config.as_ref()) {
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
                        let _ = stream.write_all(&response).await;
                        let _ = stream.flush().await;
                        let latency = start_time.elapsed().as_secs_f64();
                        observe_request_latency(route_path, latency);
                        record_request(route_path, "POST", 429);
                        return;
                    }
                }
            }

            let result = handle_route(
                route_path,
                &parsed_request,
                &mut stream,
                config.as_ref(),
                &default_api_key,
            )
            .await;

            let latency = start_time.elapsed().as_secs_f64();
            observe_request_latency(route_path, latency);

            if let Err(err) = result {
                let response = map_error_to_response(&err);
                let _ = stream.write_all(&response).await;
                let _ = stream.flush().await;
                record_request(route_path, "POST", 500);
            } else {
                record_request(route_path, "POST", 200);
            }
        }
        _ => {
            let response = b"HTTP/1.1 404 NOT FOUND\r\nContent-Length: 9\r\n\r\nNot Found";
            let _ = stream.write_all(response).await;
            let _ = stream.flush().await;
            let latency = start_time.elapsed().as_secs_f64();
            observe_request_latency(route_path, latency);
            record_request(route_path, parsed_request.method(), 404);
        }
    }
}
