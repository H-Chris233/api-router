// OpenAI API 配置
module.exports = {
  name: "openai",
  baseUrl: "https://api.openai.com",
  headers: {
    "Content-Type": "application/json",
    "User-Agent": "QwenCode/0.0.14 (linux; x64)",
    "Accept": "application/json"
  },
  endpoints: {
    "/v1/chat/completions": {
      method: "POST",
      headers: {
        "OpenAI-Beta": "assistants=v1"
      },
      streamSupport: true,
      streamHeaders: {
        "Accept": "text/event-stream"
      }
    },
    "/v1/models": {
      method: "GET",
      headers: {}
    },
    "/v1/completions": {
      method: "POST",
      headers: {}
    },
    "/v1/embeddings": {
      method: "POST",
      headers: {}
    }
  },
  modelMapping: {
    "qwen3-coder-plus": "gpt-3.5-turbo",
    "qwen3-coder-max": "gpt-4"
  },
  requestTransforms: {
    renameFields: {},
    defaultValues: {
      "temperature": 1.0
    }
  },
  responseOptions: {
    forwardedHeaders: ["openai-organization", "openai-processing-ms", "x-request-id"]
  }
};