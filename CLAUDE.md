# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概述

Light API Router 是一个轻量级的API请求转发服务，将API请求转换为OpenAI兼容格式。该项目使用Rust语言开发，基于Hyper框架构建，相比原版减少了依赖数量，提高了性能。

## 架构设计

- **核心框架**: 使用Hyper作为Web框架，提供异步HTTP服务
- **HTTP客户端**: 使用reqwest进行外部API请求
- **配置管理**: 通过transformer目录中的JSON配置文件动态加载API配置
- **请求处理**: 支持标准请求的代理转发

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

- `src/main.rs`: 主应用文件，包含所有路由和核心逻辑
- `transformer/`: 存放API配置文件的目录
  - `qwen.json`: 通义千问API配置
  - `openai.json`: OpenAI API配置
  - `generic.json`: 通用API配置模板

## 端点说明

- `POST /v1/chat/completions`: 转发聊天完成请求
- `GET /v1/models`: 获取可用模型列表
- `GET /health`: 健康检查端点

## 配置文件示例

配置文件包含以下关键部分：
- `baseUrl`: 目标API的基础URL
- `headers`: 全局请求头配置
- `modelMapping`: 模型名称映射规则