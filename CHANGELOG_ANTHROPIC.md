# Anthropic API 支持更新日志

## 新增功能

### 1. Anthropic Messages API 端点支持 (`/v1/messages`)

添加了对 Anthropic 原生 API 格式的完整支持，包括：

- ✅ 原生 `/v1/messages` 端点路由
- ✅ 支持 Anthropic 特定的请求格式
- ✅ 支持流式和非流式响应
- ✅ 支持模型名称映射
- ✅ 支持速率限制

### 2. 数据模型

在 `src/models.rs` 中新增以下数据结构：

```rust
// Anthropic 消息结构
pub struct AnthropicMessage {
    pub role: String,
    pub content: String,
}

// Anthropic 请求结构
pub struct AnthropicMessagesRequest {
    pub model: String,
    pub messages: Vec<AnthropicMessage>,
    pub max_tokens: u32,
    pub system: Option<String>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<u32>,
    pub stream: Option<bool>,
    pub stop_sequences: Option<Vec<String>>,
}

// Anthropic 响应结构
pub struct AnthropicMessagesResponse {
    pub id: String,
    pub response_type: String,
    pub role: String,
    pub content: Vec<AnthropicContentBlock>,
    pub model: String,
    pub stop_reason: Option<String>,
    pub stop_sequence: Option<String>,
    pub usage: AnthropicUsage,
}
```

### 3. 路由处理

**文件**: `src/handlers/routes.rs`

- 添加了 `/v1/messages` 路由处理
- 实现了 `adjust_anthropic_request` 函数用于模型映射
- 实现了 `anthropic_should_stream` 函数用于流式判断

**文件**: `src/handlers/router.rs`

- 在主路由匹配中添加了 `("POST", "/v1/messages")` 分支
- 自动应用速率限制和认证机制

### 4. 配置文件更新

**文件**: `transformer/anthropic.json`

添加了 `/v1/messages` 端点配置：

```json
{
  "/v1/messages": {
    "headers": {
      "Accept": "application/json, text/event-stream"
    },
    "streamSupport": true
  }
}
```

### 5. 测试覆盖

**文件**: `src/handlers/tests.rs`

新增两个测试用例：

1. `anthropic_messages_route_forwards_with_model_mapping`: 测试模型映射功能
2. `anthropic_messages_route_with_system_message`: 测试带系统提示的请求

测试覆盖：
- ✅ 基本请求转发
- ✅ 模型名称映射
- ✅ 系统提示字段
- ✅ 认证头转发
- ✅ anthropic-version 头
- ✅ 响应解析

### 6. 测试脚本

**文件**: `test_anthropic.sh`

新增专门的 Anthropic API 测试脚本，包括：
- 健康检查测试
- 模型列表测试
- 非流式请求测试
- 带系统提示的请求测试
- 流式请求测试
- OpenAI 格式到 Anthropic 转换测试

### 7. 文档

**文件**: `ANTHROPIC.md`

新增完整的 Anthropic API 支持文档，包括：
- 功能特性说明
- 完整的配置示例
- API 请求格式详解
- 多种使用场景示例
- 错误处理指南
- 最佳实践建议
- 故障排查指南

**文件**: `README.md`

更新主文档：
- 在功能特性中添加 Anthropic 支持说明
- 在 API 端点表中添加 `/v1/messages` 条目
- 添加 Anthropic API 使用示例（非流式、带系统提示、流式）

## 技术实现细节

### 请求处理流程

1. 客户端发送 POST 请求到 `/v1/messages`
2. `router.rs` 接收请求并进行速率限制检查
3. `routes.rs` 调用 `forward_json_route` 处理请求：
   - 解析 Anthropic 格式的请求体
   - 应用模型名称映射
   - 构建上游请求
4. 根据 `stream` 字段决定使用流式或非流式响应
5. 通过 `http_client.rs` 转发到配置的上游服务
6. 将响应返回给客户端

### 流式响应支持

Anthropic 的流式响应与 OpenAI 类似，都使用 Server-Sent Events (SSE) 格式：
- 自动检测 `stream: true` 参数
- 使用 `handle_streaming_request` 处理流式转发
- 支持反压机制和心跳保活
- 客户端断连时优雅关闭

### 模型映射

配置示例：
```json
{
  "modelMapping": {
    "gpt-4o": "claude-3-opus-20240229",
    "gpt-4": "claude-3-opus-20240229",
    "gpt-4o-mini": "claude-3-5-sonnet-20240620",
    "gpt-3.5-turbo": "claude-3-haiku-20240307"
  }
}
```

允许使用通用模型名称，自动转换为 Anthropic 的具体模型标识符。

## 兼容性

- ✅ 与现有 OpenAI 端点完全兼容
- ✅ 不影响其他 API 提供商的配置和功能
- ✅ 向后兼容，现有功能不受影响
- ✅ 支持热加载配置

## 测试结果

所有测试通过：
```
running 24 tests
........................
test result: ok. 24 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

running 4 tests
....
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

running 7 tests
.......
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

新增测试：
- `anthropic_messages_route_forwards_with_model_mapping` ✅
- `anthropic_messages_route_with_system_message` ✅

## 使用示例

### 启动服务

```bash
cargo run -- anthropic
```

### 基本请求

```bash
curl -X POST http://localhost:8000/v1/messages \
  -H "Authorization: Bearer sk-ant-your-api-key" \
  -H "Content-Type: application/json" \
  -H "anthropic-version: 2023-06-01" \
  -d '{
    "model": "claude-3-5-sonnet-20240620",
    "max_tokens": 1024,
    "messages": [{"role": "user", "content": "Hello"}]
  }'
```

### 流式请求

```bash
curl -N -X POST http://localhost:8000/v1/messages \
  -H "Authorization: Bearer sk-ant-your-api-key" \
  -H "Content-Type: application/json" \
  -H "anthropic-version: 2023-06-01" \
  -d '{
    "model": "claude-3-5-sonnet-20240620",
    "max_tokens": 100,
    "stream": true,
    "messages": [{"role": "user", "content": "Count to 5"}]
  }'
```

## 修改的文件清单

### 新增文件
- `ANTHROPIC.md` - Anthropic API 完整文档
- `test_anthropic.sh` - Anthropic 端点测试脚本
- `CHANGELOG_ANTHROPIC.md` - 本更新日志

### 修改的文件
- `src/models.rs` - 添加 Anthropic 数据模型
- `src/handlers/routes.rs` - 添加 /v1/messages 路由处理
- `src/handlers/router.rs` - 添加路由匹配
- `src/handlers/tests.rs` - 添加测试用例
- `transformer/anthropic.json` - 更新配置
- `README.md` - 更新文档

## 下一步计划（可选）

未来可以考虑的增强功能：

1. **OpenAI 到 Anthropic 格式转换**：自动将 OpenAI 格式的请求转换为 Anthropic 格式
2. **Anthropic 到 OpenAI 格式转换**：将 Anthropic 响应转换为 OpenAI 格式
3. **更多 Anthropic 端点支持**：如 `/v1/complete`（旧版 API）
4. **Vision API 支持**：支持图片输入
5. **工具调用支持**：支持 Anthropic 的函数调用功能

## 总结

本次更新为 API Router 添加了完整的 Anthropic Messages API 支持，包括：
- 原生 `/v1/messages` 端点
- 完整的请求/响应数据模型
- 流式和非流式响应支持
- 模型映射功能
- 速率限制支持
- 完善的测试覆盖
- 详细的文档

所有功能已通过测试，可以投入生产使用。
