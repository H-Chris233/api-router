# API Router

一个将API请求转发为OpenAI兼容格式的服务，特别适用于将具有特殊认证要求的API转换为标准OpenAI格式。

## 功能特性

- 将非标准API请求转换为OpenAI兼容格式
- 支持流式传输（SSE）
- 自动处理认证头和User-Agent
- 支持模型名称映射
- 支持对话上下文传递
- CORS支持

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
export TARGET_API_BASE="https://portal.qwen.ai"
export DEFAULT_API_KEY="your-api-key-here"

# 运行服务
cargo run
```

### 环境变量

- `TARGET_API_BASE`：目标API的基础URL（默认：https://portal.qwen.ai）
- `DEFAULT_API_KEY`：默认API密钥（默认：预设的示例密钥）
- `USER_AGENT`：User-Agent字符串（默认：QwenCode/0.0.14 (linux; x64)）

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