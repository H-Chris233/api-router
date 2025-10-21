use std::collections::HashMap;
use crate::errors::{RouterResult, RouterError};
use crate::models::{ChatCompletionRequest};
use crate::config::ApiConfig;
use crate::http_client::{send_http_request, handle_streaming_request};
use futures_lite::AsyncReadExt;
use futures_lite::AsyncWriteExt;
use log::{debug, warn};
use smol::net::TcpStream;
use std::env;
use std::fs;

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
    let body = serde_json::json!({
        "error": {
            "message": message,
        }
    }).to_string();
    format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\n\r\n{}",
        status_code, reason, body
    )
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
        RouterError::Io(msg) => build_error_response(500, "INTERNAL SERVER ERROR", &msg.to_string()),
        RouterError::Json(msg) => build_error_response(400, "BAD REQUEST", &msg.to_string()),
    }
}

pub async fn handle_request(mut stream: TcpStream, addr: std::net::SocketAddr) {
    debug!("New connection from {}", addr);

    // 读取完整的HTTP请求（可能需要多次读取）
    let mut request_bytes = Vec::new();
    let mut buffer = [0; 4096];

    // 读取直到遇到请求体结束或达到超时
    for _ in 0..1000 { // 限制读取次数避免无限循环
        match stream.read(&mut buffer).await {
            Ok(0) => break, // 连接到达EOF
            Ok(n) => {
                request_bytes.extend_from_slice(&buffer[..n]);

                // 检查是否已读取完整的HTTP头部（即双CRLF：\r\n\r\n）
                let request_str = String::from_utf8_lossy(&request_bytes);
                if let Some(body_start) = request_str.find("\r\n\r\n") {
                    let header_end = body_start + 4;

                    // 检查Content-Length头部来确定是否已读取完整请求体
                    let headers = &request_str[..body_start];
                    if let Some(content_length) = extract_content_length(headers) {
                        if request_bytes.len() >= header_end + content_length {
                            break; // 已读取完整请求
                        }
                    } else {
                        // 如果没有Content-Length，假设请求已完成（适用于GET等无请求体的请求）
                        break;
                    }
                } else {
                    // 如果没有找到请求体分隔符，继续读取
                    continue;
                }
            }
            Err(e) => {
                warn!("Failed to read from {}: {}", addr, e);
                break
            },
        }
    }

    let request = String::from_utf8_lossy(&request_bytes);
    let request_lines: Vec<&str> = request.lines().collect();

    if request_lines.is_empty() {
        return;
    }

    let request_line = request_lines[0];
    let parts: Vec<&str> = request_line.split_whitespace().collect();

    if parts.len() < 2 {
        return;
    }

    let method = parts[0];
    let path = parts[1];

    match (method, path) {
        ("GET", "/health") => {
            let response = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"status\": \"ok\", \"message\": \"Light API Router running\"}";
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.flush().await;
            return;
        }
        ("GET", "/v1/models") => {
            let response = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"object\": \"list\", \"data\": [{\"id\": \"qwen3-coder-plus\", \"object\": \"model\", \"created\": 1677610602, \"owned_by\": \"organization-owner\"}]}";
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.flush().await;
            return;
        }
        ("POST", "/v1/chat/completions") => {
            // 处理聊天完成请求
            match handle_chat_completions_request(&request, &mut stream).await {
                Ok(_) => return, // 如果是流式请求，handle_chat_completions_request会直接处理并返回
                Err(err) => {
                    let response = map_error_to_response(&err);
                    let _ = stream.write_all(response.as_bytes()).await;
                    let _ = stream.flush().await;
                    return;
                }
            }
        }
        _ => {
            let response = "HTTP/1.1 404 NOT FOUND\r\n\r\nNot Found";
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.flush().await;
            return;
        }
    }
}

async fn handle_chat_completions_request(request: &str, stream: &mut TcpStream) -> RouterResult<()> {
    // 从环境变量获取配置
    let default_api_key = env::var("DEFAULT_API_KEY")
        .unwrap_or_else(|_| "j88R1cKdHY1EcYk9hO5vJIrV3f4rrtI5I9NuFyyTiFLDCXRhY8ooddL72AT1NqyHKMf3iGvib2W9XBYV8duUtw".to_string());

    // 读取配置文件
    let args: Vec<String> = env::args().collect();
    let config_basename = if args.len() > 1 {
        args[1].clone()
    } else {
        "qwen".to_string()
    };
    let config_file = format!("./transformer/{}.json", config_basename);
    let config_content = fs::read_to_string(&config_file).or_else(|e| {
        warn!("Failed to read config {}: {}. Falling back to transformer/qwen.json", config_file, e);
        fs::read_to_string("./transformer/qwen.json").map_err(|e2| RouterError::ConfigRead(e2.to_string()))
    })?;

    let mut config: ApiConfig = serde_json::from_str(&config_content)
        .map_err(|e| RouterError::ConfigParse(e.to_string()))?;

    // 提取请求体
    let body_start = request.find("\r\n\r\n");
    let body = match body_start {
        Some(pos) => &request[pos + 4..],
        None => {
            return Err(RouterError::BadRequest("No request body".to_string()));
        }
    };

    if body.is_empty() {
        return Err(RouterError::BadRequest("Empty request body".to_string()));
    }

    // 解析请求
    let mut chat_request: ChatCompletionRequest = serde_json::from_str(body)?;

    // 检查是否为流式请求
    let is_streaming = chat_request.stream.unwrap_or(false);

    // 模型名称转换
    if let Some(ref model_mapping) = config.model_mapping {
        if let Some(target_model) = model_mapping.get(&chat_request.model) {
            chat_request.model = target_model.clone();
        }
    }
    // 如果model_mapping为None或映射中没有找到对应模型，则保持原始模型名称（透传）

    // 解析目标URL
    let full_url = if config.base_url.starts_with("http://") || config.base_url.starts_with("https://") {
        config.base_url.clone()
    } else {
        format!("https://{}", config.base_url)
    };

    // 将请求序列化为JSON
    let json_body = serde_json::to_string(&chat_request)
        .map_err(|_| RouterError::BadRequest("Invalid request body".to_string()))?;

    // 准备请求头
    let mut request_headers = config.headers.clone();
    request_headers.insert("Authorization".to_string(), format!("Bearer {}", default_api_key));
    request_headers.insert("Content-Type".to_string(), "application/json".to_string());
    request_headers.insert("User-Agent".to_string(), "api-router/1.0".to_string());

    if is_streaming {
        // 处理流式请求
        handle_streaming_request(stream, &full_url, "/v1/chat/completions", &request_headers, &json_body).await?;
        Ok(())
    } else {
        // 处理非流式请求
        let response_body = send_http_request(&format!("{}{}", full_url, "/v1/chat/completions"), "POST", &request_headers, Some(&json_body)).await?;

        // 发送响应
        let response_headers = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{}",
            response_body
        );
        stream.write_all(response_headers.as_bytes()).await?;
        stream.flush().await?;
        Ok(())
    }
}