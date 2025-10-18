use axum::{
    extract::State,
    http::{HeaderMap, Method, Response, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use futures::stream::StreamExt;
use reqwest::header;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// API 配置结构体
#[derive(Debug, Deserialize, Clone)]
struct ApiConfig {
    name: String,
    base_url: String,
    headers: HashMap<String, String>,
    endpoints: HashMap<String, EndpointConfig>,
    model_mapping: HashMap<String, String>,
    request_transforms: TransformConfig,
    response_options: ResponseOptions,
}

#[derive(Debug, Deserialize, Clone)]
struct EndpointConfig {
    method: String,
    headers: HashMap<String, String>,
    #[serde(default = "default_stream_support")]
    stream_support: bool,
    #[serde(default)]
    stream_headers: HashMap<String, String>,
}

fn default_stream_support() -> bool {
    false
}

#[derive(Debug, Deserialize, Clone)]
struct TransformConfig {
    rename_fields: HashMap<String, String>,
    default_values: HashMap<String, String>,
}

#[derive(Debug, Deserialize, Clone)]
struct ResponseOptions {
    forwarded_headers: Vec<String>,
}

// 定义OpenAI兼容的请求和响应结构
#[derive(Debug, Deserialize, Serialize, Clone)]
struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logit_bias: Option<HashMap<String, f32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct Choice {
    pub index: u32,
    pub message: Message,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

// 服务器状态
#[derive(Clone)]
struct AppState {
    client: reqwest::Client,
    api_config: ApiConfig,
    default_api_key: String,
}

#[tokio::main]
async fn main() {
    // 初始化日志
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "api_router=debug,tower_http=debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // 从环境变量获取配置
    let default_api_key = std::env::var("DEFAULT_API_KEY")
        .unwrap_or_else(|_| "j88R1cKdHY1EcYk9hO5vJIrV3f4rrtI5I9NuFyyTiFLDCXRhY8ooddL72AT1NqyHKMf3iGvib2W9XBYV8duUtw".to_string());
    
    // 从环境变量获取配置文件名，默认为qwen
    let config_file = std::env::var("API_CONFIG_FILE")
        .unwrap_or_else(|_| "qwen.json".to_string());
    
    // 读取配置文件
    let config_path = format!("./transformer/{}", config_file);
    let config_content = fs::read_to_string(&config_path)
        .expect(&format!("无法读取配置文件: {}", config_path));
    
    let api_config: ApiConfig = serde_json::from_str(&config_content)
        .expect(&format!("无法解析配置文件: {}", config_path));
    
    // 创建HTTP客户端
    let client = reqwest::Client::builder()
        .build()
        .expect("Failed to create HTTP client");

    // 创建应用状态
    let app_state = AppState {
        client,
        api_config,
        default_api_key,
    };

    // 构建路由
    let app = Router::new()
        // OpenAI兼容的聊天完成端点
        .route("/v1/chat/completions", post(proxy_chat_completions))
        // 健康检查端点
        .route("/health", get(health_check))
        // 其他可能的OpenAI兼容端点
        .route("/v1/models", get(proxy_models))
        .with_state(app_state.clone())
        // 添加CORS中间件
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE, Method::OPTIONS])
                .allow_headers(Any),
        );

    // 运行服务器
    let addr = SocketAddr::from(([0, 0, 0, 0], 8000));
    tracing::info!("API Router 启动在 http://{}", addr);
    tracing::info!("目标API基础URL: {}", app_state.api_config.base_url);
    tracing::info!("使用配置文件: {}", config_file);
    
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// 健康检查端点
async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "message": "API Router 服务正常运行"
    }))
}

// 代理模型列表
async fn proxy_models(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let target_url = format!("{}/v1/models", state.api_config.base_url);
    
    // 获取端点配置，如果没有找到则使用默认配置
    let default_config = EndpointConfig {
        method: "GET".to_string(),
        headers: HashMap::new(),
        stream_support: false,
        stream_headers: HashMap::new(),
    };
    let endpoint_config = state.api_config.endpoints.get("/v1/models").unwrap_or(&default_config);
    
    // 构建目标请求
    let mut builder = state.client.get(&target_url);
    
    // 添加全局配置头
    for (key, value) in &state.api_config.headers {
        builder = builder.header(key, value);
    }
    
    // 添加端点特定头
    for (key, value) in &endpoint_config.headers {
        builder = builder.header(key, value);
    }
    
    // 转发认证头
    if let Some(auth_header) = headers.get("authorization") {
        // 转换 header::HeaderValue 到 reqwest::header::HeaderValue
        if let Ok(header_value) = header::HeaderValue::from_str(auth_header.to_str().map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?) {
            builder = builder.header("Authorization", header_value);
        }
    } else {
        // 如果请求没有认证头，使用默认API密钥
        builder = builder.header("Authorization", format!("Bearer {}", state.default_api_key));
    }
    
    // 发送请求
    let response = builder
        .send()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // 将 reqwest::StatusCode 转换为 axum::http::StatusCode
    let axum_status = StatusCode::from_u16(status.as_u16())
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Invalid status code".to_string()))?;

    Ok((axum_status, body))
}

// 代理聊天完成请求
async fn proxy_chat_completions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<ChatCompletionRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // 检查是否需要流式响应
    if payload.stream.unwrap_or(false) {
        let result = proxy_chat_completions_streaming(state, headers, payload).await;
        Ok(result?.into_response())
    } else {
        let result = proxy_chat_completions_standard(state, headers, payload).await;
        Ok(result?.into_response())
    }
}

// 标准（非流式）聊天完成代理
async fn proxy_chat_completions_standard(
    state: AppState,
    headers: HeaderMap,
    payload: ChatCompletionRequest,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let target_url = format!("{}/v1/chat/completions", state.api_config.base_url);
    
    // 获取端点配置，如果没有找到则使用默认配置
    let default_config = EndpointConfig {
        method: "POST".to_string(),
        headers: HashMap::new(),
        stream_support: true,
        stream_headers: HashMap::new(),
    };
    let endpoint_config = state.api_config.endpoints.get("/v1/chat/completions").unwrap_or(&default_config);
    
    // 转换请求格式（如果需要）
    let transformed_payload = transform_request(payload, &state.api_config).await;
    
    // 构建目标请求
    let mut builder = state.client.post(&target_url);
    
    // 添加全局配置头
    for (key, value) in &state.api_config.headers {
        builder = builder.header(key, value);
    }
    
    // 添加端点特定头
    for (key, value) in &endpoint_config.headers {
        builder = builder.header(key, value);
    }
    
    // 转发认证头，如果请求中没有则使用默认API密钥
    if let Some(auth_header) = headers.get("authorization") {
        if let Ok(header_value) = header::HeaderValue::from_str(auth_header.to_str().map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?) {
            builder = builder.header("Authorization", header_value);
        }
    } else {
        // 如果请求没有认证头，使用默认API密钥
        builder = builder.header("Authorization", format!("Bearer {}", state.default_api_key));
    }
    
    // 发送请求
    let response = builder
        .json(&transformed_payload)
        .send()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let status = response.status();
    let response_headers = response.headers().clone();
    let body = response
        .text()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // 将 reqwest::StatusCode 转换为 axum::http::StatusCode
    let axum_status = StatusCode::from_u16(status.as_u16())
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Invalid status code".to_string()))?;

    // 构建响应
    let mut result = Response::builder()
        .status(axum_status);
    
    // 根据配置决定要转发的响应头
    let forwarded_headers = &state.api_config.response_options.forwarded_headers;
    
    // 转发响应头
    for (name, value) in response_headers.iter() {
        if let Ok(axum_name) = axum::http::HeaderName::from_bytes(name.as_str().as_bytes()) {
            if let Ok(axum_value) = axum::http::HeaderValue::from_str(value.to_str().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?) {
                // 检查是否在配置的转发列表中，或是否为以x-开头的自定义头，或是否为必要头
                let should_forward = forwarded_headers.iter().any(|h| h.eq_ignore_ascii_case(name.as_str())) ||
                    name.as_str().starts_with("x-") ||
                    name.as_str() == "content-type" || name.as_str() == "content-length";
                
                if should_forward {
                    result = result.header(axum_name, axum_value);
                }
            }
        }
    }

    Ok(result
        .body(axum::body::Body::from(body))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?)
}

// 流式聊天完成代理
async fn proxy_chat_completions_streaming(
    state: AppState,
    headers: HeaderMap,
    payload: ChatCompletionRequest,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let target_url = format!("{}/v1/chat/completions", state.api_config.base_url);
    
    // 获取端点配置，如果没有找到则使用默认配置
    let default_config = EndpointConfig {
        method: "POST".to_string(),
        headers: HashMap::new(),
        stream_support: true,
        stream_headers: HashMap::new(),
    };
    let endpoint_config = state.api_config.endpoints.get("/v1/chat/completions").unwrap_or(&default_config);
    
    // 转换请求格式
    let transformed_payload = transform_request(payload, &state.api_config).await;
    
    // 构建目标请求
    let mut builder = state.client.post(&target_url);
    
    // 添加全局配置头
    for (key, value) in &state.api_config.headers {
        builder = builder.header(key, value);
    }
    
    // 添加端点特定头
    for (key, value) in &endpoint_config.headers {
        builder = builder.header(key, value);
    }
    
    // 添加流式特定头
    for (key, value) in &endpoint_config.stream_headers {
        builder = builder.header(key, value);
    }
    
    // 转发认证头
    if let Some(auth_header) = headers.get("authorization") {
        if let Ok(header_value) = header::HeaderValue::from_str(auth_header.to_str().map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?) {
            builder = builder.header("Authorization", header_value);
        }
    } else {
        // 如果请求没有认证头，使用默认API密钥
        builder = builder.header("Authorization", format!("Bearer {}", state.default_api_key));
    }
    
    // 发送请求
    let response = builder
        .json(&transformed_payload)
        .send()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let status = response.status();
    if !status.is_success() {
        let error_body = response
            .text()
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let axum_status = StatusCode::from_u16(status.as_u16())
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Invalid status code".to_string()))?;
        return Err((axum_status, error_body));
    }

    // 获取响应体流
    let stream = response.bytes_stream();
    
    // 创建SSE响应流
    let sse_stream = stream.map(|result| {
        match result {
            Ok(bytes) => {
                // 直接传递原始字节，保持SSE格式
                Ok::<_, std::io::Error>(bytes)
            }
            Err(e) => {
                Err(std::io::Error::new(std::io::ErrorKind::Other, e))
            }
        }
    });

    // 创建SSE响应
    use axum::body::Body;
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .header("connection", "keep-alive")
        .header("x-accel-buffering", "no") // 禁用nginx缓冲
        .body(Body::from_stream(sse_stream))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?)
}

// 请求转换函数 - 根据配置文件转换格式
async fn transform_request(request: ChatCompletionRequest, config: &ApiConfig) -> ChatCompletionRequest {
    let mut transformed_request = request;
    
    // 模型名称转换
    if let Some(target_model) = config.model_mapping.get(&transformed_request.model) {
        transformed_request.model = target_model.clone();
    }
    
    // 根据配置进行字段重命名
    for (from, to) in &config.request_transforms.rename_fields {
        if from == "max_tokens" && to == "max_completion_tokens" {
            // 这里需要特殊处理，因为字段类型可能不同
            // 现在只处理模型名称映射，更复杂的转换可以后续扩展
        }
    }
    
    // 可以添加其他转换逻辑
    // 例如，根据目标API的要求调整参数
    
    transformed_request
}

// SSE数据结构
#[derive(Debug, Serialize)]
struct SSEData {
    id: String,
    object: String,
    created: u64,
    model: String,
    choices: Vec<ChoiceDelta>,
}

#[derive(Debug, Serialize)]
struct ChoiceDelta {
    index: u32,
    delta: Delta,
    finish_reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct Delta {
    role: Option<String>,
    content: Option<String>,
}
