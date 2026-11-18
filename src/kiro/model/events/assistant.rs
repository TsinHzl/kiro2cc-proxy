// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 助手响应事件
//!
//! 处理 assistantResponseEvent 类型的事件

use serde::{Deserialize, Serialize};

use crate::kiro::parser::error::ParseResult;
use crate::kiro::parser::frame::Frame;

use super::base::EventPayload;

/// 原生推理增量；文本与签名可能分别到达。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReasoningContentEvent {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub signature: String,
}

impl EventPayload for ReasoningContentEvent {
    fn from_frame(frame: &Frame) -> ParseResult<Self> {
        frame.payload_as_json()
    }
}

/// 助手响应事件
///
/// 包含 AI 助手的流式响应内容
///
/// # 设计说明
///
/// `content` 为生产读取字段；`model_id` 当前仅供测试断言使用；其他 API 返回的
/// 未知字段由 serde 默认忽略，不影响反序列化。
///
/// # 示例
///
/// ```rust
/// use kiro_rs::kiro::model::events::AssistantResponseEvent;
///
/// let json = r#"{"content":"Hello, world!"}"#;
/// let event: AssistantResponseEvent = serde_json::from_str(json).unwrap();
/// assert_eq!(event.content, "Hello, world!");
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantResponseEvent {
    /// 响应内容片段
    #[serde(default)]
    pub content: String,

    /// 模型 ID（如 "claude-sonnet-4.6"、"deepseek-3.2"）
    #[serde(default)]
    pub model_id: String,
}

impl EventPayload for AssistantResponseEvent {
    fn from_frame(frame: &Frame) -> ParseResult<Self> {
        frame.payload_as_json()
    }
}

impl std::fmt::Display for AssistantResponseEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_simple() {
        let json = r#"{"content":"Hello, world!"}"#;
        let event: AssistantResponseEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.content, "Hello, world!");
        assert_eq!(event.model_id, "");
    }

    #[test]
    fn test_deserialize_with_model_id() {
        let json = r#"{"content":"你好","modelId":"claude-sonnet-4.6"}"#;
        let event: AssistantResponseEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.content, "你好");
        assert_eq!(event.model_id, "claude-sonnet-4.6");
    }

    #[test]
    fn test_deserialize_with_extra_fields() {
        // 确保包含额外字段时反序列化不会失败
        let json = r#"{
            "content": "Done",
            "modelId": "deepseek-3.2",
            "conversationId": "conv-123",
            "messageId": "msg-456",
            "messageStatus": "COMPLETED",
            "followupPrompt": {
                "content": "Would you like me to explain further?",
                "userIntent": "EXPLAIN_CODE_SELECTION"
            }
        }"#;
        let event: AssistantResponseEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.content, "Done");
        assert_eq!(event.model_id, "deepseek-3.2");
    }

    #[test]
    fn test_serialize_minimal() {
        let event = AssistantResponseEvent {
            content: "Test".to_string(),
            model_id: "claude-sonnet-4.6".to_string(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"content\":\"Test\""));
        assert!(json.contains("\"modelId\":\"claude-sonnet-4.6\""));
    }

    #[test]
    fn test_display() {
        let event = AssistantResponseEvent {
            content: "test".to_string(),
            ..Default::default()
        };
        assert_eq!(format!("{}", event), "test");
    }
}
