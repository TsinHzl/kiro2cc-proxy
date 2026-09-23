// Copyright (c) 2026 Harllan He. Licensed under MIT.
// 输出 token 口径与空响应判定（自 context.rs 拆出，纯代码搬移）

use super::StreamContext;
use crate::anthropic::stream::calib::NEAR_EMPTY_OUTPUT_THRESHOLD;
use crate::anthropic::stream::calib::empty_response_oversized_threshold;
use crate::anthropic::stream::helpers::tokens_from_chars;
use crate::anthropic::stream::state::SseEvent;

impl StreamContext {
    /// 计费口径的输出 tokens（含 thinking）——用于入库、`effective_rate` 与近似空响应判定
    ///
    /// 由字符计数器一次性派生，而非逐 chunk 累加 token：`estimate_tokens` 对
    /// 中文/非中文两个桶各做一次向上取整，每 chunk 调用一次会累积出系统性高估
    /// （实测同一响应计费口径 8703、上报口径 8061，+7.96%，差值全部是取整噪音）。
    pub fn output_tokens(&self) -> i32 {
        tokens_from_chars(self.output_chars_cn, self.output_chars_other)
    }

    /// 对客户端上报的输出 tokens：只统计实际发出的 text_delta 与 tool_use 参数，
    /// 不含 thinking。
    ///
    /// 早前对外上报 `output_tokens.min(380)`，注释理由是"检测工具对 output_tokens
    /// 总和 > 800 扣 15 分，thinking 内容不应计入对外报告"。但固定上限把正常长回复
    /// 一并砍掉（实测真实 10145 上报 380 —— 客户端 /cost 低估 26 倍，auto-compact
    /// 判定少算最近一轮输出）。改为按来源精确排除 thinking：膨胀源头本就是 thinking
    /// （`process_assistant_response` 在分离标签前就计入计费口径），排除后无需上限。
    pub fn visible_output_tokens(&self) -> i32 {
        tokens_from_chars(self.visible_chars_cn, self.visible_chars_other)
    }

    /// 检测上游是否返回了无效的空/近似空响应。
    ///
    /// 两种判定路径：
    /// 1. **完全空**：output_tokens == 0、无工具调用、thinking 缓冲为空。
    /// 2. **近似空 + 上下文过大**：output_tokens 极少（< 30）且无工具调用，
    ///    同时 input_tokens 超过"上下文过大"阈值。此类响应是模型在上下文压力下
    ///    返回的无意义短文本（如几个空白 token），客户端拿到后会以为 end_turn
    ///    正常结束并尝试继续对话，导致 agentic 循环反复卡住。
    ///
    /// 用于给客户端返回明确的错误信号以触发重试或提示压缩上下文，
    /// 而非静默返回空的 end_turn（客户端会表现为卡住/工具不执行）。
    pub fn is_empty_response(&self) -> bool {
        let no_tool_use = !self.state_manager.has_tool_use();
        let thinking_empty = self.thinking_buffer.trim().is_empty();
        let output_tokens = self.output_tokens();

        // 路径 1：完全空
        if output_tokens == 0 && no_tool_use && thinking_empty {
            return true;
        }

        // 路径 2：近似空 + 上下文过大
        // 当 output 极少且无工具调用时，若上下文已超过阈值，判定为退化的空响应
        if output_tokens > 0
            && output_tokens < NEAR_EMPTY_OUTPUT_THRESHOLD
            && no_tool_use
            && thinking_empty
        {
            let est = self.context_input_tokens.unwrap_or(self.input_tokens);
            if est >= empty_response_oversized_threshold(&self.model) {
                tracing::warn!(
                    "[near-empty] 检测到近似空响应: output_tokens={} input_tokens={} \
                     threshold={} — 视为上下文过大导致的退化响应",
                    output_tokens,
                    est,
                    empty_response_oversized_threshold(&self.model),
                );
                return true;
            }
        }

        false
    }

    /// 空响应是否由「上下文过大」导致。
    /// 经验阈值：实测 input>10万 token 时上游稳定返回空流（疑似 Kiro 上下文软上限），
    /// 而正常响应的 input 均 <8万。取 9万 作为判定阈值。
    /// 大输入空响应 → 不应重试（重试还是同样的大请求），应提示客户端压缩上下文；
    /// 小输入空响应 → 视为偶发，可重试。
    pub fn empty_response_is_oversized_context(&self) -> bool {
        let est = self.context_input_tokens.unwrap_or(self.input_tokens);
        est >= empty_response_oversized_threshold(&self.model)
    }
}
