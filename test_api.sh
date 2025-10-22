#!/bin/bash

CONFIG="${1:-qwen}"
PORT="${2:-8000}"

echo "测试API转发功能 (transformer/${CONFIG}.json @ port ${PORT})..."

echo "提示：在另一个终端执行 'cargo run -- ${CONFIG} ${PORT}' 以启动代理服务。"

# 设置基础URL
BASE_URL="http://localhost:${PORT}"

# 测试健康检查端点
echo "测试健康检查端点..."
curl -s -X GET "$BASE_URL/health"

echo -e "\n测试模型列表端点..."
# 使用示例API密钥进行测试
API_KEY="j88R1cKdHY1EcYk9hO5vJIrV3f4rrtI5I9NuFyyTiFLDCXRhY8ooddL72AT1NqyHKMf3iGvib2W9XBYV8duUtw"
curl -s -X GET "$BASE_URL/v1/models" \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json"

echo -e "\n测试聊天完成端点..."
curl -s -X POST "$BASE_URL/v1/chat/completions" \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-3.5-turbo",
    "messages": [
      {
        "role": "user",
        "content": "你好，请介绍一下你自己。"
      }
    ],
    "temperature": 0.7,
    "max_tokens": 150
  }'

echo -e "\n测试完成！"