mod common;

use common::*;
use serde_json::json;
use std::thread;
use std::time::Duration;

#[test]
fn forwards_chat_completion_payload_with_model_mapping() {
    let upstream = MockProvider::builder()
        .route(
            "/v1/messages",
            MockResponse::json(
                200,
                json!({
                    "id": "mock-1",
                    "object": "chat.completion",
                    "choices": [
                        {"index": 0, "message": {"role": "assistant", "content": "pong"}}
                    ]
                }),
            ),
        )
        .build();

    let router_port = pick_free_port();
    let config = ConfigFixture::provider("anthropic")
        .with_base_url(&upstream.base_url())
        .with_port(router_port)
        .into_temp_file();

    let _router = RouterProcess::start(config.path(), router_port, &[]);

    let client_key = "Bearer client-key-123";
    let body = json!({
        "model": "gpt-4o",
        "messages": [
            {"role": "user", "content": "ping"}
        ],
        "stream": false
    });
    let payload = serde_json::to_vec(&body).expect("failed to encode request");

    let response = send_http_request(
        router_port,
        "POST",
        "/v1/chat/completions",
        &[("Authorization", client_key), ("Content-Type", "application/json")],
        Some(&payload),
    );

    assert_eq!(response.status, 200);
    let echoed: serde_json::Value = serde_json::from_slice(&response.body).expect("valid json");
    assert_eq!(echoed["choices"][0]["message"]["content"], "pong");

    let recorded = upstream.received_requests();
    assert_eq!(recorded.len(), 1, "expected single upstream request");
    let request = &recorded[0];
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/v1/messages");
    assert_eq!(
        request.headers.get("authorization").map(|s| s.as_str()),
        Some(client_key)
    );
    assert!(
        request
            .headers
            .get("accept")
            .map(|value| value.contains("text/event-stream"))
            .unwrap_or(false),
        "expected upstream accept header to include SSE"
    );

    let upstream_body: serde_json::Value =
        serde_json::from_slice(&request.body).expect("valid upstream payload");
    assert_eq!(upstream_body["model"], "claude-3-opus-20240229");
}

#[test]
fn enforces_route_level_rate_limiting() {
    let upstream = MockProvider::builder()
        .route(
            "/v1/chat",
            MockResponse::json(200, json!({
                "id": "rate-limit-ok",
                "object": "chat.completion",
                "choices": [
                    {"index": 0, "message": {"role": "assistant", "content": "first"}}
                ]
            })),
        )
        .build();

    let router_port = pick_free_port();
    let config_fixture = ConfigFixture::provider("cohere")
        .with_base_url(&upstream.base_url())
        .with_port(router_port)
        .set_endpoint_rate_limit("/v1/chat/completions", 1, 1);
    let config_file = config_fixture.into_temp_file();

    let _router = RouterProcess::start(config_file.path(), router_port, &[]);

    let payload = serde_json::to_vec(&json!({
        "model": "gpt-4o",
        "messages": [{"role": "user", "content": "hello"}],
    }))
    .unwrap();

    let headers = [
        ("Authorization", "Bearer rl-test"),
        ("Content-Type", "application/json"),
    ];

    let first = send_http_request(
        router_port,
        "POST",
        "/v1/chat/completions",
        &headers,
        Some(&payload),
    );
    assert_eq!(first.status, 200, "first request should succeed");

    let second = send_http_request(
        router_port,
        "POST",
        "/v1/chat/completions",
        &headers,
        Some(&payload),
    );
    assert_eq!(second.status, 429, "second request should be rate limited");
    assert!(
        second
            .header("retry-after")
            .map(|value| value.parse::<u64>().is_ok())
            .unwrap_or(false),
        "rate limited response should include Retry-After header"
    );

    let requests = upstream.received_requests();
    assert_eq!(requests.len(), 1, "rate limiter should block second upstream call");
}

#[test]
fn hot_reload_picks_up_updated_config() {
    let upstream_a = MockProvider::builder()
        .route(
            "/v1/messages",
            MockResponse::json(200, json!({"id": "provider-a", "choices": []})),
        )
        .build();
    let upstream_b = MockProvider::builder()
        .route(
            "/v1/messages",
            MockResponse::json(200, json!({"id": "provider-b", "choices": []})),
        )
        .build();

    let router_port = pick_free_port();
    let config_file = ConfigFixture::provider("anthropic")
        .with_base_url(&upstream_a.base_url())
        .with_port(router_port)
        .into_temp_file();

    let _router = RouterProcess::start(config_file.path(), router_port, &[]);

    let payload = serde_json::to_vec(&json!({
        "model": "gpt-4o",
        "messages": [{"role": "user", "content": "ping"}],
    }))
    .unwrap();
    let headers = [
        ("Authorization", "Bearer reload"),
        ("Content-Type", "application/json"),
    ];

    let initial = send_http_request(
        router_port,
        "POST",
        "/v1/chat/completions",
        &headers,
        Some(&payload),
    );
    assert_eq!(initial.status, 200);
    let initial_body: serde_json::Value = serde_json::from_slice(&initial.body).unwrap();
    assert_eq!(initial_body["id"], "provider-a");

    thread::sleep(Duration::from_millis(150));
    let updated_value = ConfigFixture::provider("anthropic")
        .with_base_url(&upstream_b.base_url())
        .with_port(router_port)
        .into_value();
    config_file.rewrite(&updated_value);

    thread::sleep(Duration::from_millis(250));

    let updated = send_http_request(
        router_port,
        "POST",
        "/v1/chat/completions",
        &headers,
        Some(&payload),
    );
    assert_eq!(updated.status, 200);
    let updated_body: serde_json::Value = serde_json::from_slice(&updated.body).unwrap();
    assert_eq!(updated_body["id"], "provider-b", "router should use reloaded config");

    let requests_a = upstream_a.received_requests();
    let requests_b = upstream_b.received_requests();
    assert_eq!(requests_a.len(), 1, "first request hits provider A");
    assert_eq!(requests_b.len(), 1, "after reload router should contact provider B");
}

#[test]
fn forwards_streaming_sse_events() {
    let upstream = MockProvider::builder()
        .route(
            "/v1beta/openai/chat/completions",
            MockResponse::stream(
                200,
                vec![("Content-Type", "text/event-stream"), ("X-Upstream", "gemini")],
                vec![
                    StreamChunk::new(b"data: {\"delta\":\"one\"}\n\n"),
                    StreamChunk::new(b"data: {\"delta\":\"two\"}\n\n").with_delay(Duration::from_millis(50)),
                    StreamChunk::new(b"data: [DONE]\n\n"),
                ],
            ),
        )
        .build();

    let router_port = pick_free_port();
    let config = ConfigFixture::provider("gemini")
        .with_base_url(&upstream.base_url())
        .with_port(router_port)
        .into_temp_file();

    let _router = RouterProcess::start(config.path(), router_port, &[]);

    let payload = serde_json::to_vec(&json!({
        "model": "gpt-4o",
        "messages": [{"role": "user", "content": "stream please"}],
        "stream": true
    }))
    .unwrap();

    let response = send_http_request(
        router_port,
        "POST",
        "/v1/chat/completions",
        &[
            ("Authorization", "Bearer streaming"),
            ("Content-Type", "application/json"),
            ("Accept", "text/event-stream"),
        ],
        Some(&payload),
    );

    assert_eq!(response.status, 200);
    assert_eq!(
        response.header("content-type"),
        Some("text/event-stream"),
        "router should expose SSE content type"
    );
    assert_eq!(
        response.header("x-accel-buffering"),
        Some("no"),
        "router should disable buffering for SSE"
    );
    let body = response.body_utf8();
    assert!(body.contains("data: {\"delta\":\"one\"}"));
    assert!(body.contains("data: {\"delta\":\"two\"}"));
    assert!(body.contains("data: [DONE]"));

    let upstream_requests = upstream.received_requests();
    assert_eq!(upstream_requests.len(), 1);
    let request = &upstream_requests[0];
    assert_eq!(request.path, "/v1beta/openai/chat/completions");
    let forwarded: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(forwarded["model"], "gemini-1.5-pro");
    assert_eq!(forwarded["stream"], true);
}
