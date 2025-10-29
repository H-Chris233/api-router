//! API Router 库模块
//! 
//! 提供 API 转发服务的核心功能，包括：
//! - 配置管理
//! - HTTP 客户端和连接池
//! - 速率限制
//! - 错误处理和追踪
//! - 指标收集
//! - OpenAI 兼容的数据模型

pub mod config;
pub mod error_tracking;
pub mod errors;
pub mod handlers;
pub mod http_client;
pub mod metrics;
pub mod models;
pub mod rate_limit;
pub mod tracing_util;

pub use http_client::PoolConfig;

#[cfg(test)]
mod tracing_tests;
