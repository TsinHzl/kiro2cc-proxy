// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! additionalModelRequestFields 构建（thinking/output_config/max_tokens/reasoning）

use crate::anthropic::types::MessagesRequest;

use super::thinking::{additional_fields_skipped, is_gpt_model};

/// 根据模型返回 Kiro 允许的 max_tokens 上限
/// claude-opus-5 / claude-opus-4.7 / claude-opus-4.8 Max Output = 128K（1M 窗口代际）
/// claude-sonnet-5 Max Output = 64K，与 sonnet-4.x 同档，走默认分支即可
pub(super) fn model_max_output_tokens(model: &str) -> i32 {
    let m = model.to_lowercase();
    if m.contains("opus-4-7")
        || m.contains("opus-4.7")
        || m.contains("opus-4-8")
        || m.contains("opus-4.8")
        || m.contains("opus-5")
        || m.contains("opus.5")
        || m.contains("opus 5")
    {
        128000
    } else {
        64000
    }
}

/// 构建 additionalModelRequestFields（thinking、output_config、max_tokens、reasoning）
///
/// 实测：claude-sonnet-4.5 / claude-opus-4.5 / claude-haiku-4.5 这三个 "4.5" 代际模型
/// Kiro 后端均拒绝该字段（先后遇到 400 REQUEST_BODY_INVALID：
/// max_tokens、output_config 均不在 schema 定义内且不允许额外属性），需跳过整个字段构建。
///
/// GPT 系（gpt-5.6-luna 等）使用独立的 `reasoning.effort` 结构（抓包实测），
/// 与 Claude 系的 `output_config.effort` 完全分离。
pub(super) fn build_additional_model_request_fields(
    req: &MessagesRequest,
    model_id: &str,
) -> Option<serde_json::Value> {
    // "4.5" 代际整体跳过，见上方实测说明
    if additional_fields_skipped(model_id) {
        return None;
    }

    // GPT 系走 reasoning.effort 路径（抓包实测：luna 使用此结构）
    if is_gpt_model(model_id) {
        let effort = req
            .output_config
            .as_ref()
            .map(|c| c.effort.as_str())
            .filter(|e| !e.is_empty())
            .unwrap_or("high");
        return Some(serde_json::json!({ "reasoning": { "effort": effort } }));
    }

    let mut fields = serde_json::Map::new();

    // thinking 字段不发送：Kiro CLI 经 ListAvailableModels schema 解析后，
    // 对 claude-sonnet-4.6 等模型只发 output_config.effort，不发 thinking 字段。
    // 发 thinking 字段会让 Kiro 后端走额外的 thinking 调度路径，显著增加 TTFB。
    // Kiro 后端的 thinking 行为由其自身默认值控制，无需代理显式指定。

    // effort 透传 + 默认注入：客户端显式携带 output_config 时按原值转发；
    // 未携带时默认注入 effort="low"。
    // 注意：默认注入值历经多次调整——issue #40 曾记录默认注入 effort="high"
    // 导致 sonnet-5 / opus-5 反代链路 TTFB 慢于直连（v3.2.1 改为仅透传修复），
    // 2026-09-15 为对齐 Claude Code 默认推理力度又回退恢复注入（值 high）。
    // 2026-09-24 实测 high 兜底仍使 sonnet-5 显著慢于 opus-5（issue #40 评论
    // 追踪），故兜底值下调为 "low"，在保留"必带 effort 字段"结构的前提下
    // 降低 Kiro 后端推理调度成本。客户端显式携带 output_config 时不受影响。
    let effort = req
        .output_config
        .as_ref()
        .map(|c| c.effort.as_str())
        .filter(|e| !e.is_empty())
        .unwrap_or("low");
    fields.insert(
        "output_config".into(),
        serde_json::json!({ "effort": effort }),
    );

    if req.max_tokens > 0 {
        let cap = model_max_output_tokens(&req.model);
        let mut capped = req.max_tokens.min(cap);
        // Kiro 侧 schema 对 max_tokens 强制 minimum = 1024，对所有 Claude 代际生效
        // （实测：claude-sonnet-4-6 在 max_tokens=200 时同样报 400
        // "Invalid additionalModelRequestFields: must have a minimum value of 1024.0"，
        // 并非只有 opus-4.7/4.8/5 的 128000 上限档才有此限制）。
        capped = capped.max(1024);
        fields.insert("max_tokens".into(), serde_json::json!(capped));
    }

    if fields.is_empty() {
        None
    } else {
        Some(serde_json::Value::Object(fields))
    }
}
