mod config;
mod error_tracking;
mod errors;
mod handlers;
mod http_client;
mod metrics;
mod models;
mod rate_limit;
mod tracing_util;

#[cfg(test)]
mod tracing_tests;

use config::{load_api_config, ApiConfig};
use error_tracking::{init_sentry, SentryConfig};
use errors::RouterError;
use handlers::handle_request;

use std::collections::HashMap;
use std::env;
use std::sync::Arc;

use smol::net::TcpListener;
use tracing::{error, info, warn};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// 创建一个默认的 API 配置
fn default_config() -> ApiConfig {
    ApiConfig {
        base_url: String::new(),
        headers: HashMap::new(),
        model_mapping: None,
        endpoints: HashMap::new(),
        port: 8000,
        rate_limit: None,
        stream_config: None,
    }
}

/// 初始化追踪和日志系统
/// 
/// 根据 LOG_FORMAT 环境变量决定日志格式：
/// - "json": 输出 JSON 格式日志
/// - 其他: 输出人类可读格式
/// 
/// 日志级别通过 RUST_LOG 环境变量控制，默认为 info
fn init_tracing() {
    let use_json = env::var("LOG_FORMAT")
        .unwrap_or_default()
        .eq_ignore_ascii_case("json");

    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap();

    if use_json {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt::layer().json())
            .init();
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt::layer())
            .init();
    }
}

fn main() -> smol::io::Result<()> {
    init_tracing();

    // 初始化 Sentry 错误追踪
    let sentry_config = SentryConfig::from_env();
    let _sentry_guard = init_sentry(&sentry_config);

    smol::block_on(async {
        let args: Vec<String> = env::args().collect();

        // 加载配置文件，如果失败则使用默认配置
        let config = match load_api_config() {
            Ok(cfg) => cfg,
            Err(err) => {
                match &err {
                    RouterError::ConfigParse(message) => {
                        warn!(
                            "配置解析失败，使用默认端口 8000: {}",
                            message
                        )
                    }
                    RouterError::ConfigRead(message) => {
                        error!(
                            "配置文件加载失败 ({}). 使用默认端口 8000",
                            message
                        )
                    }
                    other => {
                        error!(
                            "意外的配置错误 ({}). 使用默认端口 8000",
                            other
                        )
                    }
                }
                Arc::new(default_config())
            }
        };

        let configured_port = config.port;

        // 解析命令行参数中的端口号
        let base_port = if args.len() > 2 {
            match args[2].parse::<u16>() {
                Ok(p) => p,
                Err(e) => {
                    warn!(
                        "端口参数无效 '{}': {}. 使用配置/默认端口 {}",
                        args[2], e, configured_port
                    );
                    configured_port
                }
            }
        } else {
            configured_port
        };

        // 尝试绑定端口，如果失败则尝试下一个端口（最多尝试 10 次）
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

            // 主循环：接受连接并为每个连接创建异步任务
            loop {
                let (stream, addr) = match listener.accept().await {
                    Ok(s) => s,
                    Err(e) => {
                        error!("接受连接失败: {}", e);
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
