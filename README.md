# API Router

一个将API请求转发为OpenAI兼容格式的服务，特别适用于将具有特殊认证要求的API转换为标准OpenAI格式。

## 功能特性

- 将非标准API请求转换为OpenAI兼容格式
- 支持流式传输（SSE）
- 自动处理认证头和User-Agent
- 支持模型名称映射
- 支持对话上下文传递
- CORS支持
- **动态配置加载**：从transformer目录的JSON文件中动态加载API配置

## 安装与运行

### 依赖

- Rust 1.90.0 或更高版本

### 构建与运行

```bash
# 克隆项目
git clone <repository-url>
cd api-router

# 构建项目
cargo build --release

# 设置环境变量
export DEFAULT_API_KEY="your-api-key-here"
export API_CONFIG_FILE="qwen.json"  # 指定要使用的配置文件

# 运行服务
cargo run
```

### 环境变量

- `DEFAULT_API_KEY`：默认API密钥（默认：预设的示例密钥）

### 命令行参数

- 第一个参数：配置文件名（默认：qwen.json），配置文件位于transformer目录下

## 配置文件

API Router 现在支持从transformer目录中的JSON文件动态加载配置，支持：
- API基本URL设置
- 请求头配置
- 端点特殊选项配置
- 模型名称映射
- 请求/响应转换规则

### 配置文件格式

```json
{
  "name": "qwen",
  "baseUrl": "https://portal.qwen.ai",
  "headers": {
    "Content-Type": "application/json",
    "User-Agent": "QwenCode/0.0.14 (linux; x64)",
    "Accept": "application/json"
  },
  "endpoints": {
    "/v1/chat/completions": {
      "method": "POST",
      "headers": {
        "Accept": "application/json, text/event-stream"
      },
      "streamSupport": true,
      "streamHeaders": {
        "Accept": "text/event-stream",
        "Cache-Control": "no-cache",
        "Connection": "keep-alive",
        "X-Accel-Buffering": "no"
      }
    }
  },
  "modelMapping": {
    "gpt-3.5-turbo": "qwen3-coder-plus",
    "gpt-4": "qwen3-coder-max"
  },
  "requestTransforms": {
    "renameFields": {
      "max_tokens": "max_completion_tokens"
    },
    "defaultValues": {
      "temperature": 0.7
    }
  },
  "responseOptions": {
    "forwardedHeaders": ["x-request-id", "x-ratelimit-remaining"]
  }
}
```

## API 端点

### 聊天完成
- `POST /v1/chat/completions` - 转发聊天完成请求

### 模型列表
- `GET /v1/models` - 获取可用模型列表

### 健康检查
- `GET /health` - 检查服务状态

## API 端点

### 聊天完成
- `POST /v1/chat/completions` - 转发聊天完成请求

### 模型列表
- `GET /v1/models` - 获取可用模型列表

### 健康检查
- `GET /health` - 检查服务状态

## 使用示例

### 非流式请求
```bash
curl -X POST http://localhost:8000/v1/chat/completions \
  -H "Authorization: Bearer your-api-key" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "qwen3-coder-plus",
    "messages": [
      {
        "role": "user",
        "content": "你好，请介绍一下你自己。"
      }
    ],
    "temperature": 0.7,
    "max_tokens": 1500
  }'
```

### 流式请求
```bash
curl -X POST http://localhost:8000/v1/chat/completions \
  -H "Authorization: Bearer your-api-key" \
  -H "Content-Type: application/json" \
  -H "Accept: text/event-stream" \
  -d '{
    "model": "qwen3-coder-plus",
    "messages": [
      {
        "role": "user",
        "content": "你好，请介绍一下你自己。"
      }
    ],
    "temperature": 0.7,
    "max_tokens": 1500,
    "stream": true
  }'
```

## 配置

可以通过修改 `src/main.rs` 中的 `ApiConfig` 结构来自定义模型映射和其他配置。

## 许可证

MIT 许可证