use super::parser::{extract_content_length, ParsedRequest};
use super::plan::compute_upstream_path;
use super::response::build_error_response_with_headers;
use super::routes::{handle_route, with_mock_http_client};
use crate::config::{ApiConfig, EndpointConfig};
use crate::models::{ChatCompletionRequest, EmbeddingRequest};
use serde_json::json;
use serial_test::serial;
use smol::io::AsyncReadExt;
use smol::net::{TcpListener, TcpStream};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

async fn tcp_pair() -> std::io::Result<(TcpStream, TcpStream)> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let addr = listener.local_addr()?;
    let client = TcpStream::connect(addr).await?;
    let (server, _) = listener.accept().await?;
    Ok((server, client))
}

#[test]
fn parsed_request_extracts_route_path() {
    let mut headers = HashMap::new();
    headers.insert("content-type".to_string(), "application/json".to_string());
    let request = ParsedRequest::new_for_tests(
        "POST",
        "/v1/chat/completions?foo=bar",
        "HTTP/1.1",
        headers,
        b"{}".to_vec(),
    );
    assert_eq!(request.route_path(), "/v1/chat/completions");
}

#[test]
fn extract_content_length_reads_header_value() {
    let headers = "Content-Length: 123\r\nOther: value";
    assert_eq!(extract_content_length(headers), Some(123));
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
fn error_response_sets_length_header() {
    let response = build_error_response_with_headers(
        400,
        "BAD REQUEST",
        "Invalid",
        &[("X-Test", "true".to_string())],
    );
    let response_str = String::from_utf8(response).expect("valid utf8");
    assert!(response_str.contains("Content-Length:"));
    assert!(response_str.contains("\r\nX-Test: true\r\n"));
}

#[test]
#[serial]
fn chat_completions_respects_endpoint_overrides() {
    let expected_body = b"{\"id\":\"anthropic\"}".to_vec();
    let send_called = Arc::new(Mutex::new(false));
    let send_called_clone = Arc::clone(&send_called);

    let response_bytes = with_mock_http_client(
        Box::new(move |url, method, headers, body| {
            *send_called_clone.lock().unwrap() = true;
            assert_eq!(url, "https://api.override/v1/messages?mode=test");
            assert_eq!(method, "PATCH");
            assert_eq!(headers.get("Accept"), Some(&"application/json".to_string()));
            let payload: ChatCompletionRequest = serde_json::from_slice(body.expect("body")).unwrap();
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
                headers.insert("authorization".to_string(), "Bearer client-key".to_string());
                headers.insert("content-type".to_string(), "application/json".to_string());

                let parsed_request = ParsedRequest::new_for_tests(
                    "POST",
                    "/v1/chat/completions?mode=test",
                    "HTTP/1.1",
                    headers,
                    serde_json::to_vec(&body).unwrap(),
                );

                let (mut server_stream, mut client_stream) = tcp_pair().await.unwrap();
                handle_route(
                    "/v1/chat/completions",
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
#[serial]
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
            assert_eq!(headers.get("Content-Type"), Some(&"application/json".to_string()));
            assert_eq!(headers.get("Authorization"), Some(&"Bearer client-key".to_string()));
            let payload: EmbeddingRequest = serde_json::from_slice(body.expect("body")).unwrap();
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

                let parsed_request = ParsedRequest::new_for_tests(
                    "POST",
                    "/v1/embeddings",
                    "HTTP/1.1",
                    headers,
                    body,
                );

                let (mut server_stream, mut client_stream) = tcp_pair().await.unwrap();
                handle_route(
                    "/v1/embeddings",
                    &parsed_request,
                    &mut server_stream,
                    &config,
                    "unused-key",
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
    assert!(response.contains("\"ok\":true"));
    assert!(*send_called.lock().unwrap());
}

#[test]
#[serial]
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
                Some(&format!("multipart/form-data; boundary={}", boundary_for_mock))
            );
            assert_eq!(
                headers.get("Authorization"),
                Some(&"Bearer test-key".to_string())
            );
            let payload = std::str::from_utf8(body.expect("body")).unwrap().to_string();
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
                    "--{boundary}\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\nwhisper-1\r\n--{boundary}--\r\n"
                )
                .into_bytes();

                let mut headers = HashMap::new();
                headers.insert(
                    "content-type".to_string(),
                    format!("multipart/form-data; boundary={}", boundary)
                );
                headers.insert("authorization".to_string(), "Bearer test-key".to_string());

                let parsed_request = ParsedRequest::new_for_tests(
                    "POST",
                    "/v1/audio/transcriptions",
                    "HTTP/1.1",
                    headers,
                    body,
                );

                let (mut server_stream, mut client_stream) = tcp_pair().await.unwrap();
                handle_route(
                    "/v1/audio/transcriptions",
                    &parsed_request,
                    &mut server_stream,
                    &config,
                    "fallback-key",
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
