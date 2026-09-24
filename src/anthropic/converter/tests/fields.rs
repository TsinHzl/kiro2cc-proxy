//! converter 测试（自 tests.rs 拆分，纯代码搬移）
#![cfg(test)]

use super::super::convert::convert_request;
use super::super::fields::model_max_output_tokens;
#[allow(unused_imports)]
use crate::anthropic::types::ContentBlock as _;
use crate::anthropic::types::MessagesRequest;
#[allow(unused_imports)]
use crate::kiro::model::requests::conversation::Message;

#[test]
fn test_model_max_output_tokens_opus_5() {
    assert_eq!(model_max_output_tokens("claude-opus-5"), 128000);
    assert_eq!(model_max_output_tokens("claude-opus-5-thinking"), 128000);
    assert_eq!(model_max_output_tokens("Claude-Opus-5"), 128000);
    assert_eq!(model_max_output_tokens("Claude Opus 5"), 128000);

    // 回归：其他档位不变
    assert_eq!(model_max_output_tokens("claude-opus-4.6"), 64000);
    assert_eq!(model_max_output_tokens("claude-opus-4.5"), 64000);
    assert_eq!(model_max_output_tokens("claude-sonnet-5"), 64000);
    assert_eq!(model_max_output_tokens("claude-haiku-4.5"), 64000);
}

#[test]
fn test_additional_model_request_fields_max_tokens_minimum_applies_to_all_claude_models() {
    // 回归测试：Kiro 侧 schema 对 max_tokens 强制 minimum = 1024，对支持
    // additionalModelRequestFields 的 Claude 代际生效（实测 claude-sonnet-4-6
    // 在 max_tokens=200 时同样报 400 "must have a minimum value of 1024.0"），
    // 不能只对 cap==128000（opus-4.7/4.8/5）的模型生效。
    // 注意："4.5" 代际（sonnet/opus/haiku）整体跳过该字段（400 实测，
    // 见 build_additional_model_request_fields 文档），故不在本测试范围。
    use crate::anthropic::types::Message as AnthropicMessage;

    for model in [
        "claude-sonnet-4-6",
        "claude-sonnet-5",
        "claude-opus-4-6",
        "claude-opus-5",
    ] {
        let req = MessagesRequest {
            model: model.to_string(),
            max_tokens: 200, // 低于 1024 的边界输入
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: serde_json::json!("Hello"),
            }],
            stream: false,
            system: None,
            tools: None,
            tool_choice: None,
            thinking: None,
            output_config: None,
            metadata: None,
        };

        let result = convert_request(&req).unwrap();
        let fields = result
            .additional_model_request_fields
            .expect("非 GPT 模型应构建 additionalModelRequestFields");
        let max_tokens = fields["max_tokens"].as_i64().unwrap_or(0);

        assert!(
            max_tokens >= 1024,
            "model={model} max_tokens 应被下限收敛到至少 1024，实际={max_tokens}"
        );
    }
}

#[test]
fn test_output_config_effort_passthrough_with_default() {
    // effort 透传 + 默认注入（回退 issue #40 的仅透传行为）：客户端显式携带
    // output_config 时按原值转发；未携带时默认注入 effort="high"。
    use crate::anthropic::types::{Message as AnthropicMessage, OutputConfig};

    // 场景 1：客户端未携带 output_config → 默认注入 effort="high"
    let req = MessagesRequest {
        model: "claude-sonnet-5".to_string(),
        max_tokens: 32000,
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::json!("Hello"),
        }],
        stream: false,
        system: None,
        tools: None,
        tool_choice: None,
        thinking: None,
        output_config: None,
        metadata: None,
    };
    let result = convert_request(&req).unwrap();
    let fields = result
        .additional_model_request_fields
        .expect("非 4.5 代模型应构建 additionalModelRequestFields");
    assert_eq!(
        fields["output_config"]["effort"].as_str(),
        Some("high"),
        "客户端未携带 output_config 时应默认注入 effort=\"high\""
    );

    // 场景 2：客户端显式携带 output_config → effort 按客户端值透传
    let req = MessagesRequest {
        model: "claude-sonnet-5".to_string(),
        max_tokens: 32000,
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::json!("Hello"),
        }],
        stream: false,
        system: None,
        tools: None,
        tool_choice: None,
        thinking: None,
        output_config: Some(OutputConfig {
            effort: "low".to_string(),
            format: None,
        }),
        metadata: None,
    };
    let result = convert_request(&req).unwrap();
    let fields = result
        .additional_model_request_fields
        .expect("非 4.5 代模型应构建 additionalModelRequestFields");
    assert_eq!(
        fields["output_config"]["effort"].as_str(),
        Some("low"),
        "effort 必须按客户端传入值透传"
    );

    // 场景 3：客户端携带 output_config 但 effort 为空串 → 兜底为 "high"
    let req = MessagesRequest {
        model: "claude-sonnet-5".to_string(),
        max_tokens: 32000,
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::json!("Hello"),
        }],
        stream: false,
        system: None,
        tools: None,
        tool_choice: None,
        thinking: None,
        output_config: Some(OutputConfig {
            effort: "".to_string(),
            format: None,
        }),
        metadata: None,
    };
    let result = convert_request(&req).unwrap();
    let fields = result
        .additional_model_request_fields
        .expect("非 4.5 代模型应构建 additionalModelRequestFields");
    assert_eq!(
        fields["output_config"]["effort"].as_str(),
        Some("high"),
        "空串 effort 应兜底为 \"high\"，避免转发非法值"
    );
}

#[test]
fn test_4_5_generation_skips_fields_even_with_output_config() {
    // 回归测试（CR #3 补充）：4.5 代际模型即使客户端携带 output_config，
    // 也必须整体跳过 additionalModelRequestFields（该代际 schema 不接受
    // 任何结构化字段，见 build_additional_model_request_fields 文档）。
    use crate::anthropic::types::{Message as AnthropicMessage, OutputConfig};

    for model in ["claude-sonnet-4-5", "claude-opus-4-5", "claude-haiku-4-5"] {
        let req = MessagesRequest {
            model: model.to_string(),
            max_tokens: 32000,
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: serde_json::json!("Hello"),
            }],
            stream: false,
            system: None,
            tools: None,
            tool_choice: None,
            thinking: None,
            output_config: Some(OutputConfig {
                effort: "low".to_string(),
                format: None,
            }),
            metadata: None,
        };
        let result = convert_request(&req).unwrap();
        assert!(
            result.additional_model_request_fields.is_none(),
            "{model} 携带 output_config 时仍应整体跳过 additionalModelRequestFields"
        );
    }
}

#[test]
fn test_gpt_5_6_additional_model_request_fields_is_none() {
    // 实测（抓包）：gpt-5.6-* 使用 additionalModelRequestFields.reasoning.effort 路径，
    // 不接受 output_config / max_tokens（均返回 400 REQUEST_BODY_INVALID）。
    // 本测试验证 GPT 系生成正确的 reasoning.effort 结构，且默认注入 effort="high"。
    use crate::anthropic::types::Message as AnthropicMessage;

    let req = MessagesRequest {
        model: "gpt-5.6-sol".to_string(),
        max_tokens: 128000,
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::json!("Hello"),
        }],
        stream: false,
        system: None,
        tools: None,
        tool_choice: None,
        thinking: None,
        output_config: None,
        metadata: None,
    };

    let result = convert_request(&req).unwrap();
    let fields = result
        .additional_model_request_fields
        .as_ref()
        .expect("gpt-5.6 系列必须包含 additionalModelRequestFields");
    assert_eq!(
        fields["reasoning"]["effort"].as_str(),
        Some("high"),
        "gpt-5.6 系列未携带 output_config 时应默认注入 reasoning.effort=\"high\""
    );
    assert!(
        fields.get("output_config").is_none(),
        "gpt-5.6 系列不得包含 output_config 字段"
    );
    assert!(
        fields.get("max_tokens").is_none(),
        "gpt-5.6 系列不得包含 max_tokens 字段"
    );
}

#[test]
fn test_claude_4_5_generation_additional_model_request_fields_is_none() {
    // 回归测试（haiku-4.5 全部请求 400 修复）：实测 claude-sonnet-4.5 /
    // claude-opus-4.5 / claude-haiku-4.5 的 Kiro schema 均不接受
    // additionalModelRequestFields（thinking/output_config/max_tokens 均报
    // 400 REQUEST_BODY_INVALID），需整体省略该字段。历史上该跳过逻辑曾被
    // 重构为仅判断 GPT 系而丢失，导致 haiku-4.5 全量 502。
    use crate::anthropic::types::{Message as AnthropicMessage, Thinking};

    // 覆盖 /v1/models 暴露的带日期变体（含 -thinking）及简写形式
    for model in [
        "claude-haiku-4-5",
        "claude-sonnet-4-5",
        "claude-sonnet-4-5-20250929",
        "claude-sonnet-4-5-20250929-thinking",
        "claude-opus-4-5",
        "claude-opus-4-5-20251101",
        "claude-opus-4-5-20251101-thinking",
    ] {
        let req = MessagesRequest {
            model: model.to_string(),
            max_tokens: 32000,
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: serde_json::json!("Hello"),
            }],
            stream: false,
            system: None,
            tools: None,
            tool_choice: None,
            thinking: Some(Thinking {
                thinking_type: "enabled".to_string(),
                budget_tokens: 24576,
            }),
            output_config: None,
            metadata: None,
        };

        let result = convert_request(&req).unwrap();
        assert!(
            result.additional_model_request_fields.is_none(),
            "model={model} 4.5 代际必须整体省略 additionalModelRequestFields 字段"
        );
    }
}
