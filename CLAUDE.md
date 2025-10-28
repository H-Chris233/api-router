# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概述

API Router 是一个轻量级的API请求转发服务，将API请求转换为OpenAI兼容格式。该项目使用Rust语言开发，基于smol异步运行时构建。

## 架构设计

- **核心运行时**: 使用smol作为异步运行时，提供轻量级异步服务
- **HTTP客户端**: 基于smol构建的原生HTTP/HTTPS客户端，使用async-tls提供TLS支持
- **连接池**: 使用async-channel实现per-destination连接池，支持HTTP/1.1 keep-alive与TLS会话复用
- **配置管理**: 通过transformer目录中的JSON配置文件动态加载API配置
- **请求处理**: 支持标准请求与SSE流式响应的代理转发
- **速率限制**: 使用dashmap实现并发安全的令牌桶限流器

## 配置文件系统

- **位置**: 配置文件存储在 `transformer/` 目录下
- **格式**: JSON格式，支持多种API提供商配置
- **动态加载**: 启动时根据命令行参数加载对应配置文件
- **支持的配置**:
  - API基础URL设置
  - 请求头配置
  - 模型名称映射

## 主要功能

- 将非标准API请求转换为OpenAI兼容格式
- 自动处理认证头
- 支持模型名称映射
- 支持对话上下文传递

## 开发命令

### 构建项目
```bash
cargo build --release
```

### 运行服务
```bash
# 使用默认配置 (transformer/qwen.json)
cargo run

# 使用指定配置文件 (如 transformer/openai.json)
cargo run -- openai

# 指定端口运行 (默认8000)
cargo run -- qwen 8080

# 使用环境变量设置API密钥
export DEFAULT_API_KEY="your-api-key-here"
cargo run
```

### 测试API
```bash
# 运行测试脚本
./test_api.sh
```

### 测试单个API端点
```bash
# 健康检查
curl -X GET http://localhost:8000/health

# 模型列表
curl -X GET http://localhost:8000/v1/models \
  -H "Authorization: Bearer your-api-key"

# 聊天完成
curl -X POST http://localhost:8000/v1/chat/completions \
  -H "Authorization: Bearer your-api-key" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "qwen3-coder-plus",
    "messages": [{"role": "user", "content": "你好"}],
    "temperature": 0.7
  }'
```

## 代码结构

- `src/main.rs`: 主应用入口，TCP监听器与连接分发
- `src/handlers.rs`: HTTP请求解析、路由处理与请求转发
- `src/http_client.rs`: HTTP/HTTPS客户端实现，支持SSE流式响应与连接池
- `src/config.rs`: 配置文件数据结构定义
- `src/models.rs`: OpenAI兼容的请求/响应模型
- `src/rate_limit.rs`: 令牌桶速率限制器实现
- `src/errors.rs`: 统一错误类型定义
- `transformer/`: 存放API配置文件的目录
  - `qwen.json`: 通义千问API配置
  - `openai.json`: OpenAI API配置
  - 其他提供商配置（anthropic, cohere, gemini等）

## 连接池实现

详见 `CONNECTION_POOL.md` 文档。关键特性：

- **Per-destination池**: 每个(scheme, host, port)独立的连接池
- **HTTP/1.1 keep-alive**: 使用Connection: keep-alive实现连接复用
- **TLS会话复用**: 减少TLS握手开销
- **配置参数**: 
  - `max_size`: 每个池的最大连接数（默认10）
  - `idle_timeout`: 空闲连接超时时间（默认60秒）
- **并发安全**: 使用DashMap + async-channel实现无锁并发
- **自动清理**: 过期连接自动回收
- **错误恢复**: 失败连接自动recycling，不污染连接池

测试与基准：
```bash
# 运行连接池测试
cargo test connection_pool

# 运行连接池性能基准测试
./benchmarks/connection_pool_bench.sh
```

## 端点说明

- `GET /health`: 健康检查端点（包含速率限制指标）
- `GET /v1/models`: 获取可用模型列表
- `POST /v1/chat/completions`: 聊天完成代理（支持流式）
- `POST /v1/completions`: 文本完成代理（支持流式）
- `POST /v1/embeddings`: 文本嵌入代理
- `POST /v1/audio/transcriptions`: 音频转写代理（multipart）
- `POST /v1/audio/translations`: 音频翻译代理（multipart）

## 配置文件示例

配置文件包含以下关键部分：
- `baseUrl`: 目标API的基础URL
- `headers`: 全局请求头配置
- `modelMapping`: 模型名称映射规则
- `endpoints`: 端点级别的配置覆盖（路径、方法、头部、流式支持等）
- `rateLimit`: 全局与端点级别的速率限制配置
- `port`: 监听端口

## 依赖管理原则

项目经过依赖审计与精简，遵循以下原则：

1. **最小化依赖**: 仅保留必需的依赖，移除未使用的crate
2. **特性优化**: 禁用不需要的默认特性，减少编译时间
3. **运行时统一**: 使用smol提供的功能（如`smol::io`）避免重复依赖
4. **定期审计**: 使用`cargo tree`与`cargo udeps`检测冗余依赖

当前核心依赖：
- `smol` - 异步运行时（提供网络、I/O、任务调度）
- `async-tls` + `rustls` - TLS/HTTPS支持
- `async-channel` - 异步通道（连接池队列）
- `serde` + `serde_json` - JSON序列化（已优化特性）
- `url` - URL解析（已禁用默认特性）
- `dashmap` - 并发哈希表（速率限制、连接池索引）
- `once_cell` - 全局单例（`RATE_LIMITER`、`CONNECTION_POOL`）
- `log` + `env_logger` - 日志系统
- `thiserror` - 错误处理

**已移除的依赖**:
- ~~`bytes`~~ - 未使用
- ~~`futures-lite`~~ - 已用`smol::io`替代

详见 `DEPENDENCY_AUDIT.md` 了解完整审计报告。

## 异步运行时决策

经过深度技术评估（详见 `RUNTIME_EVALUATION.md` 和 `RUNTIME_ANALYSIS.md`），项目选择**保持 smol 作为异步运行时**：

### 为什么选择 smol？

1. **已足够轻量**: smol 本质上是 async-executor + async-io + async-net 的薄封装，无显著开销
2. **优秀的性能**: 二进制大小 3.4MB (LTO优化后)，编译时间约 68 秒
3. **完美兼容性**: 与 async-tls/rustls 无缝集成，支持流式传输
4. **简洁 API**: 统一的命名空间和易用接口，降低维护成本

### 评估过的替代方案

| 方案 | 结论 | 理由 |
|------|------|------|
| async-executor + async-io | ❌ 不采纳 | 收益 <3%，增加代码复杂度 |
| tokio | ❌ 不采纳 | 更重（5-6MB），违背轻量化目标 |
| monoio | ❌ 不采纳 | 兼容性差，需重写 I/O 代码 |

### 编译优化配置

项目使用以下编译优化以最小化二进制大小：

```toml
[profile.release]
lto = true              # 链接时优化，实现跨 crate 优化
codegen-units = 1       # 单个代码生成单元，提升优化质量
strip = true            # 自动移除调试符号
```

**优化效果**:
- 二进制从 4.8MB 缩减至 3.4MB (-29%)
- 无功能损失，零业务逻辑改动

### 性能基准

运行 `benchmarks/simple_bench.sh` 可测量：
- 编译时间
- 二进制大小
- 运行时响应延迟
- 资源使用（内存、CPU）

**关键指标** (当前):
- 二进制大小: 3.4 MB (stripped with LTO)
- 编译时间: ~68 秒 (release)
- 启动时间: < 100 ms
- 内存占用: < 20 MB (空闲)