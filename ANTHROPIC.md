# Anthropic API 支持文档

API Router 现已全面支持 Anthropic Messages API（`/v1/messages`）端点，可以将 Anthropic 风格的请求代理到配置的上游服务。

## 功能特性

- ✅ 支持 Anthropic Messages API（`/v1/messages`）原生格式
- ✅ 支持 `system` 系统提示字段
- ✅ 支持 `max_tokens` 必需参数
- ✅ 支持流式响应（Server-Sent Events）
- ✅ 支持模型名称映射
- ✅ 支持速率限制
- ✅ 完整的认证机制支持
- ✅ 支持 Anthropic 特定参数（temperature, top_p, top_k, stop_sequences）

## 配置示例

### 完整的 Anthropic 配置

文件位置：`transformer/anthropic.json`

```json
{
  "name": "anthropic",
  "baseUrl": "https://api.anthropic.com",
  "headers": {
    "Content-Type": "application/json",
    "User-Agent": "api-router/1.0",
    "Accept": "application/json",
    "anthropic-version": "2023-06-01"
  },
  "endpoints": {
    "/v1/chat/completions": {
      "upstreamPath": "/v1/messages",
      "headers": {
        "Accept": "application/json, text/event-stream"
      },
      "streamSupport": true
    },
    "/v1/messages": {
      "headers": {
        "Accept": "application/json, text/event-stream"
      },
      "streamSupport": true
    }
  },
  "modelMapping": {
    "gpt-4o": "claude-3-opus-20240229",
    "gpt-4": "claude-3-opus-20240229",
    "gpt-4o-mini": "claude-3-5-sonnet-20240620",
    "gpt-3.5-turbo": "claude-3-haiku-20240307"
  },
  "rateLimit": {
    "requestsPerMinute": 50,
    "burst": 10
  },
  "port": 8000
}
```

### 配置说明

1. **baseUrl**: Anthropic API 的基础 URL
2. **headers**: 全局请求头，包括必需的 `anthropic-version` 头
3. **endpoints**: 端点级别的配置
   - `/v1/messages`: 原生 Anthropic Messages API 端点
   - `/v1/chat/completions`: OpenAI 格式转换为 Anthropic 格式（可选）
4. **modelMapping**: 模型名称映射，可将通用模型名转换为 Anthropic 的具体模型
5. **rateLimit**: 速率限制配置（可选）

## API 请求格式

### Anthropic Messages API 请求结构

```json
{
  "model": "claude-3-5-sonnet-20240620",
  "max_tokens": 1024,
  "messages": [
    {
      "role": "user",
      "content": "你好，请介绍一下你自己。"
    }
  ],
  "system": "你是一个友好的助手。",
  "temperature": 0.7,
  "top_p": 0.9,
  "top_k": 40,
  "stream": false,
  "stop_sequences": ["Human:", "Assistant:"]
}
```

### 字段说明

| 字段 | 类型 | 必需 | 说明 |
|------|------|------|------|
| `model` | string | ✅ | 模型标识符，如 `claude-3-5-sonnet-20240620` |
| `max_tokens` | integer | ✅ | 生成的最大 token 数量 |
| `messages` | array | ✅ | 对话消息数组 |
| `system` | string | ❌ | 系统提示，用于设置助手的行为 |
| `temperature` | number | ❌ | 采样温度（0-1） |
| `top_p` | number | ❌ | 核采样参数 |
| `top_k` | integer | ❌ | Top-K 采样参数 |
| `stream` | boolean | ❌ | 是否启用流式响应 |
| `stop_sequences` | array | ❌ | 停止序列列表 |

### Messages 数组格式

```json
{
  "role": "user",
  "content": "你的问题或指令"
}
```

支持的角色：
- `user`: 用户消息
- `assistant`: 助手消息（用于提供对话历史）

**注意**：`system` 角色的消息应该使用请求根级别的 `system` 字段，而不是放在 `messages` 数组中。

## 使用示例

### 1. 基本的非流式请求

```bash
curl -X POST http://localhost:8000/v1/messages \
  -H "Authorization: Bearer sk-ant-your-api-key" \
  -H "Content-Type: application/json" \
  -H "anthropic-version: 2023-06-01" \
  -d '{
    "model": "claude-3-5-sonnet-20240620",
    "max_tokens": 1024,
    "messages": [
      {
        "role": "user",
        "content": "什么是 Rust 编程语言？"
      }
    ]
  }'
```

### 2. 带系统提示的请求

```bash
curl -X POST http://localhost:8000/v1/messages \
  -H "Authorization: Bearer sk-ant-your-api-key" \
  -H "Content-Type: application/json" \
  -H "anthropic-version: 2023-06-01" \
  -d '{
    "model": "claude-3-haiku-20240307",
    "max_tokens": 512,
    "system": "你是一个专业的 Rust 编程专家，擅长解释复杂的概念。",
    "messages": [
      {
        "role": "user",
        "content": "解释一下 Rust 的所有权系统。"
      }
    ],
    "temperature": 0.7
  }'
```

### 3. 多轮对话

```bash
curl -X POST http://localhost:8000/v1/messages \
  -H "Authorization: Bearer sk-ant-your-api-key" \
  -H "Content-Type: application/json" \
  -H "anthropic-version: 2023-06-01" \
  -d '{
    "model": "claude-3-5-sonnet-20240620",
    "max_tokens": 1024,
    "messages": [
      {
        "role": "user",
        "content": "你好！"
      },
      {
        "role": "assistant",
        "content": "你好！我是 Claude。有什么我可以帮助你的吗？"
      },
      {
        "role": "user",
        "content": "请帮我写一个 Rust 函数来计算斐波那契数列。"
      }
    ]
  }'
```

### 4. 流式响应

```bash
curl -N -X POST http://localhost:8000/v1/messages \
  -H "Authorization: Bearer sk-ant-your-api-key" \
  -H "Content-Type: application/json" \
  -H "anthropic-version: 2023-06-01" \
  -H "Accept: text/event-stream" \
  -d '{
    "model": "claude-3-5-sonnet-20240620",
    "max_tokens": 500,
    "stream": true,
    "messages": [
      {
        "role": "user",
        "content": "写一首关于异步编程的诗。"
      }
    ]
  }'
```

### 5. 使用模型映射

如果配置了模型映射，可以使用通用的模型名称：

```bash
curl -X POST http://localhost:8000/v1/messages \
  -H "Authorization: Bearer sk-ant-your-api-key" \
  -H "Content-Type: application/json" \
  -H "anthropic-version: 2023-06-01" \
  -d '{
    "model": "gpt-4o-mini",
    "max_tokens": 1024,
    "messages": [
      {
        "role": "user",
        "content": "你好"
      }
    ]
  }'
```

根据配置，`gpt-4o-mini` 会自动映射为 `claude-3-5-sonnet-20240620`。

## OpenAI 格式转换

API Router 还支持将 OpenAI 格式的 `/v1/chat/completions` 请求转换为 Anthropic 格式（如果配置了对应的 `upstreamPath`）：

```bash
# OpenAI 格式请求
curl -X POST http://localhost:8000/v1/chat/completions \
  -H "Authorization: Bearer sk-ant-your-api-key" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4",
    "messages": [
      {
        "role": "user",
        "content": "你好"
      }
    ],
    "temperature": 0.7,
    "max_tokens": 100
  }'
```

此请求会被转发到 Anthropic 的 `/v1/messages` 端点（根据配置中的 `upstreamPath`）。

## 启动服务

### 使用 Anthropic 配置启动

```bash
# 设置 API 密钥（可选，如果请求中没有提供）
export DEFAULT_API_KEY="sk-ant-your-api-key-here"

# 启动服务
cargo run -- anthropic

# 或指定端口
cargo run -- anthropic 8080
```

### 测试服务

使用提供的测试脚本：

```bash
./test_anthropic.sh
```

或指定端口：

```bash
./test_anthropic.sh 8080
```

## 响应格式

### 非流式响应

```json
{
  "id": "msg_01XFDUDYJgAACzvnptvVbrYL",
  "type": "message",
  "role": "assistant",
  "content": [
    {
      "type": "text",
      "text": "Rust 是一种系统编程语言..."
    }
  ],
  "model": "claude-3-5-sonnet-20240620",
  "stop_reason": "end_turn",
  "stop_sequence": null,
  "usage": {
    "input_tokens": 12,
    "output_tokens": 85
  }
}
```

### 流式响应（SSE）

流式响应使用 Server-Sent Events 格式，包含多个事件类型：

```
event: message_start
data: {"type":"message_start","message":{"id":"msg_123","type":"message","role":"assistant","content":[],"model":"claude-3-5-sonnet-20240620","usage":{"input_tokens":10,"output_tokens":0}}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Rust"}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" 是"}}

event: content_block_stop
data: {"type":"content_block_stop","index":0}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":85}}

event: message_stop
data: {"type":"message_stop"}
```

## 速率限制

Anthropic 端点支持速率限制，可以在配置文件中设置：

```json
{
  "rateLimit": {
    "requestsPerMinute": 50,
    "burst": 10
  },
  "endpoints": {
    "/v1/messages": {
      "streamSupport": true,
      "rateLimit": {
        "requestsPerMinute": 30,
        "burst": 5
      }
    }
  }
}
```

超过速率限制时，服务会返回 `429 Too Many Requests` 响应，并包含 `Retry-After` 头。

## 错误处理

### 常见错误

1. **400 Bad Request**: 请求格式错误或缺少必需字段（如 `max_tokens`）
2. **429 Too Many Requests**: 超过速率限制
3. **502 Bad Gateway**: 上游服务连接失败或响应错误

### 错误响应格式

```json
{
  "error": {
    "message": "错误描述"
  }
}
```

## 支持的模型

常用的 Anthropic 模型：

| 模型 ID | 说明 |
|---------|------|
| `claude-3-5-sonnet-20240620` | Claude 3.5 Sonnet（最新，推荐） |
| `claude-3-opus-20240229` | Claude 3 Opus（最强大） |
| `claude-3-sonnet-20240229` | Claude 3 Sonnet（平衡） |
| `claude-3-haiku-20240307` | Claude 3 Haiku（最快） |

## 最佳实践

1. **始终提供 `max_tokens`**: 这是 Anthropic API 的必需参数
2. **使用 `system` 字段**: 用于设置助手的行为和角色
3. **合理设置 `temperature`**: 通常 0.7-1.0 用于创意任务，0.0-0.3 用于精确任务
4. **启用流式响应**: 对于长文本生成，流式响应可以提供更好的用户体验
5. **配置速率限制**: 根据你的 Anthropic API 配额合理设置速率限制
6. **使用模型映射**: 可以在配置中统一管理模型名称，便于切换和测试

## 故障排查

### 问题：收到 400 错误

**解决方案**：
- 检查是否提供了 `max_tokens` 参数
- 确认 `messages` 数组格式正确
- 验证 `system` 字段不在 `messages` 数组中

### 问题：收到 401 未授权错误

**解决方案**：
- 检查 `Authorization` 头是否正确
- 确认 API 密钥有效
- 验证 `anthropic-version` 头是否存在

### 问题：流式响应不工作

**解决方案**：
- 确保配置中 `streamSupport` 设置为 `true`
- 使用 `curl -N` 选项禁用缓冲
- 在请求中设置 `"stream": true`
- 添加 `Accept: text/event-stream` 头

## 相关资源

- [Anthropic API 官方文档](https://docs.anthropic.com/claude/reference/messages_post)
- [API Router 主文档](README.md)
- [流式传输文档](STREAMING.md)
- [配置文件示例](transformer/anthropic.json)

## 测试用例

项目包含了完整的 Anthropic 端点测试用例，可以通过以下命令运行：

```bash
cargo test anthropic
```

测试覆盖：
- ✅ 基本的非流式请求转发
- ✅ 模型名称映射
- ✅ 带系统提示的请求
- ✅ 流式响应处理
- ✅ 错误处理
- ✅ 速率限制

## 贡献

欢迎提交 Issue 和 Pull Request 来改进 Anthropic API 支持！
