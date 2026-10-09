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

    // haiku 5.5：1M 窗口代际，与 opus 5 同档 128K
    assert_eq!(model_max_output_tokens("claude-haiku-5.5"), 128000);
    assert_eq!(model_max_output_tokens("claude-haiku-5.5-thinking"), 128000);
    // 空格分隔别名与映射层口径一致
    assert_eq!(model_max_output_tokens("Claude Haiku 5.5"), 128000);
    // 点号变体
    assert_eq!(model_max_output_tokens("claude-haiku.5.5"), 128000);

    // 回归：非 5.5 的 haiku-5.x 走标准 64K 档
    assert_eq!(model_max_output_tokens("claude-haiku-5.0"), 64000);
    assert_eq!(model_max_output_tokens("claude-haiku-5.6"), 64000);

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
    // effort 透传 + 默认注入：客户端显式携带 output_config 时按原值转发；
    // 未携带时默认注入 effort="low"（issue #40：high 兜底致 sonnet-5 TTFB 慢）。
    use crate::anthropic::types::{Message as AnthropicMessage, OutputConfig};

    // 场景 1：客户端未携带 output_config → 默认注入 effort="low"
    // （issue #40：high 兜底导致 sonnet-5 反代链路 TTFB 慢于直连，兜底下调为 low）
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
        Some("low"),
        "客户端未携带 output_config 时应默认注入 effort=\"low\""
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

    // 场景 3：客户端携带 output_config 但 effort 为空串 → 兜底为 "low"
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
        Some("low"),
        "空串 effort 应兜底为 \"low\"，避免转发非法值"
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

#[test]
fn test_claude_thinking_injection_matrix() {
    // thinking 注入矩阵：Claude 系请求 thinking.type == "adaptive" 时注入
    // `thinking: {"type": "adaptive"}`（Kiro 私有协议唯一实证接受形态），
    // 与 output_config / max_tokens 合并共存；enabled（已走 history[0]
    // `<thinking_mode>` 文本标签协议，不得再叠加原生字段，否则客户端规则遵从性回退）/
    // disabled / 未携带时不注入；GPT 系走 reasoning.effort 不注入 thinking；
    // 4.5 代际整体跳过（另行覆盖，见
    // test_claude_4_5_generation_additional_model_request_fields_is_none）。
    use crate::anthropic::types::{Message as AnthropicMessage, Thinking};

    fn base_req(model: &str, thinking: Option<Thinking>) -> MessagesRequest {
        MessagesRequest {
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
            thinking,
            output_config: None,
            metadata: None,
        }
    }

    // 场景 1：adaptive → 注入，且与其他字段共存
    {
        let req = base_req(
            "claude-sonnet-4-6",
            Some(Thinking {
                thinking_type: "adaptive".to_string(),
                budget_tokens: 20000,
            }),
        );
        let result = convert_request(&req).unwrap();
        let fields = result
            .additional_model_request_fields
            .expect("Claude 非 4.5 代际应构建 additionalModelRequestFields");
        assert_eq!(fields["thinking"]["type"], serde_json::json!("adaptive"));
        assert!(
            fields.get("output_config").is_some() && fields.get("max_tokens").is_some(),
            "thinking 字段须与 output_config / max_tokens 合并共存"
        );
    }

    // 场景 2：enabled / disabled / 未携带 → 不注入。
    // enabled 已由 history[0] 文本标签协议承载，叠加原生字段会使同一请求携带
    // 两套 thinking 控制信号，导致客户端规则遵从性回退（v3.4.1 回归）。
    for thinking in [
        None,
        Some(Thinking {
            thinking_type: "enabled".to_string(),
            budget_tokens: 20000,
        }),
        Some(Thinking {
            thinking_type: "disabled".to_string(),
            budget_tokens: 20000,
        }),
    ] {
        let req = base_req("claude-sonnet-4-6", thinking);
        let result = convert_request(&req).unwrap();
        let fields = result
            .additional_model_request_fields
            .expect("Claude 非 4.5 代际应构建 additionalModelRequestFields");
        assert!(
            fields.get("thinking").is_none(),
            "无 thinking / enabled / disabled 时不得注入原生 thinking 字段"
        );
    }

    // 场景 2b：enabled 仍走文本标签协议（history[0] 含 thinking_mode 标签）
    {
        let mut req = base_req(
            "claude-sonnet-4-6",
            Some(Thinking {
                thinking_type: "enabled".to_string(),
                budget_tokens: 20000,
            }),
        );
        req.system = Some(vec![crate::anthropic::types::SystemMessage {
            text: "rule".to_string(),
        }]);
        let result = convert_request(&req).unwrap();
        let first = serde_json::to_string(&result.conversation_state.history[0]).unwrap();
        assert!(first.contains("<thinking_mode>enabled</thinking_mode>"));
    }

    // 场景 3：GPT 系携带 thinking → 走 reasoning.effort，不注入 thinking
    let req = base_req(
        "gpt-5.6",
        Some(Thinking {
            thinking_type: "enabled".to_string(),
            budget_tokens: 20000,
        }),
    );
    let result = convert_request(&req).unwrap();
    let fields = result
        .additional_model_request_fields
        .expect("GPT 系应构建 reasoning 结构");
    assert!(
        fields.get("thinking").is_none(),
        "GPT 系不得注入 thinking 字段"
    );
    assert!(
        fields.get("reasoning").is_some(),
        "GPT 系维持 reasoning.effort 现状"
    );
}

#[test]
fn test_additional_fields_skipped_for_unsupported_third_party_models() {
    // 回归测试（2026-10-03 实测）：qwen3-coder-next / glm-5 / deepseek-3.2 /
    // minimax-m2.5 携带 additionalModelRequestFields 被 Kiro 后端以 400
    // "additionalModelRequestFields is not supported for this model" 拒绝，
    // 需整体跳过；minimax-m2.1 实测可正常携带（200 OK），不得误伤。
    use super::super::thinking::additional_fields_skipped;
    use crate::anthropic::converter::map_model;

    // 混入客户端真实请求名（别名），验证「别名 → map_model 归一 → 谓词」完整链路
    for model in [
        "qwen3-coder-next",
        "qwen3-coder", // 别名 → qwen3-coder-next
        "glm-5",
        "glm-4.6", // 别名 → glm-5
        "deepseek-3.2",
        "deepseek-v3.2", // 别名 → deepseek-3.2
        "minimax-m2.5",
    ] {
        let mapped = map_model(model).unwrap_or_else(|| model.to_string());
        assert!(
            additional_fields_skipped(&mapped),
            "{model} (映射为 {mapped}) 应跳过 additionalModelRequestFields"
        );
        let req = MessagesRequest {
            model: model.to_string(),
            max_tokens: 32000,
            messages: vec![crate::anthropic::types::Message {
                role: "user".to_string(),
                content: serde_json::json!("Hello"),
            }],
            stream: false,
            system: None,
            tools: None,
            tool_choice: None,
            thinking: Some(crate::anthropic::types::Thinking {
                thinking_type: "adaptive".to_string(),
                budget_tokens: 20000,
            }),
            output_config: None,
            metadata: None,
        };
        let result = convert_request(&req).unwrap();
        assert!(
            result.additional_model_request_fields.is_none(),
            "{model} 不得携带 additionalModelRequestFields"
        );
    }

    assert!(
        !additional_fields_skipped("minimax-m2.1"),
        "minimax-m2.1 实测支持该字段，不应跳过"
    );
}
