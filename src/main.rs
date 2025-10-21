use std::collections::HashMap;
use std::env;
use std::fs;
use std::str;

use serde::{Deserialize, Serialize};
use smol::net::{TcpListener, TcpStream};
use futures_lite::AsyncReadExt;
use futures_lite::AsyncWriteExt;
use url::Url;
use async_tls::TlsConnector;
use rustls::ClientConfig;
use webpki_roots::TLS_SERVER_ROOTS;

// 简化的API配置结构
#[derive(Debug, Clone, Deserialize)]
struct ApiConfig {
    #[serde(rename = "baseUrl")]
    base_url: String,
    headers: HashMap<String, String>,
    #[serde(rename = "modelMapping", default)]
    model_mapping: Option<HashMap<String, String>>,
    #[serde(rename = "port", default = "default_port")]
    port: u16,
}

// 默认端口函数
fn default_port() -> u16 {
    8000
}

// OpenAI兼容的请求结构
#[derive(Debug, Deserialize, Clone, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct ChatCompletionResponse {
    id: String,
    object: String,
    created: u64,
    model: String,
    choices: Vec<Choice>,
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct Choice {
    index: u32,
    message: Message,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct Usage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}


// 发送HTTP/HTTPS请求的辅助函数
async fn send_http_request(
    url: &str,
    method: &str,
    headers: &HashMap<String, String>,
    body: Option<&str>,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let parsed_url = url::Url::parse(url)?;
    let host = parsed_url.host_str().ok_or("Invalid URL: missing host")?;
    let port = parsed_url.port_or_known_default().unwrap_or(if parsed_url.scheme() == "https" { 443 } else { 80 });
    let path_and_query = parsed_url.path();

    let mut tcp_stream = TcpStream::connect((host, port)).await?;

    // 根据协议选择是否使用TLS - 使用类型擦除来处理不同类型的流
    let mut request = String::new();
    request.push_str(&format!("{} {} HTTP/1.1\r\n", method, path_and_query));
    request.push_str(&format!("Host: {}\r\n", host));
    request.push_str("Connection: close\r\n");

    // 添加自定义头部
    for (key, value) in headers {
        request.push_str(&format!("{}: {}\r\n", key, value));
    }

    if let Some(body_str) = body {
        request.push_str(&format!("Content-Length: {}\r\n", body_str.len()));
        request.push_str("\r\n");
        request.push_str(body_str);
    } else {
        request.push_str("\r\n");
    }

    // 根据协议选择不同的处理方式
    if parsed_url.scheme() == "https" {
        let tls_connector = TlsConnector::new();
        let mut tls_stream = tls_connector.connect(host, tcp_stream).await?;
        tls_stream.write_all(request.as_bytes()).await?;
        tls_stream.flush().await?;

        // 读取响应
        let mut response = Vec::new();
        let mut buffer = [0; 4096];
        loop {
            let n = tls_stream.read(&mut buffer).await?;
            if n == 0 {
                break;
            }
            response.extend_from_slice(&buffer[..n]);
        }

        // 解析响应，找到主体部分
        let response_str = String::from_utf8_lossy(&response);
        if let Some(body_start) = response_str.find("\r\n\r\n") {
            Ok(response_str[body_start + 4..].to_string())
        } else {
            Ok(response_str.to_string())
        }
    } else {
        // HTTP请求处理
        tcp_stream.write_all(request.as_bytes()).await?;
        tcp_stream.flush().await?;

        // 读取响应
        let mut response = Vec::new();
        let mut buffer = [0; 4096];
        loop {
            let n = tcp_stream.read(&mut buffer).await?;
            if n == 0 {
                break;
            }
            response.extend_from_slice(&buffer[..n]);
        }

        // 解析响应，找到主体部分
        let response_str = String::from_utf8_lossy(&response);
        if let Some(body_start) = response_str.find("\r\n\r\n") {
            Ok(response_str[body_start + 4..].to_string())
        } else {
            Ok(response_str.to_string())
        }
    }
}

// 处理SSE流式响应的函数
async fn handle_streaming_request(
    client_stream: &mut TcpStream,
    url: &str,
    path: &str,
    headers: &HashMap<String, String>,
    body: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let parsed_url = url::Url::parse(url)?;
    let host = parsed_url.host_str().ok_or("Invalid URL: missing host")?;
    let port = parsed_url.port_or_known_default().unwrap_or(if parsed_url.scheme() == "https" { 443 } else { 80 });

    let mut tcp_stream = TcpStream::connect((host, port)).await?;

    // 根据协议选择不同的处理方式
    if parsed_url.scheme() == "https" {
        let tls_connector = TlsConnector::new();
        let mut tls_stream = tls_connector.connect(host, tcp_stream).await?;

        // 构建流式请求
        let mut request = String::new();
        request.push_str(&format!("POST {} HTTP/1.1\r\n", path));
        request.push_str(&format!("Host: {}\r\n", host));
        request.push_str("Connection: close\r\n");

        // 添加自定义头部
        for (key, value) in headers {
            request.push_str(&format!("{}: {}\r\n", key, value));
        }

        request.push_str(&format!("Content-Length: {}\r\n", body.len()));
        request.push_str("\r\n");
        request.push_str(body);

        tls_stream.write_all(request.as_bytes()).await?;
        tls_stream.flush().await?;

        // 发送SSE响应头给客户端
        let response_headers = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\nX-Accel-Buffering: no\r\n\r\n";
        client_stream.write_all(response_headers.as_bytes()).await?;

        // 转发API响应到客户端
        let mut buffer = [0; 4096];
        loop {
            let n = tls_stream.read(&mut buffer).await?;
            if n == 0 {
                break;
            }

            // 将数据写回客户端
            client_stream.write_all(&buffer[..n]).await?;
            client_stream.flush().await?;
        }
    } else {
        // HTTP请求处理
        let mut tcp_stream = tcp_stream;

        // 构建流式请求
        let mut request = String::new();
        request.push_str(&format!("POST {} HTTP/1.1\r\n", path));
        request.push_str(&format!("Host: {}\r\n", host));
        request.push_str("Connection: close\r\n");

        // 添加自定义头部
        for (key, value) in headers {
            request.push_str(&format!("{}: {}\r\n", key, value));
        }

        request.push_str(&format!("Content-Length: {}\r\n", body.len()));
        request.push_str("\r\n");
        request.push_str(body);

        tcp_stream.write_all(request.as_bytes()).await?;
        tcp_stream.flush().await?;

        // 发送SSE响应头给客户端
        let response_headers = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\nX-Accel-Buffering: no\r\n\r\n";
        client_stream.write_all(response_headers.as_bytes()).await?;

        // 转发API响应到客户端
        let mut buffer = [0; 4096];
        loop {
            let n = tcp_stream.read(&mut buffer).await?;
            if n == 0 {
                break;
            }

            // 将数据写回客户端
            client_stream.write_all(&buffer[..n]).await?;
            client_stream.flush().await?;
        }
    }

    Ok(())
}

async fn handle_request(mut stream: TcpStream, addr: std::net::SocketAddr) {
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
            Err(_) => break,
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

    let response = match (method, path) {
        ("GET", "/health") => {
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"status\": \"ok\", \"message\": \"Light API Router running\"}"
        },
        ("GET", "/v1/models") => {
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"object\": \"list\", \"data\": [{\"id\": \"qwen3-coder-plus\", \"object\": \"model\", \"created\": 1677610602, \"owned_by\": \"organization-owner\"}]}"
        },
        ("POST", "/v1/chat/completions") => {
            // 处理聊天完成请求
            match handle_chat_completions_request(&request, &mut stream).await {
                Ok(_) => return, // 如果是流式请求，handle_chat_completions_request会直接处理并返回
                Err(response) => response,
            }
        },
        _ => {
            "HTTP/1.1 404 NOT FOUND\r\n\r\nNot Found"
        }
    };

    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.flush().await;
}

// 从HTTP头部提取Content-Length
fn extract_content_length(headers: &str) -> Option<usize> {
    for line in headers.lines() {
        if line.to_lowercase().starts_with("content-length:") {
            return line[15..].trim().parse::<usize>().ok();
        }
    }
    None
}

async fn handle_chat_completions_request(request: &str, stream: &mut TcpStream) -> Result<(), &'static str> {
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
    let config_content = fs::read_to_string(&config_file)
        .unwrap_or_else(|_| fs::read_to_string("./transformer/qwen.json").unwrap());

    let config: ApiConfig = match serde_json::from_str(&config_content) {
        Ok(c) => c,
        Err(_) => {
            let response = "HTTP/1.1 500 INTERNAL SERVER ERROR\r\n\r\nFailed to parse config";
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.flush().await;
            return Err(response);
        }
    };

    // 提取请求体
    let body_start = request.find("\r\n\r\n");
    if body_start.is_none() {
        let response = "HTTP/1.1 400 BAD REQUEST\r\n\r\nNo request body";
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.flush().await;
        return Err(response);
    }

    let body = &request[body_start.unwrap() + 4..];
    if body.is_empty() {
        let response = "HTTP/1.1 400 BAD REQUEST\r\n\r\nEmpty request body";
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.flush().await;
        return Err(response);
    }

    // 解析请求
    let mut chat_request: ChatCompletionRequest = match serde_json::from_str(body) {
        Ok(req) => req,
        Err(_) => {
            let response = "HTTP/1.1 400 BAD REQUEST\r\n\r\nInvalid JSON";
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.flush().await;
            return Err(response);
        }
    };

    // 检查是否为流式请求
    let is_streaming = chat_request.stream.unwrap_or(false);

    // 模型名称转换
    if let Some(ref model_mapping) = config.model_mapping {
        if let Some(target_model) = model_mapping.get(&chat_request.model) {
            chat_request.model = target_model.clone();
        }
    }
    // 如果model_mapping为None或映射中没有找到对应模型，则保持原始模型名称（透传）

    // 解析目标URL以获取主机和端口
    let full_url = if config.base_url.starts_with("http://") || config.base_url.starts_with("https://") {
        config.base_url.clone()
    } else {
        format!("https://{}", config.base_url)
    };

    let url = url::Url::parse(&full_url).unwrap();
    let host = url.host_str().unwrap_or("localhost");
    let port = url.port_or_known_default().unwrap_or(80);
    let path_and_query = url.path().to_string();

    // 将请求序列化为JSON
    let json_body = match serde_json::to_string(&chat_request) {
        Ok(json) => json,
        Err(_) => {
            let response = "HTTP/1.1 400 BAD REQUEST\r\n\r\nInvalid request body";
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.flush().await;
            return Err(response);
        }
    };

    // 准备请求头
    let mut request_headers = config.headers.clone();
    request_headers.insert("Authorization".to_string(), format!("Bearer {}", default_api_key));
    request_headers.insert("Content-Type".to_string(), "application/json".to_string());
    request_headers.insert("User-Agent".to_string(), "api-router/1.0".to_string());

    let target_url = format!("{}{}", full_url, path_and_query);

    if is_streaming {
        // 处理流式请求
        match handle_streaming_request(stream, &full_url, "/v1/chat/completions", &request_headers, &json_body).await {
            Ok(_) => Ok(()),
            Err(_) => {
                let response = "HTTP/1.1 502 BAD GATEWAY\r\n\r\nFailed to forward request";
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.flush().await;
                Err(response)
            }
        }
    } else {
        // 处理非流式请求
        let response_body = match send_http_request(&format!("{}{}", full_url, "/v1/chat/completions"), "POST", &request_headers, Some(&json_body)).await {
            Ok(body) => body,
            Err(_) => {
                let response = "HTTP/1.1 502 BAD GATEWAY\r\n\r\nFailed to forward request";
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.flush().await;
                return Err(response);
            }
        };

        // 发送响应
        let response_headers = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{}",
            response_body
        );
        let _ = stream.write_all(response_headers.as_bytes()).await;
        let _ = stream.flush().await;
        Ok(())
    }
}

fn main() -> smol::io::Result<()> {
    smol::block_on(async {
        // 从命令行参数获取配置文件名
        let args: Vec<String> = env::args().collect();
        let config_basename = if args.len() > 1 {
            args[1].clone()
        } else {
            "qwen".to_string()
        };

        // 读取配置文件以获取端口设置
        let config_file = format!("./transformer/{}.json", config_basename);
        let config_content = fs::read_to_string(&config_file)
            .unwrap_or_else(|_| fs::read_to_string("./transformer/qwen.json").unwrap());

        let config: ApiConfig = match serde_json::from_str(&config_content) {
            Ok(c) => c,
            Err(_) => {
                eprintln!("Failed to parse config, using default port 8000");
                ApiConfig {
                    base_url: String::new(),
                    headers: HashMap::new(),
                    model_mapping: None,
                    port: 8000,
                }
            }
        };

        // 从命令行参数获取端口，如果提供则覆盖配置文件中的端口
        let base_port = if args.len() > 2 {
            args[2].parse::<u16>().unwrap_or(config.port)
        } else {
            config.port
        };

        // 端口回退机制：尝试从指定端口开始，最多尝试10个端口
        let mut listener = None;
        let mut used_port = 0;
        for port_offset in 0..10 {
            let port = base_port + port_offset;
            let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));

            match TcpListener::bind(addr).await {
                Ok(l) => {
                    listener = Some(l);
                    used_port = port;
                    break;
                },
                Err(e) => {
                    eprintln!("端口 {} 被占用: {}, 尝试下一个端口", port, e);
                    continue;
                }
            }
        }

        if let Some(listener) = listener {
            println!("API Router 启动在 http://0.0.0.0:{}", used_port);

            loop {
                let (mut stream, addr) = listener.accept().await?;
                smol::spawn(async move {
                    handle_request(stream, addr).await;
                }).detach();
            }
        } else {
            eprintln!("无法绑定到任何端口，从 {} 到 {}", base_port, base_port + 9);
            std::process::exit(1);
        }
    })
}