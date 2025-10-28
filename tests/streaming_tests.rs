mod common;

use common::*;
use serde_json::json;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;
use std::time::Duration;

#[test]
fn streaming_preserves_chunk_order_and_content() {
    let upstream = MockProvider::builder()
        .route(
            "/v1/chat/completions",
            MockResponse::stream(
                200,
                vec![("Content-Type", "text/event-stream")],
                vec![
                    StreamChunk::new(b"data: {\"id\":\"chunk-1\",\"delta\":\"Hello\"}\n\n"),
                    StreamChunk::new(b"data: {\"id\":\"chunk-2\",\"delta\":\" \"}\n\n")
                        .with_delay(Duration::from_millis(10)),
                    StreamChunk::new(b"data: {\"id\":\"chunk-3\",\"delta\":\"world\"}\n\n")
                        .with_delay(Duration::from_millis(10)),
                    StreamChunk::new(b"data: [DONE]\n\n").with_delay(Duration::from_millis(10)),
                ],
            ),
        )
        .build();

    let router_port = pick_free_port();
    let config = ConfigFixture::provider("qwen")
        .with_base_url(&upstream.base_url())
        .with_port(router_port)
        .into_temp_file();

    let _router = RouterProcess::start(config.path(), router_port, &[]);

    let payload = serde_json::to_vec(&json!({
        "model": "qwen3-coder-plus",
        "messages": [{"role": "user", "content": "test"}],
        "stream": true
    }))
    .unwrap();

    let response = send_http_request(
        router_port,
        "POST",
        "/v1/chat/completions",
        &[
            ("Authorization", "Bearer test-key"),
            ("Content-Type", "application/json"),
        ],
        Some(&payload),
    );

    assert_eq!(response.status, 200);
    assert_eq!(response.header("content-type"), Some("text/event-stream"));

    let body = response.body_utf8();
    assert!(body.contains("chunk-1"));
    assert!(body.contains("chunk-2"));
    assert!(body.contains("chunk-3"));
    assert!(body.contains("[DONE]"));

    let chunk1_pos = body.find("chunk-1").unwrap();
    let chunk2_pos = body.find("chunk-2").unwrap();
    let chunk3_pos = body.find("chunk-3").unwrap();
    assert!(chunk1_pos < chunk2_pos, "chunks should arrive in order");
    assert!(chunk2_pos < chunk3_pos, "chunks should arrive in order");
}

#[test]
fn streaming_sends_heartbeats_on_slow_upstream() {
    let upstream = MockProvider::builder()
        .route(
            "/v1/chat/completions",
            MockResponse::stream(
                200,
                vec![("Content-Type", "text/event-stream")],
                vec![
                    StreamChunk::new(b"data: {\"delta\":\"start\"}\n\n"),
                    StreamChunk::new(b"data: {\"delta\":\"end\"}\n\n")
                        .with_delay(Duration::from_millis(3500)),
                    StreamChunk::new(b"data: [DONE]\n\n"),
                ],
            ),
        )
        .build();

    let router_port = pick_free_port();
    let mut config_value = ConfigFixture::provider("qwen")
        .with_base_url(&upstream.base_url())
        .with_port(router_port)
        .into_value();

    config_value["streamConfig"] = json!({
        "bufferSize": 4096,
        "heartbeatIntervalSecs": 2
    });

    let config_file = ConfigFixture::from_value(config_value).into_temp_file();
    let _router = RouterProcess::start(config_file.path(), router_port, &[]);

    let payload = serde_json::to_vec(&json!({
        "model": "qwen3-coder-plus",
        "messages": [{"role": "user", "content": "test"}],
        "stream": true
    }))
    .unwrap();

    let response = send_http_request(
        router_port,
        "POST",
        "/v1/chat/completions",
        &[
            ("Authorization", "Bearer test-key"),
            ("Content-Type", "application/json"),
        ],
        Some(&payload),
    );

    assert_eq!(response.status, 200);
    let body = response.body_utf8();
    assert!(body.contains("start"));
    assert!(body.contains("end"));
    assert!(body.contains("[DONE]"));
    assert!(
        body.contains(": heartbeat"),
        "should contain heartbeat comment during slow upstream"
    );
}

#[test]
fn streaming_uses_custom_buffer_size() {
    let upstream = MockProvider::builder()
        .route(
            "/v1/chat/completions",
            MockResponse::stream(
                200,
                vec![("Content-Type", "text/event-stream")],
                vec![
                    StreamChunk::new(vec![b'x'; 2048].as_slice()),
                    StreamChunk::new(b"data: [DONE]\n\n"),
                ],
            ),
        )
        .build();

    let router_port = pick_free_port();
    let mut config_value = ConfigFixture::provider("qwen")
        .with_base_url(&upstream.base_url())
        .with_port(router_port)
        .into_value();

    config_value["streamConfig"] = json!({
        "bufferSize": 1024,
        "heartbeatIntervalSecs": 30
    });

    let config_file = ConfigFixture::from_value(config_value).into_temp_file();
    let _router = RouterProcess::start(config_file.path(), router_port, &[]);

    let payload = serde_json::to_vec(&json!({
        "model": "qwen3-coder-plus",
        "messages": [{"role": "user", "content": "test"}],
        "stream": true
    }))
    .unwrap();

    let response = send_http_request(
        router_port,
        "POST",
        "/v1/chat/completions",
        &[
            ("Authorization", "Bearer test-key"),
            ("Content-Type", "application/json"),
        ],
        Some(&payload),
    );

    assert_eq!(response.status, 200);
    let body = response.body;
    let x_count = body.iter().filter(|&&b| b == b'x').count();
    assert!(
        x_count >= 2048,
        "should receive at least 2048 x bytes, got {}",
        x_count
    );
    assert!(body_utf8(&body).contains("[DONE]"));
}

fn body_utf8(body: &[u8]) -> String {
    String::from_utf8_lossy(body).into_owned()
}

#[test]
fn streaming_handles_client_early_disconnect_gracefully() {
    let upstream = MockProvider::builder()
        .route(
            "/v1/chat/completions",
            MockResponse::stream(
                200,
                vec![("Content-Type", "text/event-stream")],
                vec![
                    StreamChunk::new(b"data: {\"delta\":\"chunk1\"}\n\n"),
                    StreamChunk::new(b"data: {\"delta\":\"chunk2\"}\n\n")
                        .with_delay(Duration::from_millis(500)),
                    StreamChunk::new(b"data: {\"delta\":\"chunk3\"}\n\n")
                        .with_delay(Duration::from_millis(500)),
                    StreamChunk::new(b"data: [DONE]\n\n").with_delay(Duration::from_millis(500)),
                ],
            ),
        )
        .build();

    let router_port = pick_free_port();
    let config = ConfigFixture::provider("qwen")
        .with_base_url(&upstream.base_url())
        .with_port(router_port)
        .into_temp_file();

    let _router = RouterProcess::start(config.path(), router_port, &[]);

    let payload = serde_json::to_vec(&json!({
        "model": "qwen3-coder-plus",
        "messages": [{"role": "user", "content": "test"}],
        "stream": true
    }))
    .unwrap();

    let mut stream =
        TcpStream::connect(("127.0.0.1", router_port)).expect("failed to connect to router");
    stream.set_nodelay(true).ok();

    let request = format!(
        "POST /v1/chat/completions HTTP/1.1\r\n\
         Host: 127.0.0.1:{}\r\n\
         Authorization: Bearer test-key\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         \r\n",
        router_port,
        payload.len()
    );
    stream.write_all(request.as_bytes()).unwrap();
    stream.write_all(&payload).unwrap();
    stream.flush().unwrap();

    let mut buffer = vec![0u8; 512];
    let n = stream.read(&mut buffer).unwrap();
    let initial_response = String::from_utf8_lossy(&buffer[..n]);
    assert!(initial_response.contains("200 OK"));

    thread::sleep(Duration::from_millis(100));
    drop(stream);

    thread::sleep(Duration::from_millis(1500));
}

#[test]
fn streaming_supports_endpoint_specific_config() {
    let upstream = MockProvider::builder()
        .route(
            "/v1/chat/completions",
            MockResponse::stream(
                200,
                vec![("Content-Type", "text/event-stream")],
                vec![
                    StreamChunk::new(b"data: {\"delta\":\"test\"}\n\n"),
                    StreamChunk::new(b"data: [DONE]\n\n").with_delay(Duration::from_millis(2500)),
                ],
            ),
        )
        .build();

    let router_port = pick_free_port();
    let mut config_value = ConfigFixture::provider("qwen")
        .with_base_url(&upstream.base_url())
        .with_port(router_port)
        .into_value();

    config_value["streamConfig"] = json!({
        "bufferSize": 8192,
        "heartbeatIntervalSecs": 10
    });

    config_value["endpoints"]["/v1/chat/completions"]["streamConfig"] = json!({
        "bufferSize": 2048,
        "heartbeatIntervalSecs": 1
    });

    let config_file = ConfigFixture::from_value(config_value).into_temp_file();
    let _router = RouterProcess::start(config_file.path(), router_port, &[]);

    let payload = serde_json::to_vec(&json!({
        "model": "qwen3-coder-plus",
        "messages": [{"role": "user", "content": "test"}],
        "stream": true
    }))
    .unwrap();

    let response = send_http_request(
        router_port,
        "POST",
        "/v1/chat/completions",
        &[
            ("Authorization", "Bearer test-key"),
            ("Content-Type", "application/json"),
        ],
        Some(&payload),
    );

    assert_eq!(response.status, 200);
    let body = response.body_utf8();
    assert!(body.contains("test"));
    assert!(body.contains("[DONE]"));
    assert!(
        body.contains(": heartbeat"),
        "should use endpoint-specific heartbeat interval of 1 second"
    );
}

#[test]
fn streaming_handles_large_chunks_with_backpressure() {
    let large_chunk = vec![b'A'; 32768];
    let upstream = MockProvider::builder()
        .route(
            "/v1/chat/completions",
            MockResponse::stream(
                200,
                vec![("Content-Type", "text/event-stream")],
                vec![
                    StreamChunk::new(large_chunk.clone()),
                    StreamChunk::new(b"data: [DONE]\n\n"),
                ],
            ),
        )
        .build();

    let router_port = pick_free_port();
    let mut config_value = ConfigFixture::provider("qwen")
        .with_base_url(&upstream.base_url())
        .with_port(router_port)
        .into_value();

    config_value["streamConfig"] = json!({
        "bufferSize": 4096,
        "heartbeatIntervalSecs": 30
    });

    let config_file = ConfigFixture::from_value(config_value).into_temp_file();
    let _router = RouterProcess::start(config_file.path(), router_port, &[]);

    let payload = serde_json::to_vec(&json!({
        "model": "qwen3-coder-plus",
        "messages": [{"role": "user", "content": "test"}],
        "stream": true
    }))
    .unwrap();

    let response = send_http_request(
        router_port,
        "POST",
        "/v1/chat/completions",
        &[
            ("Authorization", "Bearer test-key"),
            ("Content-Type", "application/json"),
        ],
        Some(&payload),
    );

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body.iter().filter(|&&b| b == b'A').count(),
        32768,
        "all data should be received despite small buffer"
    );
    assert!(response.body_utf8().contains("[DONE]"));
}

#[test]
fn streaming_completions_endpoint_works() {
    let upstream = MockProvider::builder()
        .route(
            "/v1/completions",
            MockResponse::stream(
                200,
                vec![("Content-Type", "text/event-stream")],
                vec![
                    StreamChunk::new(b"data: {\"text\":\"Once\"}\n\n"),
                    StreamChunk::new(b"data: {\"text\":\" upon\"}\n\n")
                        .with_delay(Duration::from_millis(10)),
                    StreamChunk::new(b"data: [DONE]\n\n"),
                ],
            ),
        )
        .build();

    let router_port = pick_free_port();
    let config = ConfigFixture::provider("qwen")
        .with_base_url(&upstream.base_url())
        .with_port(router_port)
        .into_temp_file();

    let _router = RouterProcess::start(config.path(), router_port, &[]);

    let payload = serde_json::to_vec(&json!({
        "model": "qwen3-coder-plus",
        "prompt": "Once upon a time",
        "stream": true
    }))
    .unwrap();

    let response = send_http_request(
        router_port,
        "POST",
        "/v1/completions",
        &[
            ("Authorization", "Bearer test-key"),
            ("Content-Type", "application/json"),
        ],
        Some(&payload),
    );

    assert_eq!(response.status, 200);
    assert_eq!(response.header("content-type"), Some("text/event-stream"));
    let body = response.body_utf8();
    assert!(body.contains("Once"));
    assert!(body.contains("upon"));
    assert!(body.contains("[DONE]"));
}
