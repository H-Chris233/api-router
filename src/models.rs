//! OpenAI 兼容的数据模型定义
//! 
//! 包含请求和响应的数据结构，用于与上游 API 通信

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Chat Completion 请求结构（OpenAI 格式）
#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct ChatCompletionRequest {
    /// 模型名称
    pub model: String,
    /// 对话消息列表
    pub messages: Vec<Message>,
    /// 采样温度（0-2），默认 1
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// 是否启用流式响应
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// 最大生成 token 数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
}

/// 对话消息结构
#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct Message {
    /// 消息角色：system, user, assistant
    pub role: String,
    /// 消息内容
    pub content: String,
}

/// Chat Completion 响应结构
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ChatCompletionResponse {
    /// 唯一标识符
    pub id: String,
    /// 对象类型，通常为 "chat.completion"
    pub object: String,
    /// 创建时间戳（Unix 时间）
    pub created: u64,
    /// 使用的模型名称
    pub model: String,
    /// 生成的选项列表
    pub choices: Vec<Choice>,
    /// token 使用统计（可选）
    pub usage: Option<Usage>,
}

/// 生成的单个选项
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Choice {
    /// 选项索引
    pub index: u32,
    /// 生成的消息
    pub message: Message,
    /// 停止原因：stop, length, null
    pub finish_reason: Option<String>,
}

/// Token 使用统计
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Usage {
    /// 提示词 token 数
    pub prompt_tokens: u32,
    /// 生成的 token 数
    pub completion_tokens: u32,
    /// 总 token 数
    pub total_tokens: u32,
}

/// Text Completion 请求结构（OpenAI 格式）
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CompletionRequest {
    /// 模型名称
    pub model: String,
    /// 提示词（字符串或字符串数组）
    pub prompt: Value,
    /// 生成文本的后缀
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
    /// 最大生成 token 数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// 采样温度
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// 核采样参数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// 生成选项数量
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
    /// 是否启用流式响应
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// 日志概率数量
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<u32>,
    /// 是否回显提示词
    #[serde(skip_serializing_if = "Option::is_none")]
    pub echo: Option<bool>,
    /// 停止序列
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Value>,
    /// 存在惩罚
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
    /// 频率惩罚
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    /// best_of 参数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_of: Option<u32>,
    /// 用户标识
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CompletionChoice {
    pub index: u32,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<Value>,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CompletionResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<CompletionChoice>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EmbeddingRequest {
    pub model: String,
    pub input: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EmbeddingData {
    pub object: String,
    pub embedding: Vec<f32>,
    pub index: u32,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EmbeddingResponse {
    pub object: String,
    pub data: Vec<EmbeddingData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AudioTranscriptionRequest {
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AudioTranslationRequest {
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AudioTranscriptionResponse {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segments: Option<Value>,
}

pub type AudioTranslationResponse = AudioTranscriptionResponse;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn chat_completion_request_serializes_correctly() {
        let request = ChatCompletionRequest {
            model: "gpt-4".to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: "Hello".to_string(),
            }],
            temperature: Some(0.7),
            stream: Some(true),
            max_tokens: Some(100),
        };

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["model"], "gpt-4");
        assert_eq!(json["messages"][0]["role"], "user");
        assert_eq!(json["messages"][0]["content"], "Hello");
        assert!((json["temperature"].as_f64().unwrap() - 0.7).abs() < 0.01);
        assert_eq!(json["stream"], true);
        assert_eq!(json["max_tokens"], 100);
    }

    #[test]
    fn chat_completion_request_skips_none_fields() {
        let request = ChatCompletionRequest {
            model: "gpt-4".to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: "Hello".to_string(),
            }],
            temperature: None,
            stream: None,
            max_tokens: None,
        };

        let json = serde_json::to_value(&request).unwrap();
        assert!(!json.as_object().unwrap().contains_key("temperature"));
        assert!(!json.as_object().unwrap().contains_key("stream"));
        assert!(!json.as_object().unwrap().contains_key("max_tokens"));
    }

    #[test]
    fn chat_completion_response_deserializes_correctly() {
        let json_str = r#"{
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1677652288,
            "model": "gpt-4",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello, how can I help?"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 20,
                "total_tokens": 30
            }
        }"#;

        let response: ChatCompletionResponse = serde_json::from_str(json_str).unwrap();
        assert_eq!(response.id, "chatcmpl-123");
        assert_eq!(response.object, "chat.completion");
        assert_eq!(response.model, "gpt-4");
        assert_eq!(response.choices.len(), 1);
        assert_eq!(
            response.choices[0].message.content,
            "Hello, how can I help?"
        );
        assert!(response.usage.is_some());
        assert_eq!(response.usage.unwrap().total_tokens, 30);
    }

    #[test]
    fn completion_request_handles_string_and_array_prompt() {
        let string_prompt = CompletionRequest {
            model: "davinci".to_string(),
            prompt: json!("Hello world"),
            suffix: None,
            max_tokens: Some(50),
            temperature: Some(0.5),
            top_p: None,
            n: None,
            stream: None,
            logprobs: None,
            echo: None,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            best_of: None,
            user: None,
        };

        let serialized = serde_json::to_value(&string_prompt).unwrap();
        assert_eq!(serialized["prompt"], "Hello world");

        let array_prompt = CompletionRequest {
            model: "davinci".to_string(),
            prompt: json!(["Hello", "world"]),
            suffix: None,
            max_tokens: Some(50),
            temperature: Some(0.5),
            top_p: None,
            n: None,
            stream: None,
            logprobs: None,
            echo: None,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            best_of: None,
            user: None,
        };

        let serialized = serde_json::to_value(&array_prompt).unwrap();
        assert_eq!(serialized["prompt"], json!(["Hello", "world"]));
    }

    #[test]
    fn embedding_request_serializes_with_optional_fields() {
        let request = EmbeddingRequest {
            model: "text-embedding-ada-002".to_string(),
            input: json!("Hello world"),
            user: Some("user-123".to_string()),
            encoding_format: Some("float".to_string()),
            dimensions: Some(1536),
        };

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["model"], "text-embedding-ada-002");
        assert_eq!(json["user"], "user-123");
        assert_eq!(json["encoding_format"], "float");
        assert_eq!(json["dimensions"], 1536);
    }

    #[test]
    fn embedding_response_deserializes_correctly() {
        let json_str = r#"{
            "object": "list",
            "data": [{
                "object": "embedding",
                "embedding": [0.1, 0.2, 0.3],
                "index": 0
            }],
            "model": "text-embedding-ada-002",
            "usage": {
                "prompt_tokens": 5,
                "completion_tokens": 0,
                "total_tokens": 5
            }
        }"#;

        let response: EmbeddingResponse = serde_json::from_str(json_str).unwrap();
        assert_eq!(response.object, "list");
        assert_eq!(response.data.len(), 1);
        assert_eq!(response.data[0].embedding, vec![0.1, 0.2, 0.3]);
        assert_eq!(response.model, Some("text-embedding-ada-002".to_string()));
    }

    #[test]
    fn audio_transcription_request_serializes_with_optional_language() {
        let request = AudioTranscriptionRequest {
            model: "whisper-1".to_string(),
            prompt: Some("Transcribe this".to_string()),
            response_format: Some("json".to_string()),
            temperature: Some(0.0),
            language: Some("en".to_string()),
        };

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["model"], "whisper-1");
        assert_eq!(json["language"], "en");
    }

    #[test]
    fn anthropic_messages_request_serializes_correctly() {
        let request = AnthropicMessagesRequest {
            model: "claude-3-opus".to_string(),
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: "Hello".to_string(),
            }],
            max_tokens: 1024,
            system: Some("You are helpful".to_string()),
            temperature: Some(1.0),
            top_p: Some(0.9),
            top_k: Some(40),
            stream: Some(false),
            stop_sequences: Some(vec!["STOP".to_string()]),
        };

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["model"], "claude-3-opus");
        assert_eq!(json["max_tokens"], 1024);
        assert_eq!(json["system"], "You are helpful");
        assert_eq!(json["stop_sequences"][0], "STOP");
    }

    #[test]
    fn anthropic_messages_response_deserializes_correctly() {
        let json_str = r#"{
            "id": "msg_123",
            "type": "message",
            "role": "assistant",
            "content": [{
                "type": "text",
                "text": "Hello! How can I help?"
            }],
            "model": "claude-3-opus",
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {
                "input_tokens": 10,
                "output_tokens": 20
            }
        }"#;

        let response: AnthropicMessagesResponse = serde_json::from_str(json_str).unwrap();
        assert_eq!(response.id, "msg_123");
        assert_eq!(response.response_type, "message");
        assert_eq!(response.role, "assistant");
        assert_eq!(response.content.len(), 1);
        assert_eq!(response.content[0].text, "Hello! How can I help?");
        assert_eq!(response.usage.input_tokens, 10);
    }

    #[test]
    fn message_clone_works() {
        let msg = Message {
            role: "user".to_string(),
            content: "test".to_string(),
        };
        let cloned = msg.clone();
        assert_eq!(msg.role, cloned.role);
        assert_eq!(msg.content, cloned.content);
    }

    #[test]
    fn usage_clone_works() {
        let usage = Usage {
            prompt_tokens: 10,
            completion_tokens: 20,
            total_tokens: 30,
        };
        let cloned = usage.clone();
        assert_eq!(usage.prompt_tokens, cloned.prompt_tokens);
        assert_eq!(usage.completion_tokens, cloned.completion_tokens);
        assert_eq!(usage.total_tokens, cloned.total_tokens);
    }
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct AnthropicMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct AnthropicMessagesRequest {
    pub model: String,
    pub messages: Vec<AnthropicMessage>,
    pub max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AnthropicContentBlock {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AnthropicUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AnthropicMessagesResponse {
    pub id: String,
    #[serde(rename = "type")]
    pub response_type: String,
    pub role: String,
    pub content: Vec<AnthropicContentBlock>,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequence: Option<String>,
    pub usage: AnthropicUsage,
}
