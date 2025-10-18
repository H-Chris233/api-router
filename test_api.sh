#!/bin/bash

echo "测试API转发功能..."

# 设置基础URL
BASE_URL="http://localhost:8000"

# 测试健康检查端点
echo "测试健康检查端点..."
curl -s -X GET "$BASE_URL/health" | jq

echo -e "\n测试模型列表端点..."
# 使用示例API密钥进行测试
API_KEY="j88R1cKdHY1EcYk9hO5vJIrV3f4rrtI5I9NuFyyTiFLDCXRhY8ooddL72AT1NqyHKMf3iGvib2W9XBYV8duUtw"
curl -s -X GET "$BASE_URL/v1/models" \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" | jq | head -20

echo -e "\n测试聊天完成端点（非流式）..."
curl -s -X POST "$BASE_URL/v1/chat/completions" \
  -H "Authorization: Bearer $API_KEY" \
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
    "max_tokens": 150
  }' | jq | head -20

echo -e "\n测试聊天完成端点（流式）..."
curl -N -X POST "$BASE_URL/v1/chat/completions" \
  -H "Authorization: Bearer $API_KEY" \
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
    "max_tokens": 150,
    "stream": true
  }' | head -10

echo -e "\n测试完成！"