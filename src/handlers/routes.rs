use crate::config::ApiConfig;
use crate::error_tracking::track_upstream_failure;
use crate::errors::{RouterError, RouterResult};
use crate::http_client::{handle_streaming_request, send_http_request};
use crate::metrics::record_upstream_error;
use crate::models::{
    AnthropicMessagesRequest, ChatCompletionRequest, CompletionRequest, EmbeddingRequest,
};
use crate::tracing_util::{elapsed_ms, extract_provider};
use serde::de::DeserializeOwned;
use serde::Serialize;
use smol::net::TcpStream;
use std::borrow::Cow;
use std::collections::HashMap;
use std::time::Instant;
use tracing::debug;

use super::parser::ParsedRequest;
use super::plan::{map_model_name, prepare_forward_plan};
use super::response;

pub(super) async fn handle_route(
    route_path: &str,
    request: &ParsedRequest,
    stream: &mut TcpStream,
    config: &ApiConfig,
    default_api_key: &str,
    request_id: &str,
) -> RouterResult<()> {
    let result = match route_path {
        "/v1/chat/completions" => {
            forward_json_route::<ChatCompletionRequest>(
                route_path,
                request,
                stream,
                config,
                default_api_key,
                request_id,
                adjust_chat_request,
                Some(chat_should_stream),
            )
            .await
        }
        "/v1/completions" => {
            forward_json_route::<CompletionRequest>(
                route_path,
                request,
                stream,
                config,
                default_api_key,
                request_id,
                adjust_completion_request,
                Some(completion_should_stream),
            )
            .await
        }
        "/v1/embeddings" => {
            forward_json_route::<EmbeddingRequest>(
                route_path,
                request,
                stream,
                config,
                default_api_key,
                request_id,
                adjust_embedding_request,
                None,
            )
            .await
        }
        "/v1/audio/transcriptions" | "/v1/audio/translations" => {
            forward_multipart_route(
                route_path,
                request,
                stream,
                config,
                default_api_key,
                request_id,
            )
            .await
        }
        "/v1/messages" => {
            forward_json_route::<AnthropicMessagesRequest>(
                route_path,
                request,
                stream,
                config,
                default_api_key,
                request_id,
                adjust_anthropic_request,
                Some(anthropic_should_stream),
            )
            .await
        }
        _ => Err(RouterError::BadRequest("Unsupported route".to_string())),
    };

    if let Err(ref err) = result {
        let error_type = match err {
            RouterError::Url(_) => "url_error",
            RouterError::Io(_) => "io_error",
            RouterError::ConfigRead(_) => "config_read_error",
            RouterError::ConfigParse(_) => "config_parse_error",
            RouterError::Json(_) => "json_error",
            RouterError::Upstream(_) => "upstream_error",
            RouterError::Tls(_) => "tls_error",
            RouterError::BadRequest(_) => "bad_request",
        };
        record_upstream_error(error_type);

        // Track upstream failures for alerting
        if matches!(err, RouterError::Upstream(_) | RouterError::Tls(_)) {
            let provider = extract_provider(&config.base_url);
            track_upstream_failure(&provider, err);
        }
    }

    result
}

async fn forward_json_route<T>(
    route_path: &str,
    request: &ParsedRequest,
    stream: &mut TcpStream,
    config: &ApiConfig,
    default_api_key: &str,
    request_id: &str,
    adjust: fn(&ApiConfig, &mut T),
    stream_decider: Option<fn(&T) -> bool>,
) -> RouterResult<()>
where
    T: DeserializeOwned + Serialize,
{
    let upstream_start = Instant::now();
    let provider = extract_provider(&config.base_url);

    let span = tracing::debug_span!(
        "upstream_request",
        request_id = %request_id,
        provider = %provider,
        upstream_latency_ms = tracing::field::Empty,
    );
    let _enter = span.enter();

    if !request.has_body() {
        return Err(RouterError::BadRequest("Empty request body".to_string()));
    }

    let mut payload: T = serde_json::from_slice(request.body())?;
    adjust(config, &mut payload);
    let body_bytes = serde_json::to_vec(&payload)?;

    let plan = prepare_forward_plan(
        route_path,
        request,
        config,
        default_api_key,
        Some("application/json"),
    );

    let should_stream = stream_decider
        .map(|decider| decider(&payload))
        .unwrap_or(false);

    if should_stream {
        debug!("Starting streaming request to upstream");
        handle_streaming_request(
            stream,
            plan.base_url(),
            plan.method(),
            plan.path(),
            plan.headers(),
            &body_bytes,
            plan.stream_config(),
        )
        .await?;
        span.record("upstream_latency_ms", elapsed_ms(upstream_start));
        debug!(
            upstream_latency_ms = elapsed_ms(upstream_start),
            "Streaming request completed"
        );
    } else {
        let full_url = plan.full_url();
        let response_body =
            forward_to_upstream(&full_url, plan.method(), plan.headers(), Some(&body_bytes))
                .await?;
        span.record("upstream_latency_ms", elapsed_ms(upstream_start));
        debug!(
            upstream_latency_ms = elapsed_ms(upstream_start),
            response_size = response_body.len(),
            "Upstream request completed"
        );
        response::write_success(stream, "application/json", &response_body).await?;
    }

    Ok(())
}

async fn forward_multipart_route(
    route_path: &str,
    request: &ParsedRequest,
    stream: &mut TcpStream,
    config: &ApiConfig,
    default_api_key: &str,
    request_id: &str,
) -> RouterResult<()> {
    let upstream_start = Instant::now();
    let provider = extract_provider(&config.base_url);

    let span = tracing::debug_span!(
        "upstream_multipart_request",
        request_id = %request_id,
        provider = %provider,
        upstream_latency_ms = tracing::field::Empty,
    );
    let _enter = span.enter();
    if !request.has_body() {
        return Err(RouterError::BadRequest("Empty request body".to_string()));
    }

    let content_type = request
        .header("content-type")
        .ok_or_else(|| RouterError::BadRequest("Missing Content-Type header".to_string()))?;

    let plan = prepare_forward_plan(
        route_path,
        request,
        config,
        default_api_key,
        Some(content_type),
    );

    let body = rewrite_multipart_model(request.body(), config);
    let full_url = plan.full_url();
    let response_body = forward_to_upstream(
        &full_url,
        plan.method(),
        plan.headers(),
        Some(body.as_ref()),
    )
    .await?;
    span.record("upstream_latency_ms", elapsed_ms(upstream_start));
    debug!(
        upstream_latency_ms = elapsed_ms(upstream_start),
        response_size = response_body.len(),
        "Multipart upstream request completed"
    );
    response::write_success(stream, "application/json", &response_body).await
}

fn adjust_chat_request(config: &ApiConfig, payload: &mut ChatCompletionRequest) {
    payload.model = map_model_name(config, &payload.model);
}

fn adjust_completion_request(config: &ApiConfig, payload: &mut CompletionRequest) {
    payload.model = map_model_name(config, &payload.model);
}

fn adjust_embedding_request(config: &ApiConfig, payload: &mut EmbeddingRequest) {
    payload.model = map_model_name(config, &payload.model);
}

fn chat_should_stream(payload: &ChatCompletionRequest) -> bool {
    payload.stream.unwrap_or(false)
}

fn completion_should_stream(payload: &CompletionRequest) -> bool {
    payload.stream.unwrap_or(false)
}

fn adjust_anthropic_request(config: &ApiConfig, payload: &mut AnthropicMessagesRequest) {
    payload.model = map_model_name(config, &payload.model);
}

fn anthropic_should_stream(payload: &AnthropicMessagesRequest) -> bool {
    payload.stream.unwrap_or(false)
}

fn rewrite_multipart_model<'a>(body: &'a [u8], config: &ApiConfig) -> Cow<'a, [u8]> {
    if let Some(original_model) = extract_model_from_multipart(body) {
        if let Some(mapping) = &config.model_mapping {
            if let Some(target_model) = mapping.get(&original_model) {
                if target_model != &original_model {
                    return Cow::Owned(replace_model_in_multipart(body, target_model));
                }
            }
        }
    }
    Cow::Borrowed(body)
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
            if let Some(ref handler) = *lock.read().unwrap() {
                return (handler)(url, method, headers, body);
            }
        }
    }

    send_http_request(url, method, headers, body).await
}

#[cfg(test)]
type MockHttpHandler = Box<
    dyn Fn(&str, &str, &HashMap<String, String>, Option<&[u8]>) -> RouterResult<Vec<u8>>
        + Send
        + Sync,
>;

#[cfg(test)]
use std::panic;
#[cfg(test)]
use std::sync::{OnceLock, RwLock};

#[cfg(test)]
static HTTP_CLIENT_OVERRIDE: OnceLock<RwLock<Option<MockHttpHandler>>> = OnceLock::new();

#[cfg(test)]
pub(super) fn with_mock_http_client<F, R>(mock: MockHttpHandler, f: F) -> R
where
    F: FnOnce() -> R,
{
    let cell = HTTP_CLIENT_OVERRIDE.get_or_init(|| RwLock::new(None));
    {
        let mut guard = cell.write().unwrap();
        *guard = Some(mock);
    }
    let result = panic::catch_unwind(panic::AssertUnwindSafe(f));
    {
        let mut guard = cell.write().unwrap();
        *guard = None;
    }
    match result {
        Ok(value) => value,
        Err(err) => panic::resume_unwind(err),
    }
}
