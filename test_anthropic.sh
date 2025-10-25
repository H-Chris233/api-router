#!/bin/bash

PORT="${1:-8000}"

echo "测试 Anthropic API 端点支持 (transformer/anthropic.json @ port ${PORT})..."

echo "提示：在另一个终端执行 'cargo run -- anthropic ${PORT}' 以启动代理服务。"

# 设置基础URL
BASE_URL="http://localhost:${PORT}"

# 使用示例API密钥进行测试
API_KEY="j88R1cKdHY1EcYk9hO5vJIrV3f4rrtI5I9NuFyyTiFLDCXRhY8ooddL72AT1NqyHKMf3iGvib2W9XBYV8duUtw"

# 测试健康检查端点
echo "测试健康检查端点..."
curl -s -X GET "$BASE_URL/health"

echo -e "\n\n测试模型列表端点..."
curl -s -X GET "$BASE_URL/v1/models" \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json"

echo -e "\n\n测试 Anthropic /v1/messages 端点（非流式）..."
curl -s -X POST "$BASE_URL/v1/messages" \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -H "anthropic-version: 2023-06-01" \
  -d '{
    "model": "claude-3-5-sonnet-20240620",
    "max_tokens": 150,
    "messages": [
      {
        "role": "user",
        "content": "你好，请介绍一下你自己。"
      }
    ],
    "temperature": 0.7
  }'

echo -e "\n\n测试 Anthropic /v1/messages 端点（带 system）..."
curl -s -X POST "$BASE_URL/v1/messages" \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -H "anthropic-version: 2023-06-01" \
  -d '{
    "model": "claude-3-haiku-20240307",
    "max_tokens": 100,
    "system": "你是一个友好的助手。",
    "messages": [
      {
        "role": "user",
        "content": "什么是Rust编程语言？"
      }
    ]
  }'

echo -e "\n\n测试 Anthropic /v1/messages 端点（流式）..."
curl -s -X POST "$BASE_URL/v1/messages" \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -H "anthropic-version: 2023-06-01" \
  -d '{
    "model": "claude-3-5-sonnet-20240620",
    "max_tokens": 50,
    "stream": true,
    "messages": [
      {
        "role": "user",
        "content": "数到5"
      }
    ]
  }'

echo -e "\n\n测试 OpenAI 格式到 Anthropic 转换..."
curl -s -X POST "$BASE_URL/v1/chat/completions" \
  -H "Authorization: Bearer $API_KEY" \
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

echo -e "\n\n测试完成！"
