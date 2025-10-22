# API Router

一个将 API 请求转发为 OpenAI 兼容格式的轻量级服务，特别适用于将具有特殊认证或签名要求的 API 转换为标准 OpenAI 客户端可以直接使用的格式。

## 功能特性

- 将非标准 API 请求转换为 OpenAI 兼容格式
- 支持流式传输（SSE）转发
- 自动处理认证头、User-Agent 以及基础请求头
- 支持模型名称映射（client model ➜ provider model）
- 支持 `/v1/chat/completions`、`/v1/completions`、`/v1/embeddings`、`/v1/audio/transcriptions`、`/v1/audio/translations` 等 OpenAI 风格端点
- 自动处理音频转写/翻译请求的 multipart/form-data 载荷
- 动态加载 transformer 目录中的 JSON 配置文件
- 支持基于 API Key 与路由粒度的令牌桶限流，超限时返回 429 并暴露健康指标

## 安装与运行

### 依赖

- Rust 1.70.0 或更高版本

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
```

### 命令行参数

- 第一个参数：配置文件名（不包含 `.json` 后缀，默认 `qwen`）。配置文件位置固定在 `transformer/` 目录下。
- 第二个参数（可选）：端口号。如果未提供则使用配置文件中的 `port` 字段。

示例：

- `cargo run -- qwen` 使用 `transformer/qwen.json`
- `cargo run -- openai 9000` 使用 `transformer/openai.json` 并监听 9000 端口

## 配置文件

API Router 通过 `transformer/*.json` 文件动态加载配置，支持：

- 基础 URL (`baseUrl`)
- 默认请求头 (`headers`)
- 多端点独立设置（额外头部、自定义上游路径、是否需要 multipart 等）
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

`endpoints` 字段允许针对不同路由覆盖上游 Header、是否支持流式转发、是否需要特殊处理（如 multipart 音频上传），以及覆写限流阈值（`rateLimit`）。

#### 限流配置

- `rateLimit` 支持通过配置文件为全局及单个端点设置 `requestsPerMinute` 与 `burst` 阈值。
- 如果配置文件未提供，可通过环境变量 `RATE_LIMIT_REQUESTS_PER_MINUTE` 与 `RATE_LIMIT_BURST` 设置默认值。
- 每个客户端 API Key 与路由组合分别维护令牌桶，超限时返回 `429 Too Many Requests`，并透出 `Retry-After` 头提示重试秒数。
- `/health` 端点会返回当前活跃的令牌桶数量以及按路由分组的统计信息，便于监控限流状态。

## API 端点

| 方法 | 路径 | 说明 |
| ---- | ---- | ---- |
| GET  | `/health` | 健康检查（包含限流指标） |
| GET  | `/v1/models` | 返回可用模型列表（示例数据） |
| POST | `/v1/chat/completions` | Chat Completions 代理，支持流式 |
| POST | `/v1/completions` | Text Completions 代理，支持流式 |
| POST | `/v1/embeddings` | Embeddings 代理 |
| POST | `/v1/audio/transcriptions` | 音频转写代理（multipart/form-data） |
| POST | `/v1/audio/translations` | 音频翻译代理（multipart/form-data） |

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

## 许可证

MIT License
