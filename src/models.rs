use serde::{Deserialize, Serialize};
use serde_json::Value;

// OpenAI兼容的请求结构
#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Choice {
    pub index: u32,
    pub message: Message,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CompletionRequest {
    pub model: String,
    pub prompt: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub echo: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_of: Option<u32>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn chat_completion_request_serializes_with_all_fields() {
        let request = ChatCompletionRequest {
            model: "test-model".to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: "Hello".to_string(),
            }],
            temperature: Some(0.7),
            stream: Some(true),
            max_tokens: Some(100),
        };

        let serialized = serde_json::to_value(&request).unwrap();
        assert_eq!(serialized["model"], "test-model");
        assert_eq!(serialized["messages"][0]["role"], "user");
        assert_eq!(serialized["messages"][0]["content"], "Hello");
        assert!((serialized["temperature"].as_f64().unwrap() - 0.7).abs() < 0.01);
        assert_eq!(serialized["stream"], true);
        assert_eq!(serialized["max_tokens"], 100);
    }

    #[test]
    fn chat_completion_request_deserializes_with_minimal_fields() {
        let json = json!({
            "model": "gpt-4",
            "messages": [
                {"role": "system", "content": "You are helpful"},
                {"role": "user", "content": "Hi"}
            ]
        });

        let request: ChatCompletionRequest = serde_json::from_value(json).unwrap();
        assert_eq!(request.model, "gpt-4");
        assert_eq!(request.messages.len(), 2);
        assert_eq!(request.messages[0].role, "system");
        assert_eq!(request.messages[1].content, "Hi");
        assert!(request.temperature.is_none());
        assert!(request.stream.is_none());
        assert!(request.max_tokens.is_none());
    }

    #[test]
    fn chat_completion_request_skips_none_fields_in_serialization() {
        let request = ChatCompletionRequest {
            model: "test".to_string(),
            messages: vec![],
            temperature: None,
            stream: None,
            max_tokens: None,
        };

        let serialized = serde_json::to_value(&request).unwrap();
        assert!(!serialized.as_object().unwrap().contains_key("temperature"));
        assert!(!serialized.as_object().unwrap().contains_key("stream"));
        assert!(!serialized.as_object().unwrap().contains_key("max_tokens"));
    }

    #[test]
    fn chat_completion_response_deserializes_correctly() {
        let json = json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1677652288,
            "model": "gpt-3.5-turbo",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello there!"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 20,
                "total_tokens": 30
            }
        });

        let response: ChatCompletionResponse = serde_json::from_value(json).unwrap();
        assert_eq!(response.id, "chatcmpl-123");
        assert_eq!(response.model, "gpt-3.5-turbo");
        assert_eq!(response.choices.len(), 1);
        assert_eq!(response.choices[0].message.content, "Hello there!");
        assert_eq!(response.usage.as_ref().unwrap().total_tokens, 30);
    }

    #[test]
    fn completion_request_supports_string_and_array_prompt() {
        let string_prompt = json!({
            "model": "text-davinci-003",
            "prompt": "Once upon a time"
        });
        let request: CompletionRequest = serde_json::from_value(string_prompt).unwrap();
        assert_eq!(request.prompt, json!("Once upon a time"));

        let array_prompt = json!({
            "model": "text-davinci-003",
            "prompt": ["First prompt", "Second prompt"]
        });
        let request: CompletionRequest = serde_json::from_value(array_prompt).unwrap();
        assert_eq!(request.prompt, json!(["First prompt", "Second prompt"]));
    }

    #[test]
    fn completion_request_deserializes_with_all_optional_fields() {
        let json = json!({
            "model": "text-davinci-003",
            "prompt": "Test",
            "suffix": "...",
            "max_tokens": 50,
            "temperature": 0.5,
            "top_p": 0.9,
            "n": 1,
            "stream": false,
            "logprobs": 5,
            "echo": true,
            "stop": "\n",
            "presence_penalty": 0.1,
            "frequency_penalty": 0.2,
            "best_of": 1,
            "user": "test-user"
        });

        let request: CompletionRequest = serde_json::from_value(json).unwrap();
        assert_eq!(request.model, "text-davinci-003");
        assert_eq!(request.suffix.as_deref(), Some("..."));
        assert_eq!(request.max_tokens, Some(50));
        assert_eq!(request.temperature, Some(0.5));
        assert_eq!(request.user.as_deref(), Some("test-user"));
    }

    #[test]
    fn embedding_request_handles_different_input_types() {
        let string_input = json!({
            "model": "text-embedding-ada-002",
            "input": "The quick brown fox"
        });
        let request: EmbeddingRequest = serde_json::from_value(string_input).unwrap();
        assert_eq!(request.input, json!("The quick brown fox"));

        let array_input = json!({
            "model": "text-embedding-ada-002",
            "input": ["First text", "Second text"]
        });
        let request: EmbeddingRequest = serde_json::from_value(array_input).unwrap();
        assert_eq!(request.input, json!(["First text", "Second text"]));
    }

    #[test]
    fn embedding_response_deserializes_with_multiple_embeddings() {
        let json = json!({
            "object": "list",
            "data": [
                {
                    "object": "embedding",
                    "embedding": [0.1, 0.2, 0.3],
                    "index": 0
                },
                {
                    "object": "embedding",
                    "embedding": [0.4, 0.5, 0.6],
                    "index": 1
                }
            ],
            "model": "text-embedding-ada-002",
            "usage": {
                "prompt_tokens": 8,
                "completion_tokens": 0,
                "total_tokens": 8
            }
        });

        let response: EmbeddingResponse = serde_json::from_value(json).unwrap();
        assert_eq!(response.data.len(), 2);
        assert_eq!(response.data[0].embedding, vec![0.1, 0.2, 0.3]);
        assert_eq!(response.data[1].index, 1);
        assert_eq!(response.model.as_deref(), Some("text-embedding-ada-002"));
    }

    #[test]
    fn audio_transcription_request_deserializes_with_optional_fields() {
        let json = json!({
            "model": "whisper-1",
            "prompt": "The quick brown fox",
            "response_format": "json",
            "temperature": 0.0,
            "language": "en"
        });

        let request: AudioTranscriptionRequest = serde_json::from_value(json).unwrap();
        assert_eq!(request.model, "whisper-1");
        assert_eq!(request.prompt.as_deref(), Some("The quick brown fox"));
        assert_eq!(request.response_format.as_deref(), Some("json"));
        assert_eq!(request.temperature, Some(0.0));
        assert_eq!(request.language.as_deref(), Some("en"));
    }

    #[test]
    fn audio_translation_request_deserializes_correctly() {
        let json = json!({
            "model": "whisper-1",
            "prompt": "Translate this",
            "response_format": "text",
            "temperature": 0.2
        });

        let request: AudioTranslationRequest = serde_json::from_value(json).unwrap();
        assert_eq!(request.model, "whisper-1");
        assert_eq!(request.prompt.as_deref(), Some("Translate this"));
    }

    #[test]
    fn anthropic_messages_request_serializes_correctly() {
        let request = AnthropicMessagesRequest {
            model: "claude-3-opus".to_string(),
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: "Hello Claude".to_string(),
            }],
            max_tokens: 1024,
            system: Some("You are helpful".to_string()),
            temperature: Some(0.7),
            top_p: Some(0.9),
            top_k: Some(40),
            stream: Some(false),
            stop_sequences: Some(vec!["STOP".to_string()]),
        };

        let serialized = serde_json::to_value(&request).unwrap();
        assert_eq!(serialized["model"], "claude-3-opus");
        assert_eq!(serialized["max_tokens"], 1024);
        assert_eq!(serialized["system"], "You are helpful");
        assert_eq!(serialized["stop_sequences"][0], "STOP");
    }

    #[test]
    fn anthropic_messages_response_deserializes_correctly() {
        let json = json!({
            "id": "msg_123",
            "type": "message",
            "role": "assistant",
            "content": [{
                "type": "text",
                "text": "Hello! How can I help you?"
            }],
            "model": "claude-3-opus-20240229",
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {
                "input_tokens": 10,
                "output_tokens": 25
            }
        });

        let response: AnthropicMessagesResponse = serde_json::from_value(json).unwrap();
        assert_eq!(response.id, "msg_123");
        assert_eq!(response.response_type, "message");
        assert_eq!(response.role, "assistant");
        assert_eq!(response.content.len(), 1);
        assert_eq!(response.content[0].text, "Hello! How can I help you?");
        assert_eq!(response.usage.input_tokens, 10);
        assert_eq!(response.usage.output_tokens, 25);
    }

    #[test]
    fn usage_struct_serializes_and_deserializes() {
        let usage = Usage {
            prompt_tokens: 15,
            completion_tokens: 20,
            total_tokens: 35,
        };

        let json = serde_json::to_value(&usage).unwrap();
        assert_eq!(json["prompt_tokens"], 15);
        assert_eq!(json["completion_tokens"], 20);
        assert_eq!(json["total_tokens"], 35);

        let deserialized: Usage = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized.prompt_tokens, 15);
        assert_eq!(deserialized.completion_tokens, 20);
        assert_eq!(deserialized.total_tokens, 35);
    }
}
