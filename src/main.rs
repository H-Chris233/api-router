mod config;
mod errors;
mod handlers;
mod http_client;
mod metrics;
mod models;
mod rate_limit;

use config::{load_api_config, ApiConfig};
use errors::RouterError;
use handlers::handle_request;

use std::collections::HashMap;
use std::env;
use std::sync::Arc;

use log::{error, info, warn};
use smol::net::TcpListener;

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

fn main() -> smol::io::Result<()> {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .try_init();

    smol::block_on(async {
        let args: Vec<String> = env::args().collect();

        let config = match load_api_config() {
            Ok(cfg) => cfg,
            Err(err) => {
                match &err {
                    RouterError::ConfigParse(message) => {
                        warn!(
                            "Failed to parse config, using default port 8000: {}",
                            message
                        )
                    }
                    RouterError::ConfigRead(message) => {
                        error!(
                            "Failed to load config file ({}). Using default port 8000",
                            message
                        )
                    }
                    other => {
                        error!(
                            "Unexpected configuration error ({}). Using default port 8000",
                            other
                        )
                    }
                }
                Arc::new(default_config())
            }
        };

        let configured_port = config.port;

        let base_port = if args.len() > 2 {
            match args[2].parse::<u16>() {
                Ok(p) => p,
                Err(e) => {
                    warn!(
                        "Invalid port argument '{}': {}. Using configured/default port {}",
                        args[2], e, configured_port
                    );
                    configured_port
                }
            }
        } else {
            configured_port
        };

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
