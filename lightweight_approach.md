# 轻量级API路由器

这是一个简化版本的API路由器，减少了依赖数量并保持核心功能。

## 简化后的Cargo.toml
```toml
[package]
name = "light-api-router"
version = "0.1.0"
edition = "2024"

[dependencies]
hyper = { version = "1.0", features = ["full"] }
tokio = { version = "1.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
reqwest = { version = "0.11", features = ["json", "stream"], default-features = false }
```

## 简化后的代码结构
- 移除了tower和tower-http依赖
- 移除了UUID生成
- 移除了复杂的日志系统
- 保留了核心的API转发功能
- 简化了配置文件结构
```