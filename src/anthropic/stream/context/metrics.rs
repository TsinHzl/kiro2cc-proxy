// Copyright (c) 2026 Harllan He. Licensed under MIT.
// 输出 token 口径与空响应判定（自 context.rs 拆出，纯代码搬移）

//! 输出 token 口径统计与空响应判定（自 context.rs 拆出，纯代码搬移）。
use super::StreamContext;
use crate::anthropic::stream::calib::NEAR_EMPTY_OUTPUT_THRESHOLD;
use crate::anthropic::stream::calib::empty_response_oversized_threshold;

impl StreamContext {
    /// 计费口径的输出 tokens（含 thinking）——用于入库、`effective_rate` 与近似空响应判定
    ///
    /// BPE 真值（tiktoken cl100k_base，与输入估算、thinking_token_count 同编码器）：
    /// 流式过程中缓冲全文，此处收尾一次性编码。不能改为逐 chunk 编码累加 —— 跨
    /// chunk 的 BPE 合并（如两次 tool input 拼接点的 `}{"`）无法拆开分段累计；
    /// 前身字符估算实现同样靠"累加字符桶、收尾一次取整"消除逐次取整噪音
    /// （旧口径每 chunk 各桶向上取整一次，实测累积 +7.96% 高估）。
    /// 全文为空返回 0 —— `is_empty_response` 依赖"零输出"这一状态。
    pub fn output_tokens(&self) -> i32 {
        crate::token::count_tokens(&self.output_full_text) as i32
    }

    /// 对客户端上报的输出 tokens：只统计实际发出的 text_delta 与 tool_use 参数，
    /// 不含 thinking。BPE 真值口径，与 `output_tokens` 同收尾一次编码。
    ///
    /// 早前对外上报 `output_tokens.min(380)`，注释理由是"检测工具对 output_tokens
    /// 总和 > 800 扣 15 分，thinking 内容不应计入对外报告"。但固定上限把正常长回复
    /// 一并砍掉（实测真实 10145 上报 380 —— 客户端 /cost 低估 26 倍，auto-compact
    /// 判定少算最近一轮输出）。改为按来源精确排除 thinking：膨胀源头本就是 thinking
    /// （`process_assistant_response` 在分离标签前就计入计费口径），排除后无需上限。
    pub fn visible_output_tokens(&self) -> i32 {
        crate::token::count_tokens(&self.visible_full_text) as i32
    }

    /// 深度思考文案 tokens：客户端实际收到的 thinking_delta 内容（不含 signature），
    /// BPE 真值口径（与非流式 count_thinking_tokens、客户端「💭 N tokens」
    /// 的 thinking_text.rs 同族），观测用途（落库 usage 记录），不影响计费与上报口径
    pub fn thinking_token_count(&self) -> i32 {
        crate::token::count_tokens(&self.thinking_full_text) as i32
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
        // 在 cl100k 单 token 最长 128 字节的前提下，超过 30 × 128 字节必然 ≥ 31 token，不可能是近似空响应
        const CL100K_MAX_TOKEN_BYTES: usize = 128;
        if self.output_full_text.len()
            > NEAR_EMPTY_OUTPUT_THRESHOLD as usize * CL100K_MAX_TOKEN_BYTES
        {
            return false;
        }
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
