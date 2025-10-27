# API Router

一个将 API 请求转发为 OpenAI 兼容格式的轻量级服务，特别适用于将具有特殊认证或签名要求的 API 转换为标准 OpenAI 客户端可以直接使用的格式。

## 功能特性

- 将非标准 API 请求转换为 OpenAI 兼容格式
- **高性能流式传输（SSE）**：
  - 增量读写，避免积累整个响应
  - 支持反压机制（backpressure）
  - 可配置的缓冲区大小
  - 心跳保活（heartbeat keep-alive）
  - 客户端断连时优雅关闭
- **结构化日志与延迟监控**：
  - 基于 `tracing` 的请求级别跟踪（request ID、client IP、路由、方法）
  - 自动测量总请求延迟与上游延迟
  - 支持 JSON 格式输出，可集成 Datadog、Elasticsearch、Grafana Loki
  - 可配置的日志级别和过滤器（通过 `RUST_LOG` 环境变量）
- 自动处理认证头、User-Agent 以及基础请求头
- 支持模型名称映射（client model ➜ provider model）
- 支持 `/v1/chat/completions`、`/v1/completions`、`/v1/embeddings`、`/v1/audio/transcriptions`、`/v1/audio/translations` 等 OpenAI 风格端点
- **支持 Anthropic Messages API（`/v1/messages`）**：原生支持 Anthropic 风格的请求格式，包括 system 提示、max_tokens 参数以及流式响应
- 自动处理音频转写/翻译请求的 multipart/form-data 载荷
- 动态加载 transformer 目录中的 JSON 配置文件
- 支持基于 API Key 与路由粒度的令牌桶限流，超限时返回 429 并暴露健康指标
- **Prometheus 指标集成**：通过 `/metrics` 端点暴露请求计数、延迟分布、活跃连接数、上游错误等指标，便于监控和告警

## 安装与运行

### 依赖

- Rust 1.70.0 或更高版本

#### 核心依赖说明

本项目经过依赖精简优化，仅保留以下必要依赖：

**运行时与异步支持**：
- `smol` (v2) - 轻量级异步运行时，提供完整的 async/await 支持及 I/O 扩展

**序列化与配置**：
- `serde` (v1.0, features: derive) - 数据序列化框架
- `serde_json` (v1.0, 仅 std 特性) - JSON 序列化，已禁用默认特性减少编译时间

**网络与安全**：
- `url` (v2.0, 无默认特性) - URL 解析，仅启用必要功能
- `async-tls` (v0.12) - TLS/HTTPS 支持
- `rustls` (v0.20) - 纯 Rust TLS 实现
- `webpki-roots` (v0.22) - 根证书集合

**并发与日志**：
- `dashmap` (v5) - 线程安全的并发 HashMap，用于速率限制
- `once_cell` (v1.19) - 延迟初始化静态变量（`Lazy`）
- `tracing` (v0.1) - 结构化日志与跟踪框架
- `tracing-subscriber` (v0.3, features: json, env-filter) - 日志订阅器，支持 JSON 输出与环境变量过滤
- `uuid` (v1.0, features: v4, fast-rng) - 生成请求 ID

**错误处理**：
- `thiserror` (v1) - 派生宏简化错误类型定义

**监控与指标**：
- `prometheus` (v0.13, 无默认特性) - Prometheus 指标收集
- `lazy_static` (v1.4) - 静态变量初始化，用于全局指标注册

**已移除的冗余依赖**：
- ~~`async-channel`~~ - 未使用
- ~~`bytes`~~ - 未使用
- ~~`futures-lite`~~ - 已用 `smol::io` 提供的 I/O 扩展替代
- ~~`log`~~ - 已迁移到 `tracing`
- ~~`env_logger`~~ - 已替换为 `tracing-subscriber`

通过精简依赖与禁用非必要特性，显著减少了依赖树深度与编译时间。

### 构建与运行

```bash
# 克隆项目
git clone <repository-url>
cd api-router

# 构建项目
cargo build --release

# 设置环境变量
export DEFAULT_API_KEY="your-api-key-here"

# 运行服务（默认使用 transformer/qwen.json）
cargo run

# 启用调试日志
RUST_LOG=debug cargo run

# 使用 JSON 格式日志（便于日志聚合）
LOG_FORMAT=json cargo run
```

### 命令行参数

- 第一个参数：配置文件名（不包含 `.json` 后缀，默认 `qwen`）。配置文件位置固定在 `transformer/` 目录下。
- 第二个参数（可选）：端口号。如果未提供则使用配置文件中的 `port` 字段。

示例：

- `cargo run -- qwen` 使用 `transformer/qwen.json`
- `cargo run -- openai 9000` 使用 `transformer/openai.json` 并监听 9000 端口
- `cargo run -- anthropic` 使用 `transformer/anthropic.json`
- `cargo run -- cohere` 使用 `transformer/cohere.json`
- `cargo run -- gemini` 使用 `transformer/gemini.json`
- `cargo run -- ollama-cloud` 使用 `transformer/ollama-cloud.json`（Ollama Cloud API）
- `cargo run -- ollama-local` 使用 `transformer/ollama-local.json`（本地 Ollama 实例）

当前仓库预置的 transformer 配置包括 `qwen`（默认）、`openai`、`anthropic`、`cohere`、`gemini`、`ollama-cloud` 与 `ollama-local`，可通过上述参数快速切换不同的上游提供商。

配套的 `test_api.sh` 脚本同样接受配置名与端口参数，例如 `./test_api.sh anthropic 9000` 会针对运行在 9000 端口且使用 `transformer/anthropic.json` 的服务发起请求示例。

## 配置文件

API Router 通过 `transformer/*.json` 文件动态加载配置，支持：

- 基础 URL (`baseUrl`)
- 默认请求头 (`headers`)
- 多端点独立设置（自定义上游路径/HTTP 方法、额外头部、流式与 multipart 支持）
- 模型名称映射 (`modelMapping`)
- 令牌桶限流策略（全局 `rateLimit` 与端点覆盖）
- 自定义监听端口 (`port`)

### 配置示例

```json
{
  "name": "qwen",
  "baseUrl": "https://portal.qwen.ai",
  "headers": {
    "Content-Type": "application/json",
    "User-Agent": "QwenCode/0.0.14 (linux; x64)",
    "Accept": "application/json"
  },
  "rateLimit": {
    "requestsPerMinute": 120,
    "burst": 40
  },
  "endpoints": {
    "/v1/chat/completions": {
      "headers": {
        "Accept": "application/json, text/event-stream"
      },
      "streamSupport": true,
      "rateLimit": {
        "requestsPerMinute": 60,
        "burst": 25
      }
    },
    "/v1/completions": {
      "headers": {
        "Accept": "application/json, text/event-stream"
      },
      "streamSupport": true,
      "rateLimit": {
        "requestsPerMinute": 60,
        "burst": 25
      }
    },
    "/v1/embeddings": {},
    "/v1/audio/transcriptions": {
      "requiresMultipart": true
    },
    "/v1/audio/translations": {
      "requiresMultipart": true
    }
  },
  "modelMapping": {
    "gpt-3.5-turbo": "qwen3-coder-plus",
    "gpt-4": "qwen3-coder-max"
  },
  "port": 8000
}
```

`endpoints` 字段允许针对不同路由覆盖上游 Header、转发路径 (`upstreamPath`)、HTTP 方法 (`method`)、是否支持流式转发以及是否需要特殊处理（如 multipart 音频上传）。

#### 流式传输配置

API Router 支持配置流式传输的缓冲区大小和心跳间隔，可在全局或端点级别配置：

```json
{
  "baseUrl": "https://api.example.com",
  "streamConfig": {
    "bufferSize": 8192,
    "heartbeatIntervalSecs": 30
  },
  "endpoints": {
    "/v1/chat/completions": {
      "streamSupport": true,
      "streamConfig": {
        "bufferSize": 4096,
        "heartbeatIntervalSecs": 15
      }
    }
  }
}
```

- `bufferSize`：流式传输的缓冲区大小（字节），默认 8192（8 KB）
- `heartbeatIntervalSecs`：心跳间隔（秒），默认 30 秒。在上游响应慢时发送心跳保持连接

详细的流式传输文档请参阅 [STREAMING.md](STREAMING.md)。

#### 结构化日志与跟踪配置

API Router 使用 `tracing` 框架提供结构化日志和延迟监控：

```bash
# 设置日志级别（默认 info）
export RUST_LOG=info          # 仅关键信息
export RUST_LOG=debug         # 包含详细调试信息
export RUST_LOG=warn          # 仅警告和错误

# 设置日志格式（默认人类可读）
export LOG_FORMAT=json        # JSON 格式，适合日志聚合系统

# 模块级别过滤
export RUST_LOG=api_router=debug,hyper=warn
```

**日志字段**：
- `request_id`：每个请求的唯一 UUID
- `client_ip`：客户端 IP 地址
- `method`：HTTP 方法
- `route`：请求路径
- `status_code`：HTTP 状态码
- `latency_ms`：总请求延迟（毫秒）
- `provider`：上游提供商（qwen、openai、anthropic 等）
- `upstream_latency_ms`：上游 API 延迟（毫秒）

详细的日志配置文档请参阅 [TRACING.md](TRACING.md)。


#### 限流配置

- `rateLimit` 支持通过配置文件为全局及单个端点设置 `requestsPerMinute` 与 `burst` 阈值。
- 如果配置文件未提供，可通过环境变量 `RATE_LIMIT_REQUESTS_PER_MINUTE` 与 `RATE_LIMIT_BURST` 设置默认值。
- 每个客户端 API Key 与路由组合分别维护令牌桶，超限时返回 `429 Too Many Requests`，并透出 `Retry-After` 头提示重试秒数。
- `/health` 端点会返回当前活跃的令牌桶数量以及按路由分组的统计信息，便于监控限流状态。

#### 配置缓存与热加载

- 配置文件通过 `OnceLock + RwLock` 缓存，首次请求后会常驻内存，避免重复 I/O 与 JSON 解析开销。
- 每次获取配置时都会检查目标文件的修改时间，只要检测到变更就会自动重新读取并刷新缓存，无需重启进程。
- 通过修改或执行 `touch transformer/<name>.json` 即可触发热加载；在自定义目录下的配置同样适用。
- 设置环境变量 `API_ROUTER_CONFIG_PATH=/path/to/config.json` 可以将配置文件移动到 `transformer/` 目录之外，便于挂载外部卷或在测试中使用临时文件。

## API 端点

| 方法 | 路径 | 说明 |
| ---- | ---- | ---- |
| GET  | `/health` | 健康检查（包含限流指标） |
| GET  | `/metrics` | Prometheus 格式性能指标 |
| GET  | `/v1/models` | 返回可用模型列表（示例数据） |
| POST | `/v1/chat/completions` | Chat Completions 代理，支持流式 |
| POST | `/v1/completions` | Text Completions 代理，支持流式 |
| POST | `/v1/embeddings` | Embeddings 代理 |
| POST | `/v1/audio/transcriptions` | 音频转写代理（multipart/form-data） |
| POST | `/v1/audio/translations` | 音频翻译代理（multipart/form-data） |
| POST | `/v1/messages` | Anthropic Messages API 代理，支持流式 |

## 使用示例

### Chat Completions（非流式）
```bash
curl -X POST http://localhost:8000/v1/chat/completions \
  -H "Authorization: Bearer your-api-key" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "qwen3-coder-plus",
    "messages": [
      {"role": "user", "content": "你好，请介绍一下你自己。"}
    ],
    "temperature": 0.7,
    "max_tokens": 1500
  }'
```

### Chat Completions（SSE 流式）
```bash
curl -N -X POST http://localhost:8000/v1/chat/completions \
  -H "Authorization: Bearer your-api-key" \
  -H "Content-Type: application/json" \
  -H "Accept: text/event-stream" \
  -d '{
    "model": "qwen3-coder-plus",
    "messages": [
      {"role": "user", "content": "请用中文解释 Rust 的 async/await。"}
    ],
    "stream": true
  }'
```

### Text Completions
```bash
curl -X POST http://localhost:8000/v1/completions \
  -H "Authorization: Bearer your-api-key" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-3.5-turbo",
    "prompt": "Write a haiku about async Rust",
    "max_tokens": 64,
    "stream": false
  }'
```

### Embeddings
```bash
curl -X POST http://localhost:8000/v1/embeddings \
  -H "Authorization: Bearer your-api-key" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-3.5-turbo",
    "input": "你好，世界"
  }'
```

### 音频转写（multipart/form-data）
```bash
curl -X POST http://localhost:8000/v1/audio/transcriptions \
  -H "Authorization: Bearer your-api-key" \
  -F "file=@sample.wav" \
  -F "model=whisper-1" \
  -F "response_format=json"
```

### 音频翻译（multipart/form-data）
```bash
curl -X POST http://localhost:8000/v1/audio/translations \
  -H "Authorization: Bearer your-api-key" \
  -F "file=@sample.wav" \
  -F "model=whisper-1" \
  -F "prompt=Translate this recording"
```

### Anthropic Messages API（非流式）
```bash
curl -X POST http://localhost:8000/v1/messages \
  -H "Authorization: Bearer your-api-key" \
  -H "Content-Type: application/json" \
  -H "anthropic-version: 2023-06-01" \
  -d '{
    "model": "claude-3-5-sonnet-20240620",
    "max_tokens": 1024,
    "messages": [
      {"role": "user", "content": "你好，请介绍一下你自己。"}
    ]
  }'
```

### Anthropic Messages API（带系统提示）
```bash
curl -X POST http://localhost:8000/v1/messages \
  -H "Authorization: Bearer your-api-key" \
  -H "Content-Type: application/json" \
  -H "anthropic-version: 2023-06-01" \
  -d '{
    "model": "claude-3-haiku-20240307",
    "max_tokens": 512,
    "system": "你是一个友好的 Rust 编程助手。",
    "messages": [
      {"role": "user", "content": "什么是 async/await？"}
    ],
    "temperature": 0.7
  }'
```

### Anthropic Messages API（流式）
```bash
curl -N -X POST http://localhost:8000/v1/messages \
  -H "Authorization: Bearer your-api-key" \
  -H "Content-Type: application/json" \
  -H "anthropic-version: 2023-06-01" \
  -H "Accept: text/event-stream" \
  -d '{
    "model": "claude-3-5-sonnet-20240620",
    "max_tokens": 100,
    "stream": true,
    "messages": [
      {"role": "user", "content": "数到5"}
    ]
  }'
```

### Ollama 支持

API Router 现已支持 Ollama API，提供两种配置：

#### Ollama Cloud（`ollama-cloud`）

用于 Ollama Cloud API（https://ollama.com/api），需要 API Key 认证。

**启动服务**：
```bash
export DEFAULT_API_KEY="your-ollama-cloud-api-key"
cargo run -- ollama-cloud
```

**Chat Completions（非流式）**：
```bash
curl -X POST http://localhost:8000/v1/chat/completions \
  -H "Authorization: Bearer your-ollama-api-key" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "glm-4.6",
    "messages": [
      {"role": "user", "content": "你好"}
    ],
    "stream": false
  }'
```

**Chat Completions（流式）**：
```bash
curl -N -X POST http://localhost:8000/v1/chat/completions \
  -H "Authorization: Bearer your-ollama-api-key" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "glm-4.6",
    "messages": [
      {"role": "user", "content": "你好"}
    ],
    "stream": true
  }'
```

#### Ollama Local（`ollama-local`）

用于本地运行的 Ollama 实例（默认 http://localhost:11434），通常不需要 API Key。

**启动服务**：
```bash
# 本地 Ollama 通常不需要 API Key，但仍可以设置（如果你的本地实例配置了认证）
cargo run -- ollama-local
```

**Chat Completions**：
```bash
curl -X POST http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3.2",
    "messages": [
      {"role": "user", "content": "Hello"}
    ],
    "stream": false
  }'
```

**Text Completions（使用 `/api/generate`）**：
```bash
curl -X POST http://localhost:8000/v1/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3.2",
    "prompt": "Write a haiku about programming"
  }'
```

**Embeddings**：
```bash
curl -X POST http://localhost:8000/v1/embeddings \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3.2",
    "input": "Hello world"
  }'
```

**端点映射**：
- `/v1/chat/completions` → `/api/chat`
- `/v1/completions` → `/api/generate`
- `/v1/embeddings` → `/api/embeddings`

**模型映射**：

两个配置都提供了 OpenAI 模型到 Ollama 模型的默认映射：
- `gpt-3.5-turbo` → `llama3.2`
- `gpt-4` → `llama3.1:70b`
- `gpt-4-turbo` → `llama3.1:70b`
- `gpt-4o` → `llama3.1:405b`
- `gpt-4o-mini` → `llama3.2`

你也可以在配置文件中修改 `modelMapping` 来使用其他 Ollama 模型。

## 开发指南：Handlers 模块

重新梳理后的 `handlers` 目录按职责拆分为五个子模块，便于后续维护与扩展：

- `handlers/router.rs`：面向 TCP 连接的入口，负责读取原始字节流、调用解析器、执行限流校验，并委托路由层进行转发。
- `handlers/parser.rs`：封装 HTTP 请求解析、Header 归一化、默认密钥解析等通用逻辑，同时提供 `ParsedRequest` 实例的便捷接口。
- `handlers/plan.rs`：生成上游调用所需的 `ForwardPlan`，统一处理基地址合并、方法覆盖以及 Header 构建，避免重复克隆配置。
- `handlers/routes.rs`：实现具体的 OpenAI 兼容路由，复用 `forward_json_route` 与 `forward_multipart_route` 处理 JSON/SSE 与 multipart 请求。
- `handlers/response.rs`：构建响应报文并负责向客户端写回，消除 `Vec<u8> ⇆ String` 的多余转换。

若要为代理增加新的 OpenAI 风格端点，可按照如下步骤扩展：

1. 在 `routes.rs` 中注册新的路径，选择 `forward_json_route` 或 `forward_multipart_route` 并提供模型映射/流式判定函数；
2. 视需要在配置文件中新增端点定义，`plan.rs` 会自动合并上游 Method、Header 与流式配置；
3. 在 `handlers/tests.rs` 中补充针对新增路径的单元或集成测试，复用 `with_mock_http_client` 以隔离真实上游依赖。

通过上述拆分，核心逻辑更加聚焦，测试覆盖也更加精确，有助于未来接入新的模型端点或协议扩展。

## 测试与代码覆盖率

API Router 拥有全面的单元测试和集成测试覆盖。

### 运行测试

```bash
# 运行所有测试（单元测试 + 集成测试）
cargo test --all

# 仅运行单元测试
cargo test --lib

# 仅运行集成测试
cargo test --test '*'

# 运行特定模块的测试
cargo test --lib config::tests
cargo test --lib rate_limit::tests
```

### 测试统计

- **单元测试**：151 个
- **集成测试**：11 个
- **总测试数**：162 个
- **测试通过率**：100%

### 覆盖的模块

| 模块 | 测试数量 | 覆盖率估算 |
|------|----------|------------|
| 配置解析 (config.rs) | 20+ | ~90% |
| 速率限制 (rate_limit.rs) | 19+ | ~95% |
| 模型序列化 (models.rs) | 13+ | ~85% |
| 错误处理 (errors.rs) | 11 | ~95% |
| HTTP 工具 (http_client.rs) | 10 | ~75% |
| 请求解析 (parser.rs) | 23 | ~90% |
| 请求规划 (plan.rs) | 21 | ~90% |
| 集成流程 | 11 | ~80% |

### 代码覆盖率工具

项目支持两种代码覆盖率工具：

#### 1. cargo-tarpaulin（推荐）

```bash
# 安装
cargo install cargo-tarpaulin

# 生成覆盖率报告
cargo tarpaulin --out Html --out Xml

# 查看 HTML 报告
open tarpaulin-report.html
```

#### 2. grcov（备选）

```bash
# 安装
cargo install grcov
rustup component add llvm-tools-preview

# 详细步骤请参阅 COVERAGE.md
```

### CI 集成

GitHub Actions 工作流会自动：
- 运行所有测试
- 生成覆盖率报告
- 检查覆盖率阈值（70% 警告）
- 上传覆盖率报告为构建工件

### 详细文档

- [TEST_SUMMARY.md](TEST_SUMMARY.md) - 测试覆盖详情和统计
- [COVERAGE.md](COVERAGE.md) - 代码覆盖率工具使用指南

## 监控与指标

API Router 集成了 Prometheus 指标收集，通过 `/metrics` 端点暴露性能数据：

```bash
curl http://localhost:8000/metrics
```

### 可用指标

- **`requests_total`**：按路由、方法和状态码统计的请求总数（Counter）
- **`upstream_errors_total`**：按错误类型统计的上游错误总数（Counter）
- **`request_latency_seconds`**：按路由统计的请求延迟分布（Histogram）
- **`active_connections`**：当前活跃连接数（Gauge）
- **`rate_limiter_buckets`**：活跃的限流令牌桶数量（Gauge）

详细的指标说明、Prometheus 配置示例和 Grafana 查询请参阅 [METRICS.md](METRICS.md)。

## 许可证

MIT License
