mod config;
mod errors;
mod handlers;
mod http_client;
mod models;
mod rate_limit;

use config::ApiConfig;
use handlers::handle_request;

use std::collections::HashMap;
use std::env;
use std::fs;

use log::{error, info, warn};
use smol::net::TcpListener;

fn main() -> smol::io::Result<()> {
    // 初始化日志
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .try_init();

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
        let config_content = match fs::read_to_string(&config_file) {
            Ok(c) => c,
            Err(e) => {
                warn!(
                    "Failed to read config {}: {}. Falling back to transformer/qwen.json",
                    config_file, e
                );
                match fs::read_to_string("./transformer/qwen.json") {
                    Ok(c2) => c2,
                    Err(e2) => {
                        error!("Failed to read fallback config transformer/qwen.json: {}. Using default port 8000", e2);
                        String::from("{\"port\":8000,\"baseUrl\":\"\",\"headers\":{}}")
                    }
                }
            }
        };

        let config: ApiConfig = match serde_json::from_str(&config_content) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to parse config, using default port 8000: {}", e);
                ApiConfig {
                    base_url: String::new(),
                    headers: HashMap::new(),
                    model_mapping: None,
                    endpoints: HashMap::new(),
                    port: 8000,
                    rate_limit: None,
                }
            }
        };

        // 从命令行参数获取端口，如果提供则覆盖配置文件中的端口
        let base_port = if args.len() > 2 {
            match args[2].parse::<u16>() {
                Ok(p) => p,
                Err(e) => {
                    warn!(
                        "Invalid port argument '{}': {}. Using configured/default port {}",
                        args[2], e, config.port
                    );
                    config.port
                }
            }
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
                }
                Err(e) => {
                    warn!("端口 {} 被占用: {}, 尝试下一个端口", port, e);
                    continue;
                }
            }
        }

        if let Some(listener) = listener {
            info!("API Router 启动在 http://0.0.0.0:{}", used_port);

            loop {
                let (stream, addr) = match listener.accept().await {
                    Ok(s) => s,
                    Err(e) => {
                        error!("Failed to accept connection: {}", e);
                        continue;
                    }
                };
                smol::spawn(async move {
                    handle_request(stream, addr).await;
                })
                .detach();
            }
        } else {
            error!("无法绑定到任何端口，从 {} 到 {}", base_port, base_port + 9);
            std::process::exit(1);
        }
    })
}
