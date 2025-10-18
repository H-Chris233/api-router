// Qwen API 配置
module.exports = {
  name: "qwen",
  baseUrl: "https://portal.qwen.ai",
  headers: {
    "Content-Type": "application/json",
    "User-Agent": "QwenCode/0.0.14 (linux; x64)",
    "Accept": "application/json"
  },
  endpoints: {
    "/v1/chat/completions": {
      method: "POST",
      headers: {
        "Accept": "application/json, text/event-stream"
      },
      // 特殊处理选项
      streamSupport: true,
      streamHeaders: {
        "Accept": "text/event-stream",
        "Cache-Control": "no-cache",
        "Connection": "keep-alive",
        "X-Accel-Buffering": "no"
      }
    },
    "/v1/models": {
      method: "GET",
      headers: {}
    },
    "/v1/completions": {
      method: "POST",
      headers: {}
    }
  },
  // 模型映射
  modelMapping: {
    "gpt-3.5-turbo": "qwen3-coder-plus",
    "gpt-4": "qwen3-coder-max",
    "gpt-4-turbo": "qwen3-coder-turbo"
  },
  // 请求参数转换规则
  requestTransforms: {
    // 重命名字段
    renameFields: {
      "max_tokens": "max_completion_tokens"
    },
    // 添加默认值
    defaultValues: {
      "temperature": 0.7
    }
  },
  // 响应处理选项
  responseOptions: {
    // 需要转发的响应头
    forwardedHeaders: ["x-request-id", "x-ratelimit-remaining", "x-ratelimit-reset"]
  }
};