# Ollama 支持文档

API Router 现已支持 Ollama API，提供两种配置：Ollama Cloud 和 Ollama Local。

## 配置文件

### 1. Ollama Cloud (`transformer/ollama-cloud.json`)

用于连接 Ollama Cloud API (https://ollama.com/api)，需要 API Key 认证。

**特点**：
- 基础 URL: `https://ollama.com`
- 需要 API Key 认证
- 适用于云端托管的模型服务
- 配置了合理的速率限制（全局 100 req/min，聊天/完成端点 60 req/min）

**使用示例**：
```bash
# 设置 API Key
export DEFAULT_API_KEY="your-ollama-cloud-api-key"

# 启动服务
cargo run -- ollama-cloud

# 测试请求
curl -X POST http://localhost:8000/v1/chat/completions \
  -H "Authorization: Bearer your-ollama-cloud-api-key" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "glm-4.6",
    "messages": [{"role": "user", "content": "你好"}],
    "stream": false
  }'
```

### 2. Ollama Local (`transformer/ollama-local.json`)

用于连接本地运行的 Ollama 实例，默认监听 http://localhost:11434。

**特点**：
- 基础 URL: `http://localhost:11434`
- 通常不需要 API Key（除非本地实例配置了认证）
- 适用于本地部署的模型
- 更高的速率限制（全局 300 req/min），因为是本地服务

**使用示例**：
```bash
# 确保本地 Ollama 服务正在运行
# ollama serve

# 启动 API Router
cargo run -- ollama-local

# 测试请求（无需 Authorization）
curl -X POST http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3.2",
    "messages": [{"role": "user", "content": "Hello"}],
    "stream": false
  }'
```

## 端点映射

API Router 将 OpenAI 风格的端点转换为 Ollama API 端点：

| OpenAI 端点 | Ollama 端点 | 说明 |
|------------|------------|------|
| `/v1/chat/completions` | `/api/chat` | 聊天完成，支持流式 |
| `/v1/completions` | `/api/generate` | 文本生成，支持流式 |
| `/v1/embeddings` | `/api/embeddings` | 文本嵌入 |

## 模型映射

两个配置都提供了默认的模型映射，将常见的 OpenAI 模型名称映射到 Ollama 模型：

| OpenAI 模型 | Ollama 模型 |
|------------|------------|
| `gpt-3.5-turbo` | `llama3.2` |
| `gpt-4` | `llama3.1:70b` |
| `gpt-4-turbo` | `llama3.1:70b` |
| `gpt-4o` | `llama3.1:405b` |
| `gpt-4o-mini` | `llama3.2` |

你可以直接修改配置文件中的 `modelMapping` 字段来自定义映射关系。

## 流式支持

两个配置都支持服务器发送事件（SSE）流式响应：

```bash
# 流式聊天请求
curl -N -X POST http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3.2",
    "messages": [{"role": "user", "content": "写一首关于编程的诗"}],
    "stream": true
  }'
```

## 速率限制

### Ollama Cloud
- 全局限制: 100 请求/分钟，突发 30
- 聊天/完成端点: 60 请求/分钟，突发 20

### Ollama Local
- 全局限制: 300 请求/分钟，突发 100
- 聊天/完成端点: 无额外限制（使用全局设置）

速率限制可以通过修改配置文件的 `rateLimit` 字段来调整。

## 测试脚本

项目提供了测试脚本来验证配置：

```bash
# 测试 Ollama Local 配置
./test_ollama.sh ollama-local

# 测试 Ollama Cloud 配置（需要设置 API Key）
export DEFAULT_API_KEY="your-api-key"
./test_ollama.sh ollama-cloud
```

## 响应格式

需要注意的是，Ollama API 的响应格式与 OpenAI 不完全相同：

**Ollama 响应示例**：
```json
{
  "model": "glm-4.6",
  "created_at": "2025-10-24T16:12:36.127089439Z",
  "message": {
    "role": "assistant",
    "content": "你好！很高兴见到你...",
    "thinking": "收到用户发来的\"你好\"..."
  },
  "done": true,
  "done_reason": "stop",
  "total_duration": 3787827471,
  "prompt_eval_count": 6,
  "eval_count": 185
}
```

API Router 作为透明代理，会原样返回 Ollama 的响应格式。客户端需要处理 Ollama 特有的字段（如 `created_at` 而非 `created`，`message` 而非 `choices` 数组等）。

## 自定义配置

如需修改配置（如更改本地 Ollama 端口、调整速率限制、添加自定义模型映射等），直接编辑 `transformer/ollama-*.json` 文件即可，修改后会自动热加载，无需重启服务。

示例：修改本地 Ollama 端口为 8080
```json
{
  "baseUrl": "http://localhost:8080",
  ...
}
```

## 故障排除

### 本地连接失败
```
{"error":{"message":"Connection refused (os error 111)"}}
```

**解决方案**：
1. 确保本地 Ollama 服务正在运行：`ollama serve`
2. 检查 Ollama 监听的端口是否为 11434
3. 如果使用其他端口，修改配置文件的 `baseUrl`

### 云端认证失败
```
{"error":{"message":"Unauthorized"}}
```

**解决方案**：
1. 确保已设置正确的 API Key：`export DEFAULT_API_KEY="your-key"`
2. 检查请求中的 `Authorization` 头是否正确
3. 验证 API Key 是否有效且有足够的配额

## 参考资料

- [Ollama API 文档](https://github.com/ollama/ollama/blob/main/docs/api.md)
- [API Router 主文档](README.md)
- [配置文件说明](README.md#配置文件)
