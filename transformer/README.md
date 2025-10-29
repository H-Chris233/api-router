# Transformer 配置文件说明

本目录包含各个 API 提供商的配置文件，用于将不同的 API 转换为 OpenAI 兼容格式。

## 配置文件列表

- `qwen.json` - 通义千问 API 配置（默认）
- `openai.json` - OpenAI API 配置
- `anthropic.json` - Anthropic Claude API 配置
- `cohere.json` - Cohere API 配置
- `gemini.json` - Google Gemini API 配置
- `ollama-cloud.json` - Ollama Cloud API 配置
- `ollama-local.json` - 本地 Ollama 实例配置

## 配置文件结构

### 基本字段

```json
{
  "name": "配置名称",
  "baseUrl": "上游 API 的基础 URL",
  "port": 8000,
  "headers": {
    "Content-Type": "application/json",
    "User-Agent": "自定义 User-Agent"
  },
  "modelMapping": {
    "客户端模型名": "上游实际模型名"
  },
  "rateLimit": {
    "requestsPerMinute": 120,
    "burst": 40
  },
  "streamConfig": {
    "bufferSize": 8192,
    "heartbeatIntervalSecs": 30
  },
  "endpoints": {
    "/v1/chat/completions": {
      "upstreamPath": "/v1/messages",
      "method": "POST",
      "headers": {},
      "streamSupport": true,
      "requiresMultipart": false,
      "rateLimit": {},
      "streamConfig": {}
    }
  }
}
```

### 字段说明

#### 顶层字段

- **name** (可选): 配置名称，用于标识
- **baseUrl** (必需): 上游 API 的基础 URL，包含协议和主机名
- **port** (可选): 本地监听端口，默认 8000
- **headers** (可选): 全局请求头，会合并到所有请求中
- **modelMapping** (可选): 模型名称映射表，将客户端请求的模型名转换为上游模型名
- **rateLimit** (可选): 全局速率限制配置
- **streamConfig** (可选): 全局流式传输配置
- **endpoints** (可选): 端点级别的配置覆盖

#### rateLimit 字段

```json
{
  "requestsPerMinute": 120,  // 每分钟最大请求数，0 表示不限制
  "burst": 40                // 突发容量，默认等于 requestsPerMinute
}
```

#### streamConfig 字段

```json
{
  "bufferSize": 8192,              // 流式传输缓冲区大小（字节），默认 8192
  "heartbeatIntervalSecs": 30      // 心跳间隔（秒），默认 30
}
```

#### endpoints 字段

每个端点可以覆盖全局配置：

- **upstreamPath** (可选): 上游路径，用于路径重写
- **method** (可选): HTTP 方法覆写（GET, POST 等）
- **headers** (可选): 端点特定的请求头
- **streamSupport** (可选): 是否支持流式传输，默认 false
- **requiresMultipart** (可选): 是否需要 multipart/form-data 格式，默认 false
- **rateLimit** (可选): 端点级别的速率限制，优先级高于全局配置
- **streamConfig** (可选): 端点级别的流式配置，优先级高于全局配置

## 使用方法

### 1. 选择配置文件

通过命令行参数指定配置文件（不含 .json 后缀）：

```bash
# 使用 qwen.json（默认）
cargo run

# 使用 anthropic.json
cargo run -- anthropic

# 使用 openai.json 并指定端口
cargo run -- openai 9000
```

### 2. 环境变量覆盖

可以通过环境变量覆盖部分配置：

```bash
# 覆盖默认 API 密钥
export DEFAULT_API_KEY="your-api-key"

# 覆盖速率限制
export RATE_LIMIT_REQUESTS_PER_MINUTE=60
export RATE_LIMIT_BURST=20

# 使用自定义配置文件路径
export API_ROUTER_CONFIG_PATH=/path/to/custom/config.json

cargo run
```

### 3. 配置优先级

配置解析优先级（从高到低）：

1. 环境变量（`API_ROUTER_CONFIG_PATH`）
2. 命令行参数（第一个参数）
3. 默认配置（`transformer/qwen.json`）

速率限制配置优先级：

1. 端点级别配置
2. 全局配置
3. 环境变量

## 配置示例

### 基本配置

最简单的配置只需要 baseUrl：

```json
{
  "baseUrl": "https://api.example.com"
}
```

### 完整配置

包含所有可选字段的完整示例：

```json
{
  "name": "example",
  "baseUrl": "https://api.example.com",
  "port": 8000,
  "headers": {
    "Content-Type": "application/json",
    "X-API-Version": "2023"
  },
  "rateLimit": {
    "requestsPerMinute": 100,
    "burst": 30
  },
  "streamConfig": {
    "bufferSize": 16384,
    "heartbeatIntervalSecs": 45
  },
  "modelMapping": {
    "gpt-3.5-turbo": "example-small",
    "gpt-4": "example-large"
  },
  "endpoints": {
    "/v1/chat/completions": {
      "upstreamPath": "/api/chat",
      "headers": {
        "Accept": "text/event-stream"
      },
      "streamSupport": true,
      "rateLimit": {
        "requestsPerMinute": 50
      }
    },
    "/v1/embeddings": {
      "upstreamPath": "/api/embed",
      "method": "POST"
    }
  }
}
```

## 热重载

配置文件支持热重载，修改配置后无需重启服务：

```bash
# 修改配置文件
vi transformer/qwen.json

# 或触发文件修改时间更新
touch transformer/qwen.json
```

下一个请求会自动加载新配置。

## 添加新的 API 提供商

1. 在 `transformer/` 目录创建新的 JSON 配置文件
2. 设置 baseUrl 和必要的 headers
3. 配置端点映射和模型映射
4. 测试配置：

```bash
cargo run -- your-config-name
curl http://localhost:8000/health
```

## 故障排除

### 配置文件加载失败

检查配置文件是否为有效的 JSON 格式：

```bash
cat transformer/your-config.json | jq .
```

### 端口被占用

服务会自动尝试下一个端口（最多尝试 10 次）：

```
端口 8000 被占用: ..., 尝试下一个端口
API Router 启动在 http://0.0.0.0:8001
```

### 速率限制不生效

确保配置中的 `requestsPerMinute` 不为 0：

```json
{
  "rateLimit": {
    "requestsPerMinute": 60  // 不能为 0
  }
}
```

## 更多信息

- 详细的 API 文档：`../docs/openapi.yaml`
- 配置示例：查看现有的 `*.json` 文件
- 速率限制文档：`../docs/指标监控.md`
- 流式传输文档：`../docs/流式传输.md`
