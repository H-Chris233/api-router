use std::collections::HashMap;
use std::env;
use std::fs;
use std::io;
use std::str;

use serde::{Deserialize, Serialize};
use smol::net::{TcpListener, TcpStream};
use futures_lite::AsyncReadExt;
use futures_lite::AsyncWriteExt;
use url::Url;

// 简化的API配置结构
#[derive(Debug, Clone, Deserialize)]
struct ApiConfig {
    base_url: String,
    headers: HashMap<String, String>,
    model_mapping: HashMap<String, String>,
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

// 发送HTTP请求的辅助函数
async fn send_http_request(
    host: &str,
    port: u16,
    path: &str,
    method: &str,
    headers: &HashMap<String, String>,
    body: Option<&str>,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let mut stream = TcpStream::connect((host, port)).await?;

    let mut request = String::new();
    request.push_str(&format!("{} {} HTTP/1.1\r\n", method, path));
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

    stream.write_all(request.as_bytes()).await?;

    // 读取响应
    let mut response = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        let n = stream.read(&mut buffer).await?;
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

async fn handle_request(mut stream: TcpStream, addr: std::net::SocketAddr) {
    let mut buffer = [0; 1024];
    let _ = stream.read(&mut buffer).await;

    let request = String::from_utf8_lossy(&buffer[..]);
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
            handle_chat_completions_request(&request).await
        },
        _ => {
            "HTTP/1.1 404 NOT FOUND\r\n\r\nNot Found"
        }
    };

    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.flush().await;
}

async fn handle_chat_completions_request(request: &str) -> &'static str {
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
            return "HTTP/1.1 500 INTERNAL SERVER ERROR\r\n\r\nFailed to parse config";
        }
    };

    // 提取请求体
    let body_start = request.find("\r\n\r\n");
    if body_start.is_none() {
        return "HTTP/1.1 400 BAD REQUEST\r\n\r\nNo request body";
    }

    let body = &request[body_start.unwrap() + 4..];
    if body.is_empty() {
        return "HTTP/1.1 400 BAD REQUEST\r\n\r\nEmpty request body";
    }

    // 解析请求
    let mut chat_request: ChatCompletionRequest = match serde_json::from_str(body) {
        Ok(req) => req,
        Err(_) => {
            return "HTTP/1.1 400 BAD REQUEST\r\n\r\nInvalid JSON";
        }
    };

    // 模型名称转换
    if let Some(target_model) = config.model_mapping.get(&chat_request.model) {
        chat_request.model = target_model.clone();
    }

    // 解析目标URL以获取主机和端口
    let url = url::Url::parse(&format!("http://{}", config.base_url.replace("https://", "").replace("http://", "")).as_str()).unwrap();
    let host = url.host_str().unwrap_or("localhost");
    let port = url.port().unwrap_or(80);

    // 将请求序列化为JSON
    let json_body = match serde_json::to_string(&chat_request) {
        Ok(json) => json,
        Err(_) => {
            return "HTTP/1.1 400 BAD REQUEST\r\n\r\nInvalid request body";
        }
    };

    // 准备请求头
    let mut request_headers = config.headers.clone();
    request_headers.insert("Authorization".to_string(), format!("Bearer {}", default_api_key));
    request_headers.insert("Content-Type".to_string(), "application/json".to_string());
    request_headers.insert("User-Agent".to_string(), "api-router/1.0".to_string());

    // 发送HTTP请求
    let response_body = match send_http_request(host, port, "/v1/chat/completions", "POST", &request_headers, Some(&json_body)).await {
        Ok(body) => body,
        Err(_) => {
            return "HTTP/1.1 502 BAD GATEWAY\r\n\r\nFailed to forward request";
        }
    };

    // 由于我们无法从自定义HTTP请求函数中获取状态码，我们假设请求成功
    let response_headers = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{}",
        response_body
    );

    Box::leak(response_headers.into_boxed_str())
}

fn main() -> smol::io::Result<()> {
    smol::block_on(async {
        // 从命令行参数获取端口，默认为8000
        let args: Vec<String> = env::args().collect();
        let port = if args.len() > 2 {
            args[2].parse::<u16>().unwrap_or(8000)
        } else {
            8000
        };

        let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));

        let listener = TcpListener::bind(addr).await?;
        println!("API Router 启动在 http://{}", addr);

        loop {
            let (stream, addr) = listener.accept().await?;
            smol::spawn(async move {
                handle_request(stream, addr).await;
            }).detach();
        }
    })
}