use std::collections::HashMap;
use std::env;
use std::fs;
use std::net::SocketAddr;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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

// HTTP客户端
static HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn get_client() -> &'static reqwest::Client {
    HTTP_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .build()
            .expect("Failed to create HTTP client")
    })
}

async fn handle_request(mut stream: tokio::net::TcpStream, addr: SocketAddr) {
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

    // 构建目标请求
    let target_url = format!("{}/v1/chat/completions", config.base_url);

    // 构建HTTP请求
    let mut builder = get_client().post(&target_url);

    // 添加配置头
    for (key, value) in &config.headers {
        builder = builder.header(key, value);
    }

    // 添加认证头
    builder = builder.header("Authorization", format!("Bearer {}", default_api_key));

    // 发送请求
    let response = match builder.json(&chat_request).send().await {
        Ok(resp) => resp,
        Err(_) => {
            return "HTTP/1.1 502 BAD GATEWAY\r\n\r\nFailed to forward request";
        }
    };

    let status = response.status();
    let response_body = match response.text().await {
        Ok(body) => body,
        Err(_) => {
            return "HTTP/1.1 502 BAD GATEWAY\r\n\r\nFailed to read response";
        }
    };

    let response_headers = format!(
        "HTTP/1.1 {}\r\nContent-Type: application/json\r\n\r\n{}",
        status.as_u16(),
        response_body
    );

    Box::leak(response_headers.into_boxed_str())
}

#[tokio::main]
async fn main() {
    // 从命令行参数获取端口，默认为8000
    let args: Vec<String> = env::args().collect();
    let port = if args.len() > 2 {
        args[2].parse::<u16>().unwrap_or(8000)
    } else {
        8000
    };

    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    let listener = TcpListener::bind(addr).await.unwrap();
    println!("Light API Router 启动在 http://{}", addr);

    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                tokio::spawn(async move {
                    handle_request(stream, addr).await;
                });
            }
            Err(e) => eprintln!("Error accepting connection: {:?}", e),
        }
    }
}