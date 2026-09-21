// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! thinking 前缀生成、反伪标签引导语与模型谓词

use crate::anthropic::types::MessagesRequest;

pub(crate) fn is_gpt_model(model_id: &str) -> bool {
    model_id.to_ascii_lowercase().starts_with("gpt-")
}

/// `additionalModelRequestFields` 结构化字段的整体跳过谓词（单一来源）。
///
/// "4.5" 代际（sonnet/opus/haiku）被 Kiro 后端拒绝该字段，需整体跳过。
/// GPT 系已改为发送 `reasoning.effort` 独立结构（见 `fields.rs`），
/// 不再属于"整体跳过"范畴，故本谓词仅保留 "4.5" 代际跳过条件。
/// converter 侧 `build_additional_model_request_fields` 与 provider 侧
/// thinking adaptive 注入共用本谓词，避免排除条件双份硬编码漂移。
pub(crate) fn additional_fields_skipped(model_id: &str) -> bool {
    model_id.ends_with("4.5")
}

/// 判断是否为 `gpt-5.6-luna`。
///
/// **注意范围**：与 `is_gpt_model`（匹配全部 gpt-* 系列）不同，这里专指 luna 一个
/// 型号。GPT 系现已改为发送 `reasoning.effort` 结构（见 `build_additional_model_request_fields`），
/// 不再整体跳过 `additionalModelRequestFields`。但"上游恒返回 `thinking=0`"
/// 这一具体观察目前只在 luna 上验证过
/// （见 `openai::model_map` 已知限制注释与 README）。terra/sol 是否同样如此并无
/// 实测证据，因此涉及"是否应完全放弃 thinking 处理"的判断只能收窄到 luna，
/// 不能套用到全部 GPT 系模型，否则会误伤 terra/sol 本该具备的推理能力。
pub(crate) fn is_luna_model(model_id: &str) -> bool {
    model_id.to_ascii_lowercase().starts_with("gpt-5.6-luna")
}

/// `gpt-5.6-luna` 上游恒返回 `thinking=0`（已知限制，见 README/openai::model_map），
/// 且不支持 Claude/Kiro 的 thinking 协议。注意：GPT 系现已改为发送
/// `reasoning.effort` 结构，不再整体跳过 `additionalModelRequestFields`；
/// `generate_thinking_prefix` 已收窄至仅 luna 跳过
/// `<thinking_mode>` 文本标签（sol/terra 会注入，见该函数文档）。
///
/// 当客户端请求里仍然携带 `thinking` 配置（如 Claude Code 默认开启 extended
/// thinking）时，luna 在缺少思考协议约束、且自身不产出推理内容的情况下，有概率
/// 自行选择用类似 `<analysis>...</analysis><summary>...</summary>` 的自造伪标签来
/// 组织输出，而不是符合 Anthropic 协议的纯文本。这类标签不会被
/// `find_real_thinking_start_tag` 等仅识别 `<thinking>` 的逻辑捕获，会作为可见文本
/// 原样转发给客户端。
///
/// 因此仅对 luna 在系统提示里显式告知模型不要使用这类自造包裹标签，直接输出最终
/// 答案。terra/sol 未有类似实测问题，不注入此提示，避免不必要地扰动其系统提示与
/// prompt cache key。
const GPT_ANTI_PSEUDO_TAG_HINT: &str = "\
Do not wrap your response in custom pseudo-XML tags such as <analysis>, <summary>, \
<thinking>, <plan>, or similar self-invented section markers. Respond with plain, \
direct prose or standard Markdown only. If you want to reason before answering, do \
the reasoning silently and only output the final answer.";

/// 判断是否需要为 `gpt-5.6-luna` 注入反伪标签引导语。
///
/// 仅在客户端显式请求了 thinking 时注入，因为这是诱发 luna 使用自造分析/总结标签的
/// 主要场景；避免对所有请求都追加提示词，以免不必要地扰动 prompt cache 前缀与
/// token 消耗。仅针对 luna，不影响 terra/sol（见 `is_luna_model` 文档）。
pub(super) fn gpt_anti_pseudo_tag_hint(
    req: &MessagesRequest,
    model_id: &str,
) -> Option<&'static str> {
    if !is_luna_model(model_id) {
        return None;
    }
    // 与 handlers::resolve_thinking_enabled 保持同一判断口径：必须用 is_enabled()
    // 而非 is_some()。客户端可能显式传 `{"type": "disabled"}` 来关闭 thinking（
    // Anthropic 协议允许，见 build_additional_model_request_fields 对该取值的处理），
    // 此时 thinking.is_some() 为真但并未真正启用，若仍注入提示语，会与
    // resolve_thinking_enabled（已判定不启用、走正常文本路径）的状态不一致，
    // 并无谓污染 prompt cache key。
    if req
        .thinking
        .as_ref()
        .map(|t| t.is_enabled())
        .unwrap_or(false)
    {
        Some(GPT_ANTI_PSEUDO_TAG_HINT)
    } else {
        None
    }
}

/// 生成 thinking 标签前缀。
///
/// 仅 `gpt-5.6-luna` 跳过：luna 不支持 Claude/Kiro thinking 控制协议，且上游恒返回
/// `thinking=0`（实测，见 `is_luna_model` 文档）。sol/terra 未见同类实测证据，此前对
/// 全部 GPT 系一刀切跳过导致其 thinking 恒不生效，现收窄到 luna，sol/terra 与 Claude
/// 系一样按 `<thinking_mode>` 文本协议注入。
pub(super) fn generate_thinking_prefix(req: &MessagesRequest, model_id: &str) -> Option<String> {
    if is_luna_model(model_id) {
        return None;
    }

    if let Some(t) = &req.thinking {
        if t.thinking_type == "enabled" {
            return Some(format!(
                "<thinking_mode>enabled</thinking_mode><max_thinking_length>{}</max_thinking_length>",
                t.budget_tokens
            ));
        } else if t.thinking_type == "adaptive" {
            // adaptive 模式不注入任何 thinking 标签到 system 消息：
            // Kiro CLI 不注入这些标签，让 Kiro 后端用自身默认行为控制 thinking。
            // 此前注入 <thinking_mode>adaptive</thinking_mode> 会触发 Kiro 后端
            // 额外的 thinking 调度路径，显著增加 TTFB（实测根因）。
            return None;
        }
    }
    None
}

/// 检查内容是否已包含thinking标签
pub(super) fn has_thinking_tags(content: &str) -> bool {
    content.contains("<thinking_mode>") || content.contains("<max_thinking_length>")
}
