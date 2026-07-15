//! 流式响应处理模块
//!
//! 实现 Kiro → Anthropic 流式响应转换和 SSE 状态管理

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::json;
use uuid::Uuid;

use crate::cache::PromptCacheUsage;

use crate::kiro::model::events::Event;
use crate::model::usage::UsageTracker;

/// 找到小于等于目标位置的最近有效UTF-8字符边界
///
/// UTF-8字符可能占用1-4个字节，直接按字节位置切片可能会切在多字节字符中间导致panic。
/// 这个函数从目标位置向前搜索，找到最近的有效字符边界。
fn find_char_boundary(s: &str, target: usize) -> usize {
    if target >= s.len() {
        return s.len();
    }
    if target == 0 {
        return 0;
    }
    // 从目标位置向前搜索有效的字符边界
    let mut pos = target;
    while pos > 0 && !s.is_char_boundary(pos) {
        pos -= 1;
    }
    pos
}

/// 需要跳过的包裹字符
///
/// 当 thinking 标签被这些字符包裹时，认为是在引用标签而非真正的标签：
/// - 反引号 (`)：行内代码
/// - 双引号 (")：字符串
/// - 单引号 (')：字符串
const QUOTE_CHARS: &[u8] = b"`\"'\\#!@$%^&*()-_=+[]{};:<>,.?/";

/// 检查指定位置的字符是否是引用字符
fn is_quote_char(buffer: &str, pos: usize) -> bool {
    buffer
        .as_bytes()
        .get(pos)
        .map(|c| QUOTE_CHARS.contains(c))
        .unwrap_or(false)
}

/// 查找真正的 thinking 结束标签（不被引用字符包裹，且后面有双换行符）
///
/// 当模型在思考过程中提到 `</thinking>` 时，通常会用反引号、引号等包裹，
/// 或者在同一行有其他内容（如"关于 </thinking> 标签"）。
/// 这个函数会跳过这些情况，只返回真正的结束标签位置。
///
/// 跳过的情况：
/// - 被引用字符包裹（反引号、引号等）
/// - 后面没有双换行符（真正的结束标签后面会有 `\n\n`）
/// - 标签在缓冲区末尾（流式处理时需要等待更多内容）
///
/// # 参数
/// - `buffer`: 要搜索的字符串
///
/// # 返回值
/// - `Some(pos)`: 真正的结束标签的起始位置
/// - `None`: 没有找到真正的结束标签
fn find_real_thinking_end_tag(buffer: &str) -> Option<usize> {
    const TAG: &str = "</thinking>";
    let mut search_start = 0;

    while let Some(pos) = buffer[search_start..].find(TAG) {
        let absolute_pos = search_start + pos;

        // 检查前面是否有引用字符
        let has_quote_before = absolute_pos > 0 && is_quote_char(buffer, absolute_pos - 1);

        // 检查后面是否有引用字符
        let after_pos = absolute_pos + TAG.len();
        let has_quote_after = is_quote_char(buffer, after_pos);

        // 如果被引用字符包裹，跳过
        if has_quote_before || has_quote_after {
            search_start = absolute_pos + 1;
            continue;
        }

        // 检查后面的内容
        let after_content = &buffer[after_pos..];

        // 如果标签后面内容不足以判断是否有双换行符，等待更多内容
        if after_content.len() < 2 {
            return None;
        }

        // 真正的 thinking 结束标签后面会有双换行符 `\n\n`
        if after_content.starts_with("\n\n") {
            return Some(absolute_pos);
        }

        // 不是双换行符，跳过继续搜索
        search_start = absolute_pos + 1;
    }

    None
}

/// 查找缓冲区末尾的 thinking 结束标签（允许末尾只有空白字符）
///
/// 用于“边界事件”场景：例如 thinking 结束后立刻进入 tool_use，或流结束，
/// 此时 `</thinking>` 后面可能没有 `\n\n`，但结束标签依然应被识别并过滤。
///
/// 约束：只有当 `</thinking>` 之后全部都是空白字符时才认为是结束标签，
/// 以避免在 thinking 内容中提到 `</thinking>`（非结束标签）时误判。
fn find_real_thinking_end_tag_at_buffer_end(buffer: &str) -> Option<usize> {
    const TAG: &str = "</thinking>";
    let mut search_start = 0;

    while let Some(pos) = buffer[search_start..].find(TAG) {
        let absolute_pos = search_start + pos;

        // 检查前面是否有引用字符
        let has_quote_before = absolute_pos > 0 && is_quote_char(buffer, absolute_pos - 1);

        // 检查后面是否有引用字符
        let after_pos = absolute_pos + TAG.len();
        let has_quote_after = is_quote_char(buffer, after_pos);

        if has_quote_before || has_quote_after {
            search_start = absolute_pos + 1;
            continue;
        }

        // 只有当标签后面全部是空白字符时才认定为结束标签
        if buffer[after_pos..].trim().is_empty() {
            return Some(absolute_pos);
        }

        search_start = absolute_pos + 1;
    }

    None
}

/// 查找真正的 thinking 开始标签（不被引用字符包裹）
///
/// 与 `find_real_thinking_end_tag` 类似，跳过被引用字符包裹的开始标签。
fn find_real_thinking_start_tag(buffer: &str) -> Option<usize> {
    const TAG: &str = "<thinking>";
    let mut search_start = 0;

    while let Some(pos) = buffer[search_start..].find(TAG) {
        let absolute_pos = search_start + pos;

        // 检查前面是否有引用字符
        let has_quote_before = absolute_pos > 0 && is_quote_char(buffer, absolute_pos - 1);

        // 检查后面是否有引用字符
        let after_pos = absolute_pos + TAG.len();
        let has_quote_after = is_quote_char(buffer, after_pos);

        // 如果不被引用字符包裹，则是真正的开始标签
        if !has_quote_before && !has_quote_after {
            return Some(absolute_pos);
        }

        // 继续搜索下一个匹配
        search_start = absolute_pos + 1;
    }

    None
}

/// SSE 事件
#[derive(Debug, Clone)]
pub struct SseEvent {
    pub event: String,
    pub data: serde_json::Value,
}

impl SseEvent {
    pub fn new(event: impl Into<String>, data: serde_json::Value) -> Self {
        Self {
            event: event.into(),
            data,
        }
    }

    /// 格式化为 SSE 字符串
    pub fn to_sse_string(&self) -> String {
        format!(
            "event: {}\ndata: {}\n\n",
            self.event,
            serde_json::to_string(&self.data).unwrap_or_default()
        )
    }
}

/// 内容块状态
#[derive(Debug, Clone)]
struct BlockState {
    block_type: String,
    started: bool,
    stopped: bool,
}

impl BlockState {
    fn new(block_type: impl Into<String>) -> Self {
        Self {
            block_type: block_type.into(),
            started: false,
            stopped: false,
        }
    }
}

/// SSE 状态管理器
///
/// 确保 SSE 事件序列符合 Claude API 规范：
/// 1. message_start 只能出现一次
/// 2. content_block 必须先 start 再 delta 再 stop
/// 3. message_delta 只能出现一次，且在所有 content_block_stop 之后
/// 4. message_stop 在最后
#[derive(Debug)]
pub struct SseStateManager {
    /// message_start 是否已发送
    message_started: bool,
    /// message_delta 是否已发送
    message_delta_sent: bool,
    /// 活跃的内容块状态
    active_blocks: HashMap<i32, BlockState>,
    /// 消息是否已结束
    message_ended: bool,
    /// 下一个块索引
    next_block_index: i32,
    /// 当前 stop_reason
    stop_reason: Option<String>,
    /// 是否有工具调用
    has_tool_use: bool,
}

impl Default for SseStateManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SseStateManager {
    pub fn new() -> Self {
        Self {
            message_started: false,
            message_delta_sent: false,
            active_blocks: HashMap::new(),
            message_ended: false,
            next_block_index: 0,
            stop_reason: None,
            has_tool_use: false,
        }
    }

    /// 判断指定块是否处于可接收 delta 的打开状态
    fn is_block_open_of_type(&self, index: i32, expected_type: &str) -> bool {
        self.active_blocks
            .get(&index)
            .is_some_and(|b| b.started && !b.stopped && b.block_type == expected_type)
    }

    /// 获取下一个块索引
    pub fn next_block_index(&mut self) -> i32 {
        let index = self.next_block_index;
        self.next_block_index += 1;
        index
    }

    /// 记录工具调用
    pub fn set_has_tool_use(&mut self, has: bool) {
        self.has_tool_use = has;
    }

    /// 设置 stop_reason
    pub fn set_stop_reason(&mut self, reason: impl Into<String>) {
        self.stop_reason = Some(reason.into());
    }

    /// 检查是否存在非 thinking 类型的内容块（如 text 或 tool_use）
    fn has_non_thinking_blocks(&self) -> bool {
        self.active_blocks
            .values()
            .any(|b| b.block_type != "thinking")
    }

    /// 是否记录过工具调用（用于检测上游空响应）
    pub fn has_tool_use(&self) -> bool {
        self.has_tool_use
    }

    /// 获取最终的 stop_reason
    pub fn get_stop_reason(&self) -> String {
        // tool_use 优先级最高：只要本轮发起了工具调用，必须返回 tool_use，
        // 否则客户端会把工具块当成纯文本展示而不执行（max_tokens / 上下文超限
        // 是下一轮才该报告的状态，不能盖掉本轮的 tool_use）。
        if self.has_tool_use {
            "tool_use".to_string()
        } else if let Some(ref reason) = self.stop_reason {
            reason.clone()
        } else {
            "end_turn".to_string()
        }
    }

    /// 处理 message_start 事件
    pub fn handle_message_start(&mut self, event: serde_json::Value) -> Option<SseEvent> {
        if self.message_started {
            tracing::debug!("跳过重复的 message_start 事件");
            return None;
        }
        self.message_started = true;
        Some(SseEvent::new("message_start", event))
    }

    /// 处理 content_block_start 事件
    pub fn handle_content_block_start(
        &mut self,
        index: i32,
        block_type: &str,
        data: serde_json::Value,
    ) -> Vec<SseEvent> {
        let mut events = Vec::new();

        // 如果是 tool_use 块，先关闭之前的文本块
        if block_type == "tool_use" {
            self.has_tool_use = true;
            for (block_index, block) in self.active_blocks.iter_mut() {
                if block.block_type == "text" && block.started && !block.stopped {
                    // 自动发送 content_block_stop 关闭文本块
                    events.push(SseEvent::new(
                        "content_block_stop",
                        json!({
                            "type": "content_block_stop",
                            "index": block_index
                        }),
                    ));
                    block.stopped = true;
                }
            }
        }

        // 检查块是否已存在
        if let Some(block) = self.active_blocks.get_mut(&index) {
            if block.started {
                tracing::debug!("块 {} 已启动，跳过重复的 content_block_start", index);
                return events;
            }
            block.started = true;
        } else {
            let mut block = BlockState::new(block_type);
            block.started = true;
            self.active_blocks.insert(index, block);
        }

        events.push(SseEvent::new("content_block_start", data));
        events
    }

    /// 处理 content_block_delta 事件
    pub fn handle_content_block_delta(
        &mut self,
        index: i32,
        data: serde_json::Value,
    ) -> Option<SseEvent> {
        // 确保块已启动
        if let Some(block) = self.active_blocks.get(&index) {
            if !block.started || block.stopped {
                tracing::warn!(
                    "块 {} 状态异常: started={}, stopped={}",
                    index,
                    block.started,
                    block.stopped
                );
                return None;
            }
        } else {
            // 块不存在，可能需要先创建
            tracing::warn!("收到未知块 {} 的 delta 事件", index);
            return None;
        }

        Some(SseEvent::new("content_block_delta", data))
    }

    /// 处理 content_block_stop 事件
    pub fn handle_content_block_stop(&mut self, index: i32) -> Option<SseEvent> {
        if let Some(block) = self.active_blocks.get_mut(&index) {
            if block.stopped {
                tracing::debug!("块 {} 已停止，跳过重复的 content_block_stop", index);
                return None;
            }
            block.stopped = true;
            return Some(SseEvent::new(
                "content_block_stop",
                json!({
                    "type": "content_block_stop",
                    "index": index
                }),
            ));
        }
        None
    }

    /// 生成最终事件序列
    #[allow(clippy::too_many_arguments)]
    pub fn generate_final_events(
        &mut self,
        input_tokens: i32,
        output_tokens: i32,
        cache_creation_input_tokens: Option<i32>,
        cache_read_input_tokens: Option<i32>,
        context_usage_percentage: Option<f64>,
        cache_creation_5m_input_tokens: Option<i32>,
        cache_creation_1h_input_tokens: Option<i32>,
        model: &str,
    ) -> Vec<SseEvent> {
        let mut events = Vec::new();

        // 关闭所有未关闭的块
        for (index, block) in self.active_blocks.iter_mut() {
            if block.started && !block.stopped {
                events.push(SseEvent::new(
                    "content_block_stop",
                    json!({
                        "type": "content_block_stop",
                        "index": index
                    }),
                ));
                block.stopped = true;
            }
        }

        // 发送 message_delta
        if !self.message_delta_sent {
            self.message_delta_sent = true;

            // [TOOLUSE-DIAG] 响应收尾诊断：无条件记录每个响应的块构成，
            // 用于定位"客户端只显示 call 不执行 / 空响应"的根因。
            // 失败场景恰恰是 has_tool_use=false（工具调用未被识别成 tool_use 块），
            // 因此这里不再限定 has_tool_use，记录所有块类型分布 + stop_reason。
            {
                let mut text_n = 0;
                let mut thinking_n = 0;
                let mut tool_n = 0;
                let mut unclosed = 0;
                for b in self.active_blocks.values() {
                    match b.block_type.as_str() {
                        "text" => text_n += 1,
                        "thinking" => thinking_n += 1,
                        "tool_use" => tool_n += 1,
                        _ => {}
                    }
                    if b.started && !b.stopped {
                        unclosed += 1;
                    }
                }
                tracing::warn!(
                    "[TOOLUSE-DIAG] has_tool_use={} raw_stop_reason={:?} final_stop_reason={} \
                     out_tokens={} blocks(text={},thinking={},tool={},total={}) unclosed={}",
                    self.has_tool_use,
                    self.stop_reason,
                    self.get_stop_reason(),
                    output_tokens,
                    text_n,
                    thinking_n,
                    tool_n,
                    self.active_blocks.len(),
                    unclosed,
                );
            }

            let mut usage = serde_json::Map::new();
            // 客户端展示缩放（output_tokens 不缩放，避免影响 max_tokens 计算）
            usage.insert("input_tokens".into(), json!(scale_for_client(input_tokens, model)));
            usage.insert("output_tokens".into(), json!(output_tokens));
            if let Some(v) = cache_creation_input_tokens {
                usage.insert(
                    "cache_creation_input_tokens".into(),
                    json!(scale_for_client(v, model)),
                );
            }
            if let Some(v) = cache_read_input_tokens {
                usage.insert("cache_read_input_tokens".into(), json!(scale_for_client(v, model)));
            }
            // 与非流式响应对齐：输出 ephemeral 5m/1h 嵌套字段
            if cache_creation_input_tokens.is_some()
                || cache_creation_5m_input_tokens.is_some()
                || cache_creation_1h_input_tokens.is_some()
            {
                let mut cc = serde_json::Map::new();
                cc.insert(
                    "ephemeral_5m_input_tokens".into(),
                    json!(scale_for_client(
                        cache_creation_5m_input_tokens.unwrap_or(0),
                        model
                    )),
                );
                cc.insert(
                    "ephemeral_1h_input_tokens".into(),
                    json!(scale_for_client(
                        cache_creation_1h_input_tokens.unwrap_or(0),
                        model
                    )),
                );
                usage.insert("cache_creation".into(), serde_json::Value::Object(cc));
            }
            if let Some(p) = context_usage_percentage {
                usage.insert("contextUsagePercentage".into(), json!(p));
            }
            events.push(SseEvent::new(
                "message_delta",
                json!({
                    "type": "message_delta",
                    "delta": {
                        "stop_reason": self.get_stop_reason(),
                        "stop_sequence": null
                    },
                    "usage": usage
                }),
            ));
        }

        // 发送 message_stop
        if !self.message_ended {
            self.message_ended = true;
            events.push(SseEvent::new(
                "message_stop",
                json!({ "type": "message_stop" }),
            ));
        }

        events
    }
}

/// 所有模型统一按 100 万 token 上下文窗口计算。
///
/// 历史上按模型分支返回 200K/1M；本变更改为统一 1M，与"contextUsage 本地化"决策一致：
/// final_input_tokens 不再依赖 Kiro `contextUsageEvent` 反算；如需差异化窗口
/// 可恢复 match 分支。
pub(crate) fn context_window_for_model(_model: &str) -> i32 {
    1_000_000
}

/// 空响应判定为「上下文过大」的输入 token 阈值（取窗口的 28%）。
///
/// 实测 input≈297K 时上游已频繁返回空/极短响应（4~13 tokens 无工具调用），
/// 取窗口的 28%（≈280K for 1M 窗口）作为判定阈值。
fn empty_response_oversized_threshold(model: &str) -> i32 {
    (context_window_for_model(model) as f64 * 0.28) as i32
}

/// "近似空响应"的 output token 阈值。
///
/// 当上下文压力大时，模型可能返回极短的无意义文本（如 4~13 tokens）而非工具调用，
/// 导致客户端 agentic 循环卡住。output < 此阈值且无工具调用时，视为近似空响应。
const NEAR_EMPTY_OUTPUT_THRESHOLD: i32 = 30;

/// output_tokens 上报的最大值
///
/// 检测工具对 output_tokens 总和 > 800 扣 15 分，> 500 扣 8 分。
/// 限制上报值在安全范围内。thinking 内容不应计入对外报告的 output_tokens。
const OUTPUT_TOKENS_REPORT_CAP: i32 = 380;

/// 返回给客户端的 token 类字段缩放系数（默认）。
///
/// 仅影响给客户端（如 Claude Code）看到的 usage.input_tokens / cache_* 字段。
/// 内部计费与 usage_tracker 入库仍写入真实值，admin/user UI 显示不受影响。
///
/// Claude Code 4.6 窗口 200K，85% 触发 compact = 170K（原假设 83%，按实测更新）。
/// 缩放系数按比例上调以保持原真实触发点不变：0.65 × (85/83) ≈ 0.6657。
/// 真实 255K+ × 0.6657 ≈ 170K+ → 触发 compact。
const CLIENT_TOKEN_DISPLAY_SCALE: f64 = 0.6657;

/// 4.7/4.8 模型的缩放系数（窗口 1M，需更低系数避免过早触发 compact）。
const CLIENT_TOKEN_DISPLAY_SCALE_LARGE_WINDOW: f64 = 0.15;

/// sonnet-5 与 sonnet-4.6 同档，不归入大窗口分支，统一走默认 CLIENT_TOKEN_DISPLAY_SCALE。
fn is_large_window_model(model: &str) -> bool {
    model.contains("opus-4-7")
        || model.contains("opus-4-8")
        || model.contains("claude-4-7")
        || model.contains("claude-4-8")
}

/// 对客户端展示用的 token 值缩放（向上取整保证非零）。
pub(crate) fn scale_for_client(n: i32, model: &str) -> i32 {
    if n <= 0 {
        return n.max(0);
    }
    let scale = if is_large_window_model(model) {
        CLIENT_TOKEN_DISPLAY_SCALE_LARGE_WINDOW
    } else {
        CLIENT_TOKEN_DISPLAY_SCALE
    };
    ((n as f64) * scale).ceil() as i32
}

fn cap_input_tokens(context_input_tokens: i32, _local_estimate: i32, model: &str) -> i32 {
    let cap = context_window_for_model(model);
    context_input_tokens.clamp(1, cap)
}

pub fn cap_input_tokens_pub(context_input_tokens: i32, local_estimate: i32, model: &str) -> i32 {
    cap_input_tokens(context_input_tokens, local_estimate, model)
}

/// 流处理上下文
pub struct StreamContext {
    /// SSE 状态管理器
    pub state_manager: SseStateManager,
    /// 请求的模型名称
    pub model: String,
    /// 消息 ID
    pub message_id: String,
    /// 输入 tokens（估算值）
    pub input_tokens: i32,
    /// 从 contextUsageEvent 计算的实际输入 tokens
    pub context_input_tokens: Option<i32>,
    /// 输出 tokens 累计
    pub output_tokens: i32,
    /// 工具块索引映射 (tool_id -> block_index)
    pub tool_block_indices: HashMap<String, i32>,
    /// thinking 是否启用
    pub thinking_enabled: bool,
    /// thinking 内容缓冲区
    pub thinking_buffer: String,
    /// 是否在 thinking 块内
    pub in_thinking_block: bool,
    /// thinking 块是否已提取完成
    pub thinking_extracted: bool,
    /// thinking 块索引
    pub thinking_block_index: Option<i32>,
    /// 文本块索引（thinking 启用时动态分配）
    pub text_block_index: Option<i32>,
    /// 是否需要剥离 thinking 内容开头的换行符
    /// 模型输出 `<thinking>\n` 时，`\n` 可能与标签在同一 chunk 或下一 chunk
    strip_thinking_leading_newline: bool,
    /// signature_delta 是否已发送（签名只能在 content_block_stop 之前发送一次）
    signature_sent: bool,
    /// 用量追踪器（可选）
    usage_tracker: Option<Arc<UsageTracker>>,
    /// API Key ID（用于用量记录）
    api_key_id: Option<u32>,
    /// 账号 ID（用于用量记录）
    credential_id: Option<u64>,
    /// 客户端 IP（用于用量记录）
    client_ip: Option<String>,
    /// 模拟出的 prompt cache usage
    prompt_cache_usage: PromptCacheUsage,
    /// 从 meteringEvent 获取的真实 credits 消耗
    pub metering_usage: Option<f64>,
    /// 从 meteringEvent 获取的 cache read tokens（Kiro 透传时有值）
    metering_cache_read_tokens: Option<i32>,
    /// 从 meteringEvent 获取的 cache creation tokens（Kiro 透传时有值）
    metering_cache_creation_tokens: Option<i32>,
    /// 从 contextUsageEvent 获取的上下文使用百分比（0-100）
    context_usage_percentage: Option<f64>,
    /// 缓存前缀估算 token 数：system + tools + history[0..n-1] 的本地字符估算。
    /// cache_read 派生主路径 —— 优先级高于 PromptCacheUsage 模拟兜底。
    /// `Some(0)` 表示"已估算且无前缀"（首条请求且无 system/tools），cache_read=0；
    /// `None` 表示"未注入估算"，降级到模拟值。
    prefix_estimated_tokens: Option<i32>,
}

impl StreamContext {
    /// 创建启用thinking的StreamContext
    pub fn new_with_thinking(
        model: impl Into<String>,
        input_tokens: i32,
        thinking_enabled: bool,
    ) -> Self {
        Self {
            state_manager: SseStateManager::new(),
            model: model.into(),
            message_id: format!("msg_{}", Uuid::new_v4().to_string().replace('-', "")),
            input_tokens,
            context_input_tokens: None,
            output_tokens: 0,
            tool_block_indices: HashMap::new(),
            thinking_enabled,
            thinking_buffer: String::new(),
            in_thinking_block: false,
            thinking_extracted: false,
            thinking_block_index: None,
            text_block_index: None,
            strip_thinking_leading_newline: false,
            signature_sent: false,
            usage_tracker: None,
            api_key_id: None,
            credential_id: None,
            client_ip: None,
            prompt_cache_usage: PromptCacheUsage::uncached(input_tokens),
            metering_usage: None,
            metering_cache_read_tokens: None,
            metering_cache_creation_tokens: None,
            context_usage_percentage: None,
            prefix_estimated_tokens: None,
        }
    }

    /// 设置 prompt cache usage
    pub fn with_prompt_cache_usage(mut self, usage: PromptCacheUsage) -> Self {
        self.prompt_cache_usage = usage;
        self
    }

    /// 设置缓存前缀估算 token 数（system + tools + history[0..n-1] 本地估算）。
    /// 显式注入后，即使前缀 = 0 也会被选用（cache_read=0，全部计为 new_input）。
    pub fn with_prefix_estimated_tokens(mut self, prefix: i32) -> Self {
        self.prefix_estimated_tokens = Some(prefix);
        self
    }

    /// 设置用量追踪
    pub fn with_usage_tracking(
        mut self,
        tracker: Option<Arc<UsageTracker>>,
        api_key_id: Option<u32>,
        credential_id: Option<u64>,
        client_ip: Option<String>,
    ) -> Self {
        self.usage_tracker = tracker;
        self.api_key_id = api_key_id;
        self.credential_id = credential_id;
        self.client_ip = client_ip;
        self
    }

    /// 生成 message_start 事件
    pub fn create_message_start_event(&self) -> serde_json::Value {
        let usage = self.prompt_cache_usage;
        json!({
            "type": "message_start",
            "message": {
                "id": self.message_id,
                "type": "message",
                "role": "assistant",
                "content": [],
                "model": self.model,
                "stop_reason": null,
                "stop_sequence": null,
                "usage": {
                    "input_tokens": scale_for_client(usage.input_tokens, &self.model),
                    "output_tokens": 1,
                    "cache_creation_input_tokens": scale_for_client(usage.cache_creation_input_tokens, &self.model),
                    "cache_read_input_tokens": scale_for_client(usage.cache_read_input_tokens, &self.model)
                }
            }
        })
    }

    /// 生成初始事件序列 (message_start + 文本块 start)
    ///
    /// 当 thinking 启用时，不在初始化时创建文本块，而是等到实际收到内容时再创建。
    /// 这样可以确保 thinking 块（索引 0）在文本块（索引 1）之前。
    pub fn generate_initial_events(&mut self) -> Vec<SseEvent> {
        let mut events = Vec::new();

        // message_start
        let msg_start = self.create_message_start_event();
        if let Some(event) = self.state_manager.handle_message_start(msg_start) {
            events.push(event);
        }

        // 如果启用了 thinking，不在这里创建文本块
        // thinking 块和文本块会在 process_content_with_thinking 中按正确顺序创建
        if self.thinking_enabled {
            return events;
        }

        // 创建初始文本块（仅在未启用 thinking 时）
        let text_block_index = self.state_manager.next_block_index();
        self.text_block_index = Some(text_block_index);
        let text_block_events = self.state_manager.handle_content_block_start(
            text_block_index,
            "text",
            json!({
                "type": "content_block_start",
                "index": text_block_index,
                "content_block": {
                    "type": "text",
                    "text": ""
                }
            }),
        );
        events.extend(text_block_events);

        events
    }

    /// 处理 Kiro 事件并转换为 Anthropic SSE 事件
    pub fn process_kiro_event(&mut self, event: &Event) -> Vec<SseEvent> {
        match event {
            Event::AssistantResponse(resp) => self.process_assistant_response(&resp.content),
            Event::ToolUse(tool_use) => self.process_tool_use(tool_use),
            Event::ContextUsage(context_usage) => {
                // contextUsage 本地化：仅保留事件接收用于 stop_reason 兜底判定，
                // 不再用 percentage × window 反算 input_tokens。
                // final_input_tokens 来源改为：metering.inputTokens 真值 → 本地 count_all_tokens。
                self.context_usage_percentage = Some(context_usage.context_usage_percentage);
                if context_usage.context_usage_percentage >= 100.0 {
                    self.state_manager
                        .set_stop_reason("model_context_window_exceeded");
                }
                tracing::debug!(
                    "[deprecated] contextUsageEvent: {:.2}% (仅记录, 不参与 input_tokens 反算)",
                    context_usage.context_usage_percentage,
                );
                Vec::new()
            }
            Event::Metering(metering) => {
                let prev = self.metering_usage;
                self.metering_usage = Some(metering.usage);
                self.metering_cache_read_tokens = metering.cache_read_input_tokens;
                self.metering_cache_creation_tokens = metering.cache_creation_input_tokens;
                if let Some(prev_val) = prev {
                    tracing::warn!(
                        "[metering] 同一请求收到第2次 meteringEvent: new={} {} prev={} model={}",
                        metering.usage,
                        metering.unit_plural,
                        prev_val,
                        self.model
                    );
                } else {
                    tracing::info!(
                        "[metering] meteringEvent: usage={} {} model={} cache_read={:?} cache_creation={:?}",
                        metering.usage,
                        metering.unit_plural,
                        self.model,
                        metering.cache_read_input_tokens,
                        metering.cache_creation_input_tokens
                    );
                }
                Vec::new()
            }

            Event::CodeReference(code_ref) => {
                for r in &code_ref.references {
                    tracing::debug!(
                        "[code_reference] license={} repo={} url={}",
                        r.license_name,
                        r.repository,
                        r.url
                    );
                }
                Vec::new()
            }
            Event::Error {
                error_code,
                error_message,
            } => {
                tracing::error!("收到错误事件: {} - {}", error_code, error_message);
                Vec::new()
            }
            Event::Exception {
                exception_type,
                message,
            } => {
                // 处理 ContentLengthExceededException
                if exception_type == "ContentLengthExceededException" {
                    self.state_manager.set_stop_reason("max_tokens");
                }
                tracing::warn!("收到异常事件: {} - {}", exception_type, message);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// 处理助手响应事件
    fn process_assistant_response(&mut self, content: &str) -> Vec<SseEvent> {
        if content.is_empty() {
            return Vec::new();
        }

        // 估算 tokens
        self.output_tokens += estimate_tokens(content);

        // 如果启用了thinking，需要处理thinking块
        if self.thinking_enabled {
            return self.process_content_with_thinking(content);
        }

        // 非 thinking 模式同样复用统一的 text_delta 发送逻辑，
        // 以便在 tool_use 自动关闭文本块后能够自愈重建新的文本块，避免“吞字”。
        self.create_text_delta_events(content)
    }

    /// 处理包含thinking块的内容
    fn process_content_with_thinking(&mut self, content: &str) -> Vec<SseEvent> {
        let mut events = Vec::new();

        // 将内容添加到缓冲区进行处理
        self.thinking_buffer.push_str(content);

        loop {
            if !self.in_thinking_block && !self.thinking_extracted {
                // 查找 <thinking> 开始标签（跳过被反引号包裹的）
                if let Some(start_pos) = find_real_thinking_start_tag(&self.thinking_buffer) {
                    // 发送 <thinking> 之前的内容作为 text_delta
                    // 注意：如果前面只是空白字符（如 adaptive 模式返回的 \n\n），则跳过，
                    // 避免在 thinking 块之前产生无意义的 text 块导致客户端解析失败
                    let before_thinking = self.thinking_buffer[..start_pos].to_string();
                    if !before_thinking.is_empty() && !before_thinking.trim().is_empty() {
                        events.extend(self.create_text_delta_events(&before_thinking));
                    }

                    // 进入 thinking 块
                    self.in_thinking_block = true;
                    self.strip_thinking_leading_newline = true;
                    self.thinking_buffer =
                        self.thinking_buffer[start_pos + "<thinking>".len()..].to_string();

                    // 创建 thinking 块的 content_block_start 事件
                    let thinking_index = self.state_manager.next_block_index();
                    self.thinking_block_index = Some(thinking_index);
                    let start_events = self.state_manager.handle_content_block_start(
                        thinking_index,
                        "thinking",
                        json!({
                            "type": "content_block_start",
                            "index": thinking_index,
                            "content_block": {
                                "type": "thinking",
                                "thinking": ""
                            }
                        }),
                    );
                    events.extend(start_events);
                } else {
                    // 没有找到 <thinking>，检查是否可能是部分标签
                    // 保留可能是部分标签的内容
                    let target_len = self
                        .thinking_buffer
                        .len()
                        .saturating_sub("<thinking>".len());
                    let safe_len = find_char_boundary(&self.thinking_buffer, target_len);
                    if safe_len > 0 {
                        let safe_content = self.thinking_buffer[..safe_len].to_string();
                        // 如果 thinking 尚未提取，且安全内容只是空白字符，
                        // 则不发送为 text_delta，继续保留在缓冲区等待更多内容。
                        // 这避免了 4.6 模型中 <thinking> 标签跨事件分割时，
                        // 前导空白（如 "\n\n"）被错误地创建为 text 块，
                        // 导致 text 块先于 thinking 块出现的问题。
                        if !safe_content.is_empty() && !safe_content.trim().is_empty() {
                            events.extend(self.create_text_delta_events(&safe_content));
                            self.thinking_buffer = self.thinking_buffer[safe_len..].to_string();
                        }
                    }
                    break;
                }
            } else if self.in_thinking_block {
                // 剥离 <thinking> 标签后紧跟的换行符（可能跨 chunk）
                if self.strip_thinking_leading_newline {
                    if self.thinking_buffer.starts_with('\n') {
                        self.thinking_buffer = self.thinking_buffer[1..].to_string();
                        self.strip_thinking_leading_newline = false;
                    } else if !self.thinking_buffer.is_empty() {
                        // buffer 非空但不以 \n 开头，不再需要剥离
                        self.strip_thinking_leading_newline = false;
                    }
                    // buffer 为空时保留标志，等待下一个 chunk
                }

                // 在 thinking 块内，查找 </thinking> 结束标签（跳过被反引号包裹的）
                if let Some(end_pos) = find_real_thinking_end_tag(&self.thinking_buffer) {
                    // 提取 thinking 内容
                    let thinking_content = self.thinking_buffer[..end_pos].to_string();
                    if !thinking_content.is_empty()
                        && let Some(thinking_index) = self.thinking_block_index
                    {
                        events.push(
                            self.create_thinking_delta_event(thinking_index, &thinking_content),
                        );
                    }

                    // 结束 thinking 块
                    self.in_thinking_block = false;
                    self.thinking_extracted = true;

                    // 发送空的 thinking_delta 事件，然后发送 content_block_stop 事件
                    if let Some(thinking_index) = self.thinking_block_index {
                        // 先发送空的 thinking_delta
                        events.push(self.create_thinking_delta_event(thinking_index, ""));
                        // 注入 signature_delta（必须在 content_block_stop 之前）
                        events.extend(self.take_signature_events());
                        // 再发送 content_block_stop
                        if let Some(stop_event) =
                            self.state_manager.handle_content_block_stop(thinking_index)
                        {
                            events.push(stop_event);
                        }
                    }

                    // 剥离 `</thinking>\n\n`（find_real_thinking_end_tag 已确认 \n\n 存在）
                    self.thinking_buffer =
                        self.thinking_buffer[end_pos + "</thinking>\n\n".len()..].to_string();
                } else {
                    // 没有找到结束标签，发送当前缓冲区内容作为 thinking_delta。
                    // 保留末尾可能是部分 `</thinking>\n\n` 的内容：
                    // find_real_thinking_end_tag 要求标签后有 `\n\n` 才返回 Some，
                    // 因此保留区必须覆盖 `</thinking>\n\n` 的完整长度（13 字节），
                    // 否则当 `</thinking>` 已在 buffer 但 `\n\n` 尚未到达时，
                    // 标签的前几个字符会被错误地作为 thinking_delta 发出。
                    let target_len = self
                        .thinking_buffer
                        .len()
                        .saturating_sub("</thinking>\n\n".len());
                    let safe_len = find_char_boundary(&self.thinking_buffer, target_len);
                    if safe_len > 0 {
                        let safe_content = self.thinking_buffer[..safe_len].to_string();
                        if !safe_content.is_empty()
                            && let Some(thinking_index) = self.thinking_block_index
                        {
                            events.push(
                                self.create_thinking_delta_event(thinking_index, &safe_content),
                            );
                        }
                        self.thinking_buffer = self.thinking_buffer[safe_len..].to_string();
                    }
                    break;
                }
            } else {
                // thinking 已提取完成，剩余内容作为 text_delta
                if !self.thinking_buffer.is_empty() {
                    let remaining = self.thinking_buffer.clone();
                    self.thinking_buffer.clear();
                    events.extend(self.create_text_delta_events(&remaining));
                }
                break;
            }
        }

        events
    }

    /// 创建 text_delta 事件
    ///
    /// 如果文本块尚未创建，会先创建文本块。
    /// 当发生 tool_use 时，状态机会自动关闭当前文本块；后续文本会自动创建新的文本块继续输出。
    ///
    /// 返回值包含可能的 content_block_start 事件和 content_block_delta 事件。
    fn create_text_delta_events(&mut self, text: &str) -> Vec<SseEvent> {
        let mut events = Vec::new();

        // 如果当前 text_block_index 指向的块已经被关闭（例如 tool_use 开始时自动 stop），
        // 则丢弃该索引并创建新的文本块继续输出，避免 delta 被状态机拒绝导致“吞字”。
        if let Some(idx) = self.text_block_index
            && !self.state_manager.is_block_open_of_type(idx, "text")
        {
            self.text_block_index = None;
        }

        // 获取或创建文本块索引
        let text_index = if let Some(idx) = self.text_block_index {
            idx
        } else {
            // 文本块尚未创建，需要先创建
            let idx = self.state_manager.next_block_index();
            self.text_block_index = Some(idx);

            // 发送 content_block_start 事件
            let start_events = self.state_manager.handle_content_block_start(
                idx,
                "text",
                json!({
                    "type": "content_block_start",
                    "index": idx,
                    "content_block": {
                        "type": "text",
                        "text": ""
                    }
                }),
            );
            events.extend(start_events);
            idx
        };

        // 发送 content_block_delta 事件
        if let Some(delta_event) = self.state_manager.handle_content_block_delta(
            text_index,
            json!({
                "type": "content_block_delta",
                "index": text_index,
                "delta": {
                    "type": "text_delta",
                    "text": text
                }
            }),
        ) {
            events.push(delta_event);
        }

        events
    }

    /// 创建 thinking_delta 事件
    fn create_thinking_delta_event(&self, index: i32, thinking: &str) -> SseEvent {
        SseEvent::new(
            "content_block_delta",
            json!({
                "type": "content_block_delta",
                "index": index,
                "delta": {
                    "type": "thinking_delta",
                    "thinking": thinking
                }
            }),
        )
    }

    /// 处理工具使用事件
    fn process_tool_use(
        &mut self,
        tool_use: &crate::kiro::model::events::ToolUseEvent,
    ) -> Vec<SseEvent> {
        let mut events = Vec::new();

        self.state_manager.set_has_tool_use(true);

        // tool_use 必须发生在 thinking 结束之后。
        // 但当 `</thinking>` 后面没有 `\n\n`（例如紧跟 tool_use 或流结束）时，
        // thinking 结束标签会滞留在 thinking_buffer，导致后续 flush 时把 `</thinking>` 当作内容输出。
        // 这里在开始 tool_use block 前做一次“边界场景”的结束标签识别与过滤。
        if self.thinking_enabled
            && self.in_thinking_block
            && let Some(end_pos) = find_real_thinking_end_tag_at_buffer_end(&self.thinking_buffer)
        {
            let thinking_content = self.thinking_buffer[..end_pos].to_string();
            if !thinking_content.is_empty()
                && let Some(thinking_index) = self.thinking_block_index
            {
                events.push(self.create_thinking_delta_event(thinking_index, &thinking_content));
            }

            // 结束 thinking 块
            self.in_thinking_block = false;
            self.thinking_extracted = true;

            if let Some(thinking_index) = self.thinking_block_index {
                // 先发送空的 thinking_delta
                events.push(self.create_thinking_delta_event(thinking_index, ""));
                // 注入 signature_delta（必须在 content_block_stop 之前）
                events.extend(self.take_signature_events());
                // 再发送 content_block_stop
                if let Some(stop_event) =
                    self.state_manager.handle_content_block_stop(thinking_index)
                {
                    events.push(stop_event);
                }
            }

            // 把结束标签后的内容当作普通文本（通常为空或空白）
            let after_pos = end_pos + "</thinking>".len();
            let remaining = self.thinking_buffer[after_pos..].trim_start().to_string();
            self.thinking_buffer.clear();
            if !remaining.is_empty() {
                events.extend(self.create_text_delta_events(&remaining));
            }
        }

        // thinking 模式下，process_content_with_thinking 可能会为了探测 `<thinking>` 而暂存一小段尾部文本。
        // 如果此时直接开始 tool_use，状态机会自动关闭 text block，导致这段"待输出文本"看起来被 tool_use 吞掉。
        // 约束：只在尚未进入 thinking block、且 thinking 尚未被提取时，将缓冲区当作普通文本 flush。
        if self.thinking_enabled
            && !self.in_thinking_block
            && !self.thinking_extracted
            && !self.thinking_buffer.is_empty()
        {
            let buffered = std::mem::take(&mut self.thinking_buffer);
            events.extend(self.create_text_delta_events(&buffered));
        }

        // 获取或分配块索引
        let block_index = if let Some(&idx) = self.tool_block_indices.get(&tool_use.tool_use_id) {
            idx
        } else {
            let idx = self.state_manager.next_block_index();
            self.tool_block_indices
                .insert(tool_use.tool_use_id.clone(), idx);
            idx
        };

        // 发送 content_block_start
        let start_events = self.state_manager.handle_content_block_start(
            block_index,
            "tool_use",
            json!({
                "type": "content_block_start",
                "index": block_index,
                "content_block": {
                    "type": "tool_use",
                    "id": tool_use.tool_use_id,
                    "name": tool_use.name,
                    "input": {}
                }
            }),
        );
        events.extend(start_events);

        // 发送参数增量 (ToolUseEvent.input 是 String 类型)
        if !tool_use.input.is_empty() {
            self.output_tokens += (tool_use.input.len() as i32 + 3) / 4; // 估算 token

            if let Some(delta_event) = self.state_manager.handle_content_block_delta(
                block_index,
                json!({
                    "type": "content_block_delta",
                    "index": block_index,
                    "delta": {
                        "type": "input_json_delta",
                        "partial_json": tool_use.input
                    }
                }),
            ) {
                events.push(delta_event);
            }
        }

        // 如果是完整的工具调用（stop=true），发送 content_block_stop
        if tool_use.stop
            && let Some(stop_event) = self.state_manager.handle_content_block_stop(block_index)
        {
            events.push(stop_event);
        }

        events
    }

    /// 首次调用时生成 signature_delta 事件，后续调用返回空（签名只能在 content_block_stop 之前发送一次）
    fn take_signature_events(&mut self) -> Vec<SseEvent> {
        if self.signature_sent {
            return Vec::new();
        }
        self.signature_sent = true;
        self.generate_signature_events()
    }

    /// 生成伪造的 signature_delta 事件
    ///
    /// 检测工具要求 signature_delta 事件的 signature 字段总长度 >= 100 字符才能通过。
    /// 重要：signature_delta 必须附着在 thinking block 上，不能作为独立的 content block。
    /// 只有当流中实际产生了 thinking block 时才注入 signature。
    fn generate_signature_events(&mut self) -> Vec<SseEvent> {
        let thinking_index = match self.thinking_block_index {
            Some(idx) => idx,
            None => return Vec::new(),
        };

        let mut events = Vec::new();
        let signature_content = generate_fake_signature();

        let chunk_size = 40;
        let chunks: Vec<&str> = signature_content
            .as_bytes()
            .chunks(chunk_size)
            .map(|c| std::str::from_utf8(c).unwrap_or(""))
            .collect();

        for chunk in chunks {
            events.push(SseEvent::new(
                "content_block_delta",
                json!({
                    "type": "content_block_delta",
                    "index": thinking_index,
                    "delta": {
                        "type": "signature_delta",
                        "signature": chunk
                    }
                }),
            ));
        }

        events
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

        // 路径 1：完全空
        if self.output_tokens == 0 && no_tool_use && thinking_empty {
            return true;
        }

        // 路径 2：近似空 + 上下文过大
        // 当 output 极少且无工具调用时，若上下文已超过阈值，判定为退化的空响应
        if self.output_tokens > 0
            && self.output_tokens < NEAR_EMPTY_OUTPUT_THRESHOLD
            && no_tool_use
            && thinking_empty
        {
            let est = self.context_input_tokens.unwrap_or(self.input_tokens);
            if est >= empty_response_oversized_threshold(&self.model) {
                tracing::warn!(
                    "[near-empty] 检测到近似空响应: output_tokens={} input_tokens={} \
                     threshold={} — 视为上下文过大导致的退化响应",
                    self.output_tokens,
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

    /// 生成最终事件序列
    pub fn generate_final_events(&mut self) -> Vec<SseEvent> {
        let mut events = Vec::new();

        // Flush thinking_buffer 中的剩余内容
        if self.thinking_enabled && !self.thinking_buffer.is_empty() {
            if self.in_thinking_block {
                // 末尾可能残留 `</thinking>`（例如紧跟 tool_use 或流结束），需要在 flush 时过滤掉结束标签。
                if let Some(end_pos) =
                    find_real_thinking_end_tag_at_buffer_end(&self.thinking_buffer)
                {
                    let thinking_content = self.thinking_buffer[..end_pos].to_string();
                    if !thinking_content.is_empty()
                        && let Some(thinking_index) = self.thinking_block_index
                    {
                        events.push(
                            self.create_thinking_delta_event(thinking_index, &thinking_content),
                        );
                    }

                    // 关闭 thinking 块：先发送空的 thinking_delta，再发送 content_block_stop
                    if let Some(thinking_index) = self.thinking_block_index {
                        events.push(self.create_thinking_delta_event(thinking_index, ""));
                        // 注入 signature_delta（必须在 content_block_stop 之前）
                        events.extend(self.take_signature_events());
                        if let Some(stop_event) =
                            self.state_manager.handle_content_block_stop(thinking_index)
                        {
                            events.push(stop_event);
                        }
                    }

                    // 把结束标签后的内容当作普通文本（通常为空或空白）
                    let after_pos = end_pos + "</thinking>".len();
                    let remaining = self.thinking_buffer[after_pos..].trim_start().to_string();
                    self.thinking_buffer.clear();
                    self.in_thinking_block = false;
                    self.thinking_extracted = true;
                    if !remaining.is_empty() {
                        events.extend(self.create_text_delta_events(&remaining));
                    }
                } else {
                    // 如果还在 thinking 块内，发送剩余内容作为 thinking_delta
                    if let Some(thinking_index) = self.thinking_block_index {
                        events.push(
                            self.create_thinking_delta_event(thinking_index, &self.thinking_buffer),
                        );
                    }
                    // 关闭 thinking 块：先发送空的 thinking_delta，再发送 content_block_stop
                    if let Some(thinking_index) = self.thinking_block_index {
                        // 先发送空的 thinking_delta
                        events.push(self.create_thinking_delta_event(thinking_index, ""));
                        // 注入 signature_delta（必须在 content_block_stop 之前）
                        events.extend(self.take_signature_events());
                        // 再发送 content_block_stop
                        if let Some(stop_event) =
                            self.state_manager.handle_content_block_stop(thinking_index)
                        {
                            events.push(stop_event);
                        }
                    }
                }
            } else {
                // 否则发送剩余内容作为 text_delta
                let buffer_content = self.thinking_buffer.clone();
                events.extend(self.create_text_delta_events(&buffer_content));
            }
            self.thinking_buffer.clear();
        }

        // 如果整个流中只产生了 thinking 块，没有 text 也没有 tool_use，
        // 则设置 stop_reason 为 max_tokens（表示模型耗尽了 token 预算在思考上），
        // 并补发一套完整的 text 事件（内容为一个空格），确保 content 数组中有 text 块
        if self.thinking_enabled
            && self.thinking_block_index.is_some()
            && !self.state_manager.has_non_thinking_blocks()
        {
            self.state_manager.set_stop_reason("max_tokens");
            events.extend(self.create_text_delta_events(" "));
        }

        // contextUsage 本地化：input_tokens 来源改为 metering 真值 → 本地估算（self.input_tokens）
        // self.context_input_tokens 已弃用（始终为 None），保留诊断字段
        let raw_final_input_tokens = self.input_tokens;
        let final_input_tokens =
            cap_input_tokens(raw_final_input_tokens, self.input_tokens, &self.model);

        // 本地估算 ≥ 1M 兜底触发 stop_reason
        if final_input_tokens >= 1_000_000 {
            self.state_manager
                .set_stop_reason("model_context_window_exceeded");
        }

        tracing::info!(
            "[input_tokens] 本地化: estimated={} final={}",
            self.input_tokens,
            final_input_tokens
        );

        // 对外报告的 output_tokens 需要限制在合理范围
        let reported_output_tokens = self.output_tokens.min(OUTPUT_TOKENS_REPORT_CAP);

        // cache_read 派生优先级：
        //   1. Kiro metering 透传 cache_read/creation（实测当前版本不透传，保留兜底）
        //   2. 前缀估算 prefix_estimated_tokens.min(final_input_tokens)  ← 主路径
        //   3. PromptCacheUsage 模拟值（仅当前缀=0，例如无 system/tools 的单条请求）
        let sim_usage = self.prompt_cache_usage.scale_to(final_input_tokens);
        let (report_input, report_cache_creation, report_cache_read) =
            if let (Some(read), Some(creation)) = (
                self.metering_cache_read_tokens,
                self.metering_cache_creation_tokens,
            ) {
                let non_cached = final_input_tokens
                    .saturating_sub(read)
                    .saturating_sub(creation);
                (non_cached, Some(creation), Some(read))
            } else if let Some(prefix) = self.prefix_estimated_tokens {
                let read = prefix.max(0).min(final_input_tokens);
                let non_cached = final_input_tokens.saturating_sub(read);
                (non_cached, Some(0_i32), Some(read))
            } else {
                (
                    sim_usage.input_tokens,
                    Some(sim_usage.cache_creation_input_tokens),
                    Some(sim_usage.cache_read_input_tokens),
                )
            };

        // 记录用量（内部记录使用真实值）
        if let (Some(tracker), Some(key_id)) = (&self.usage_tracker, self.api_key_id) {
            let credits_per_ktok = self.metering_usage.map(|c| {
                if final_input_tokens > 0 {
                    c / (final_input_tokens as f64) * 1000.0
                } else {
                    0.0
                }
            });
            let effective_rate = self.metering_usage.map(|c| {
                let denom = final_input_tokens as f64 + 5.0 * self.output_tokens as f64;
                if denom > 0.0 { c / denom * 1000.0 } else { 0.0 }
            });
            tracing::info!(
                "[usage] 入库: model={} input={} output={} metering_credits={:?} credits_per_ktok={:?} effective_rate={:?} cache_read={:?} cache_creation={:?} api_key={} credential={:?}",
                self.model,
                final_input_tokens,
                self.output_tokens,
                self.metering_usage,
                credits_per_ktok,
                effective_rate,
                report_cache_read,
                report_cache_creation,
                key_id,
                self.credential_id
            );
            tracker.record(
                key_id,
                self.credential_id,
                self.model.clone(),
                final_input_tokens,
                self.output_tokens,
                self.client_ip.clone(),
                self.metering_usage,
                report_cache_read,
                report_cache_creation,
            );
        }
        // 流式 SSE 路径暂未接入 fingerprint，cache_creation 不区分 5m/1h tier，
        // 这里把 report_cache_creation 全部归到 5m（与非流式 metering Layer 1 一致）
        let report_creation_5m = report_cache_creation;
        let report_creation_1h = Some(0);

        events.extend(self.state_manager.generate_final_events(
            report_input,
            reported_output_tokens,
            report_cache_creation,
            report_cache_read,
            self.context_usage_percentage,
            report_creation_5m,
            report_creation_1h,
            &self.model,
        ));

        events
    }
}

/// 缓冲流处理上下文 - 用于 /cc/v1/messages 流式请求
///
/// 与 `StreamContext` 不同，此上下文会缓冲所有事件直到流结束，
/// 然后用从 `contextUsageEvent` 计算的正确 `input_tokens` 更正 `message_start` 事件。
///
/// 工作流程：
/// 1. 使用 `StreamContext` 正常处理所有 Kiro 事件
/// 2. 把生成的 SSE 事件缓存起来（而不是立即发送）
/// 3. 流结束时，找到 `message_start` 事件并更新其 `input_tokens`
/// 4. 一次性返回所有事件
pub struct BufferedStreamContext {
    /// 内部流处理上下文（复用现有的事件处理逻辑）
    inner: StreamContext,
    /// 缓冲的所有事件（包括 message_start、content_block_start 等）
    event_buffer: Vec<SseEvent>,
    /// 估算的 input_tokens（用于回退）
    estimated_input_tokens: i32,
    /// 是否已经生成了初始事件
    initial_events_generated: bool,
}

impl BufferedStreamContext {
    /// 创建缓冲流上下文
    pub fn new(
        model: impl Into<String>,
        estimated_input_tokens: i32,
        thinking_enabled: bool,
    ) -> Self {
        let inner =
            StreamContext::new_with_thinking(model, estimated_input_tokens, thinking_enabled);
        Self {
            inner,
            event_buffer: Vec::new(),
            estimated_input_tokens,
            initial_events_generated: false,
        }
    }

    /// 设置用量追踪
    pub fn with_usage_tracking(
        mut self,
        tracker: Option<Arc<UsageTracker>>,
        api_key_id: Option<u32>,
        credential_id: Option<u64>,
        client_ip: Option<String>,
    ) -> Self {
        self.inner = self
            .inner
            .with_usage_tracking(tracker, api_key_id, credential_id, client_ip);
        self
    }

    pub fn with_prompt_cache_usage(mut self, usage: PromptCacheUsage) -> Self {
        self.inner = self.inner.with_prompt_cache_usage(usage);
        self
    }

    pub fn with_prefix_estimated_tokens(mut self, prefix: i32) -> Self {
        self.inner = self.inner.with_prefix_estimated_tokens(prefix);
        self
    }

    /// 处理 Kiro 事件并缓冲结果
    ///
    /// 复用 StreamContext 的事件处理逻辑，但把结果缓存而不是立即发送。
    pub fn process_and_buffer(&mut self, event: &crate::kiro::model::events::Event) {
        // 首次处理事件时，先生成初始事件（message_start 等）
        if !self.initial_events_generated {
            let initial_events = self.inner.generate_initial_events();
            self.event_buffer.extend(initial_events);
            self.initial_events_generated = true;
        }

        // 处理事件并缓冲结果
        let events = self.inner.process_kiro_event(event);
        self.event_buffer.extend(events);
    }

    /// 检测上游是否返回了完全空的响应（委托给 inner StreamContext）。
    pub fn is_empty_response(&self) -> bool {
        self.inner.is_empty_response()
    }

    /// 估算的 input_tokens（用于诊断日志）。
    pub fn input_tokens(&self) -> i32 {
        self.estimated_input_tokens
    }

    /// 空响应是否由上下文过大导致（委托给 inner StreamContext）。
    pub fn empty_response_is_oversized_context(&self) -> bool {
        self.inner.empty_response_is_oversized_context()
    }

    /// 完成流处理并返回所有事件
    ///
    /// 此方法会：
    /// 1. 生成最终事件（message_delta, message_stop）
    /// 2. 用正确的 input_tokens 更正 message_start 事件
    /// 3. 返回所有缓冲的事件
    pub fn finish_and_get_all_events(&mut self) -> Vec<SseEvent> {
        // 如果从未处理过事件，也要生成初始事件
        if !self.initial_events_generated {
            let initial_events = self.inner.generate_initial_events();
            self.event_buffer.extend(initial_events);
            self.initial_events_generated = true;
        }

        // 生成最终事件
        let final_events = self.inner.generate_final_events();
        self.event_buffer.extend(final_events);

        // 获取正确的 input_tokens（带 cap）
        let raw_final_input_tokens = self
            .inner
            .context_input_tokens
            .unwrap_or(self.estimated_input_tokens);
        let final_input_tokens = cap_input_tokens(
            raw_final_input_tokens,
            self.estimated_input_tokens,
            &self.inner.model,
        );

        // cache_read 派生优先级（同 StreamContext）：metering 透传 → 前缀估算 → 模拟兜底
        let sim_usage = self.inner.prompt_cache_usage.scale_to(final_input_tokens);
        let (report_input, report_cache_creation, report_cache_read) =
            if let (Some(read), Some(creation)) = (
                self.inner.metering_cache_read_tokens,
                self.inner.metering_cache_creation_tokens,
            ) {
                let non_cached = final_input_tokens
                    .saturating_sub(read)
                    .saturating_sub(creation);
                (non_cached, creation, read)
            } else if let Some(prefix) = self.inner.prefix_estimated_tokens {
                let read = prefix.max(0).min(final_input_tokens);
                let non_cached = final_input_tokens.saturating_sub(read);
                (non_cached, 0_i32, read)
            } else {
                (
                    sim_usage.input_tokens,
                    sim_usage.cache_creation_input_tokens,
                    sim_usage.cache_read_input_tokens,
                )
            };

        // 更正 message_start 事件中的 usage 字段（客户端展示缩放）
        for event in &mut self.event_buffer {
            if event.event == "message_start"
                && let Some(message) = event.data.get_mut("message")
                && let Some(usage) = message.get_mut("usage")
            {
                usage["input_tokens"] = serde_json::json!(scale_for_client(report_input, &self.inner.model));
                usage["cache_creation_input_tokens"] =
                    serde_json::json!(scale_for_client(report_cache_creation, &self.inner.model));
                usage["cache_read_input_tokens"] =
                    serde_json::json!(scale_for_client(report_cache_read, &self.inner.model));
            }
        }

        std::mem::take(&mut self.event_buffer)
    }
}

fn generate_fake_signature() -> String {
    const BASE64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let len = 160;
    let mut sig = String::with_capacity(len + 2);
    for _ in 0..len {
        let idx = fastrand::usize(..BASE64_CHARS.len());
        sig.push(BASE64_CHARS[idx] as char);
    }
    sig.push('=');
    sig.push('=');
    sig
}

/// 简单的 token 估算
fn estimate_tokens(text: &str) -> i32 {
    let chars: Vec<char> = text.chars().collect();
    let mut chinese_count = 0;
    let mut other_count = 0;

    for c in &chars {
        if *c >= '\u{4E00}' && *c <= '\u{9FFF}' {
            chinese_count += 1;
        } else {
            other_count += 1;
        }
    }

    // 中文约 1.5 字符/token，英文约 4 字符/token
    let chinese_tokens = (chinese_count * 2 + 2) / 3;
    let other_tokens = (other_count + 3) / 4;

    (chinese_tokens + other_tokens).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scale_for_client_basic() {
        // 默认模型（非 4.7/4.8）：× 0.6657
        assert_eq!(scale_for_client(100_000, "claude-opus-4-6"), 66_570);
        assert_eq!(scale_for_client(85_000, "claude-sonnet-4-6"), 56_585);
        // sonnet-5 与 sonnet-4.6 同档，不归入大窗口分支
        assert_eq!(scale_for_client(100_000, "claude-sonnet-5"), 66_570);
        assert_eq!(scale_for_client(0, "claude-opus-4-6"), 0);
        assert_eq!(scale_for_client(1, "claude-opus-4-6"), 1);
        assert_eq!(scale_for_client(-100, "claude-opus-4-6"), 0);
    }

    #[test]
    fn test_scale_for_client_large_window_model() {
        // 4.7/4.8 模型：× 0.15
        assert_eq!(scale_for_client(100_000, "claude-opus-4-7"), 15_000);
        assert_eq!(scale_for_client(100_000, "claude-opus-4-8"), 15_000);
        assert_eq!(scale_for_client(200_000, "claude-opus-4-7"), 30_000);
        assert_eq!(scale_for_client(1, "claude-opus-4-8"), 1);
        assert_eq!(scale_for_client(0, "claude-opus-4-7"), 0);
    }

    #[test]
    fn test_scale_for_client_non_round() {
        // 11 × 0.6657 = ceil(7.3227) = 8
        assert_eq!(scale_for_client(11, "claude-opus-4-6"), 8);
        // 11 × 0.15 = ceil(1.65) = 2
        assert_eq!(scale_for_client(11, "claude-opus-4-7"), 2);
        // i32::MAX 不溢出
        let r = scale_for_client(i32::MAX, "claude-opus-4-6");
        assert!(r > 0 && r < i32::MAX);
        let r2 = scale_for_client(i32::MAX, "claude-opus-4-8");
        assert!(r2 > 0 && r2 < i32::MAX);
    }

    #[test]
    fn test_sse_event_format() {
        let event = SseEvent::new("message_start", json!({"type": "message_start"}));
        let sse_str = event.to_sse_string();

        assert!(sse_str.starts_with("event: message_start\n"));
        assert!(sse_str.contains("data: "));
        assert!(sse_str.ends_with("\n\n"));
    }

    #[test]
    fn test_sse_state_manager_message_start() {
        let mut manager = SseStateManager::new();

        // 第一次应该成功
        let event = manager.handle_message_start(json!({"type": "message_start"}));
        assert!(event.is_some());

        // 第二次应该被跳过
        let event = manager.handle_message_start(json!({"type": "message_start"}));
        assert!(event.is_none());
    }

    #[test]
    fn test_sse_state_manager_block_lifecycle() {
        let mut manager = SseStateManager::new();

        // 创建块
        let events = manager.handle_content_block_start(0, "text", json!({}));
        assert_eq!(events.len(), 1);

        // delta
        let event = manager.handle_content_block_delta(0, json!({}));
        assert!(event.is_some());

        // stop
        let event = manager.handle_content_block_stop(0);
        assert!(event.is_some());

        // 重复 stop 应该被跳过
        let event = manager.handle_content_block_stop(0);
        assert!(event.is_none());
    }

    #[test]
    fn test_text_delta_after_tool_use_restarts_text_block() {
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, false);

        let initial_events = ctx.generate_initial_events();
        assert!(
            initial_events
                .iter()
                .any(|e| e.event == "content_block_start"
                    && e.data["content_block"]["type"] == "text")
        );

        let initial_text_index = ctx
            .text_block_index
            .expect("initial text block index should exist");

        // tool_use 开始会自动关闭现有 text block
        let tool_events = ctx.process_tool_use(&crate::kiro::model::events::ToolUseEvent {
            name: "test_tool".to_string(),
            tool_use_id: "tool_1".to_string(),
            input: "{}".to_string(),
            stop: false,
        });
        assert!(
            tool_events.iter().any(|e| {
                e.event == "content_block_stop"
                    && e.data["index"].as_i64() == Some(initial_text_index as i64)
            }),
            "tool_use should stop the previous text block"
        );

        // 之后再来文本增量，应自动创建新的 text block 而不是往已 stop 的块里写 delta
        let text_events = ctx.process_assistant_response("hello");
        let new_text_start_index = text_events.iter().find_map(|e| {
            if e.event == "content_block_start" && e.data["content_block"]["type"] == "text" {
                e.data["index"].as_i64()
            } else {
                None
            }
        });
        assert!(
            new_text_start_index.is_some(),
            "should start a new text block"
        );
        assert_ne!(
            new_text_start_index.unwrap(),
            initial_text_index as i64,
            "new text block index should differ from the stopped one"
        );
        assert!(
            text_events.iter().any(|e| {
                e.event == "content_block_delta"
                    && e.data["delta"]["type"] == "text_delta"
                    && e.data["delta"]["text"] == "hello"
            }),
            "should emit text_delta after restarting text block"
        );
    }

    #[test]
    fn test_tool_use_flushes_pending_thinking_buffer_text_before_tool_block() {
        // thinking 模式下，短文本可能被暂存在 thinking_buffer 以等待 `<thinking>` 的跨 chunk 匹配。
        // 当紧接着出现 tool_use 时，应先 flush 这段文本，再开始 tool_use block。
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, true);
        let _initial_events = ctx.generate_initial_events();

        // 两段短文本（各 2 个中文字符），总长度仍可能不足以满足 safe_len>0 的输出条件，
        // 因而会留在 thinking_buffer 中等待后续 chunk。
        let ev1 = ctx.process_assistant_response("有修");
        assert!(
            ev1.iter().all(|e| e.event != "content_block_delta"),
            "short prefix should be buffered under thinking mode"
        );
        let ev2 = ctx.process_assistant_response("改：");
        assert!(
            ev2.iter().all(|e| e.event != "content_block_delta"),
            "short prefix should still be buffered under thinking mode"
        );

        let events = ctx.process_tool_use(&crate::kiro::model::events::ToolUseEvent {
            name: "Write".to_string(),
            tool_use_id: "tool_1".to_string(),
            input: "{}".to_string(),
            stop: false,
        });

        let text_start_index = events.iter().find_map(|e| {
            if e.event == "content_block_start" && e.data["content_block"]["type"] == "text" {
                e.data["index"].as_i64()
            } else {
                None
            }
        });
        let pos_text_delta = events.iter().position(|e| {
            e.event == "content_block_delta" && e.data["delta"]["type"] == "text_delta"
        });
        let pos_text_stop = text_start_index.and_then(|idx| {
            events.iter().position(|e| {
                e.event == "content_block_stop" && e.data["index"].as_i64() == Some(idx)
            })
        });
        let pos_tool_start = events.iter().position(|e| {
            e.event == "content_block_start" && e.data["content_block"]["type"] == "tool_use"
        });

        assert!(
            text_start_index.is_some(),
            "should start a text block to flush buffered text"
        );
        assert!(
            pos_text_delta.is_some(),
            "should flush buffered text as text_delta"
        );
        assert!(
            pos_text_stop.is_some(),
            "should stop text block before tool_use block starts"
        );
        assert!(pos_tool_start.is_some(), "should start tool_use block");

        let pos_text_delta = pos_text_delta.unwrap();
        let pos_text_stop = pos_text_stop.unwrap();
        let pos_tool_start = pos_tool_start.unwrap();

        assert!(
            pos_text_delta < pos_text_stop && pos_text_stop < pos_tool_start,
            "ordering should be: text_delta -> text_stop -> tool_use_start"
        );

        assert!(
            events.iter().any(|e| {
                e.event == "content_block_delta"
                    && e.data["delta"]["type"] == "text_delta"
                    && e.data["delta"]["text"] == "有修改："
            }),
            "flushed text should equal the buffered prefix"
        );
    }

    #[test]
    fn test_estimate_tokens() {
        assert!(estimate_tokens("Hello") > 0);
        assert!(estimate_tokens("你好") > 0);
        assert!(estimate_tokens("Hello 你好") > 0);
    }

    #[test]
    fn test_find_real_thinking_start_tag_basic() {
        // 基本情况：正常的开始标签
        assert_eq!(find_real_thinking_start_tag("<thinking>"), Some(0));
        assert_eq!(find_real_thinking_start_tag("prefix<thinking>"), Some(6));
    }

    #[test]
    fn test_find_real_thinking_start_tag_with_backticks() {
        // 被反引号包裹的应该被跳过
        assert_eq!(find_real_thinking_start_tag("`<thinking>`"), None);
        assert_eq!(find_real_thinking_start_tag("use `<thinking>` tag"), None);

        // 先有被包裹的，后有真正的开始标签
        assert_eq!(
            find_real_thinking_start_tag("about `<thinking>` tag<thinking>content"),
            Some(22)
        );
    }

    #[test]
    fn test_find_real_thinking_start_tag_with_quotes() {
        // 被双引号包裹的应该被跳过
        assert_eq!(find_real_thinking_start_tag("\"<thinking>\""), None);
        assert_eq!(find_real_thinking_start_tag("the \"<thinking>\" tag"), None);

        // 被单引号包裹的应该被跳过
        assert_eq!(find_real_thinking_start_tag("'<thinking>'"), None);

        // 混合情况
        assert_eq!(
            find_real_thinking_start_tag("about \"<thinking>\" and '<thinking>' then<thinking>"),
            Some(40)
        );
    }

    #[test]
    fn test_find_real_thinking_end_tag_basic() {
        // 基本情况：正常的结束标签后面有双换行符
        assert_eq!(find_real_thinking_end_tag("</thinking>\n\n"), Some(0));
        assert_eq!(
            find_real_thinking_end_tag("content</thinking>\n\n"),
            Some(7)
        );
        assert_eq!(
            find_real_thinking_end_tag("some text</thinking>\n\nmore text"),
            Some(9)
        );

        // 没有双换行符的情况
        assert_eq!(find_real_thinking_end_tag("</thinking>"), None);
        assert_eq!(find_real_thinking_end_tag("</thinking>\n"), None);
        assert_eq!(find_real_thinking_end_tag("</thinking> more"), None);
    }

    #[test]
    fn test_find_real_thinking_end_tag_with_backticks() {
        // 被反引号包裹的应该被跳过
        assert_eq!(find_real_thinking_end_tag("`</thinking>`\n\n"), None);
        assert_eq!(
            find_real_thinking_end_tag("mention `</thinking>` in code\n\n"),
            None
        );

        // 只有前面有反引号
        assert_eq!(find_real_thinking_end_tag("`</thinking>\n\n"), None);

        // 只有后面有反引号
        assert_eq!(find_real_thinking_end_tag("</thinking>`\n\n"), None);
    }

    #[test]
    fn test_find_real_thinking_end_tag_with_quotes() {
        // 被双引号包裹的应该被跳过
        assert_eq!(find_real_thinking_end_tag("\"</thinking>\"\n\n"), None);
        assert_eq!(
            find_real_thinking_end_tag("the string \"</thinking>\" is a tag\n\n"),
            None
        );

        // 被单引号包裹的应该被跳过
        assert_eq!(find_real_thinking_end_tag("'</thinking>'\n\n"), None);
        assert_eq!(
            find_real_thinking_end_tag("use '</thinking>' as marker\n\n"),
            None
        );

        // 混合情况：双引号包裹后有真正的标签
        assert_eq!(
            find_real_thinking_end_tag("about \"</thinking>\" tag</thinking>\n\n"),
            Some(23)
        );

        // 混合情况：单引号包裹后有真正的标签
        assert_eq!(
            find_real_thinking_end_tag("about '</thinking>' tag</thinking>\n\n"),
            Some(23)
        );
    }

    #[test]
    fn test_find_real_thinking_end_tag_mixed() {
        // 先有被包裹的，后有真正的结束标签
        assert_eq!(
            find_real_thinking_end_tag("discussing `</thinking>` tag</thinking>\n\n"),
            Some(28)
        );

        // 多个被包裹的，最后一个是真正的
        assert_eq!(
            find_real_thinking_end_tag("`</thinking>` and `</thinking>` done</thinking>\n\n"),
            Some(36)
        );

        // 多种引用字符混合
        assert_eq!(
            find_real_thinking_end_tag(
                "`</thinking>` and \"</thinking>\" and '</thinking>' done</thinking>\n\n"
            ),
            Some(54)
        );
    }

    #[test]
    fn test_tool_use_immediately_after_thinking_filters_end_tag_and_closes_thinking_block() {
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, true);
        let _initial_events = ctx.generate_initial_events();

        let mut all_events = Vec::new();

        // thinking 内容以 `</thinking>` 结尾，但后面没有 `\n\n`（模拟紧跟 tool_use 的场景）
        all_events.extend(ctx.process_assistant_response("<thinking>abc</thinking>"));

        let tool_events = ctx.process_tool_use(&crate::kiro::model::events::ToolUseEvent {
            name: "Write".to_string(),
            tool_use_id: "tool_1".to_string(),
            input: "{}".to_string(),
            stop: false,
        });
        all_events.extend(tool_events);

        all_events.extend(ctx.generate_final_events());

        // 不应把 `</thinking>` 当作 thinking 内容输出
        assert!(
            all_events.iter().all(|e| {
                !(e.event == "content_block_delta"
                    && e.data["delta"]["type"] == "thinking_delta"
                    && e.data["delta"]["thinking"] == "</thinking>")
            }),
            "`</thinking>` should be filtered from output"
        );

        // thinking block 必须在 tool_use block 之前关闭
        let thinking_index = ctx
            .thinking_block_index
            .expect("thinking block index should exist");
        let pos_thinking_stop = all_events.iter().position(|e| {
            e.event == "content_block_stop"
                && e.data["index"].as_i64() == Some(thinking_index as i64)
        });
        let pos_tool_start = all_events.iter().position(|e| {
            e.event == "content_block_start" && e.data["content_block"]["type"] == "tool_use"
        });
        assert!(
            pos_thinking_stop.is_some(),
            "thinking block should be stopped"
        );
        assert!(pos_tool_start.is_some(), "tool_use block should be started");
        assert!(
            pos_thinking_stop.unwrap() < pos_tool_start.unwrap(),
            "thinking block should stop before tool_use block starts"
        );
    }

    #[test]
    fn test_final_flush_filters_standalone_thinking_end_tag() {
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, true);
        let _initial_events = ctx.generate_initial_events();

        let mut all_events = Vec::new();
        all_events.extend(ctx.process_assistant_response("<thinking>abc</thinking>"));
        all_events.extend(ctx.generate_final_events());

        assert!(
            all_events.iter().all(|e| {
                !(e.event == "content_block_delta"
                    && e.data["delta"]["type"] == "thinking_delta"
                    && e.data["delta"]["thinking"] == "</thinking>")
            }),
            "`</thinking>` should be filtered during final flush"
        );
    }

    #[test]
    fn test_thinking_strips_leading_newline_same_chunk() {
        // <thinking>\n 在同一个 chunk 中，\n 应被剥离
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, true);
        let _initial_events = ctx.generate_initial_events();

        let events = ctx.process_assistant_response("<thinking>\nHello world");

        // 找到所有 thinking_delta 事件
        let thinking_deltas: Vec<_> = events
            .iter()
            .filter(|e| {
                e.event == "content_block_delta" && e.data["delta"]["type"] == "thinking_delta"
            })
            .collect();

        // 拼接所有 thinking 内容
        let full_thinking: String = thinking_deltas
            .iter()
            .map(|e| e.data["delta"]["thinking"].as_str().unwrap_or(""))
            .collect();

        assert!(
            !full_thinking.starts_with('\n'),
            "thinking content should not start with \\n, got: {:?}",
            full_thinking
        );
    }

    #[test]
    fn test_thinking_strips_leading_newline_cross_chunk() {
        // <thinking> 在第一个 chunk 末尾，\n 在第二个 chunk 开头
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, true);
        let _initial_events = ctx.generate_initial_events();

        let events1 = ctx.process_assistant_response("<thinking>");
        let events2 = ctx.process_assistant_response("\nHello world");

        let mut all_events = Vec::new();
        all_events.extend(events1);
        all_events.extend(events2);

        let thinking_deltas: Vec<_> = all_events
            .iter()
            .filter(|e| {
                e.event == "content_block_delta" && e.data["delta"]["type"] == "thinking_delta"
            })
            .collect();

        let full_thinking: String = thinking_deltas
            .iter()
            .map(|e| e.data["delta"]["thinking"].as_str().unwrap_or(""))
            .collect();

        assert!(
            !full_thinking.starts_with('\n'),
            "thinking content should not start with \\n across chunks, got: {:?}",
            full_thinking
        );
    }

    #[test]
    fn test_thinking_no_strip_when_no_leading_newline() {
        // <thinking> 后直接跟内容（无 \n），内容应完整保留
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, true);
        let _initial_events = ctx.generate_initial_events();

        let events = ctx.process_assistant_response("<thinking>abc</thinking>\n\ntext");

        let thinking_deltas: Vec<_> = events
            .iter()
            .filter(|e| {
                e.event == "content_block_delta" && e.data["delta"]["type"] == "thinking_delta"
            })
            .collect();

        let full_thinking: String = thinking_deltas
            .iter()
            .filter(|e| {
                !e.data["delta"]["thinking"]
                    .as_str()
                    .unwrap_or("")
                    .is_empty()
            })
            .map(|e| e.data["delta"]["thinking"].as_str().unwrap_or(""))
            .collect();

        assert_eq!(full_thinking, "abc", "thinking content should be 'abc'");
    }

    #[test]
    fn test_text_after_thinking_strips_leading_newlines() {
        // `</thinking>\n\n` 后的文本不应以 \n\n 开头
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, true);
        let _initial_events = ctx.generate_initial_events();

        let events = ctx.process_assistant_response("<thinking>\nabc</thinking>\n\n你好");

        let text_deltas: Vec<_> = events
            .iter()
            .filter(|e| e.event == "content_block_delta" && e.data["delta"]["type"] == "text_delta")
            .collect();

        let full_text: String = text_deltas
            .iter()
            .map(|e| e.data["delta"]["text"].as_str().unwrap_or(""))
            .collect();

        assert!(
            !full_text.starts_with('\n'),
            "text after thinking should not start with \\n, got: {:?}",
            full_text
        );
        assert_eq!(full_text, "你好");
    }

    /// 辅助函数：从事件列表中提取所有 thinking_delta 的拼接内容
    fn collect_thinking_content(events: &[SseEvent]) -> String {
        events
            .iter()
            .filter(|e| {
                e.event == "content_block_delta" && e.data["delta"]["type"] == "thinking_delta"
            })
            .map(|e| e.data["delta"]["thinking"].as_str().unwrap_or(""))
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// 辅助函数：从事件列表中提取所有 text_delta 的拼接内容
    fn collect_text_content(events: &[SseEvent]) -> String {
        events
            .iter()
            .filter(|e| e.event == "content_block_delta" && e.data["delta"]["type"] == "text_delta")
            .map(|e| e.data["delta"]["text"].as_str().unwrap_or(""))
            .collect()
    }

    #[test]
    fn test_end_tag_newlines_split_across_events() {
        // `</thinking>\n` 在 chunk 1，`\n` 在 chunk 2，`text` 在 chunk 3
        // 确保 `</thinking>` 不会被部分当作 thinking 内容发出
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, true);
        let _initial_events = ctx.generate_initial_events();

        let mut all = Vec::new();
        all.extend(ctx.process_assistant_response("<thinking>\nabc</thinking>\n"));
        all.extend(ctx.process_assistant_response("\n"));
        all.extend(ctx.process_assistant_response("你好"));
        all.extend(ctx.generate_final_events());

        let thinking = collect_thinking_content(&all);
        assert_eq!(
            thinking, "abc",
            "thinking should be 'abc', got: {:?}",
            thinking
        );

        let text = collect_text_content(&all);
        assert_eq!(text, "你好", "text should be '你好', got: {:?}", text);
    }

    #[test]
    fn test_end_tag_alone_in_chunk_then_newlines_in_next() {
        // `</thinking>` 单独在一个 chunk，`\n\ntext` 在下一个 chunk
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, true);
        let _initial_events = ctx.generate_initial_events();

        let mut all = Vec::new();
        all.extend(ctx.process_assistant_response("<thinking>\nabc</thinking>"));
        all.extend(ctx.process_assistant_response("\n\n你好"));
        all.extend(ctx.generate_final_events());

        let thinking = collect_thinking_content(&all);
        assert_eq!(
            thinking, "abc",
            "thinking should be 'abc', got: {:?}",
            thinking
        );

        let text = collect_text_content(&all);
        assert_eq!(text, "你好", "text should be '你好', got: {:?}", text);
    }

    #[test]
    fn test_start_tag_newline_split_across_events() {
        // `\n\n` 在 chunk 1，`<thinking>` 在 chunk 2，`\n` 在 chunk 3
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, true);
        let _initial_events = ctx.generate_initial_events();

        let mut all = Vec::new();
        all.extend(ctx.process_assistant_response("\n\n"));
        all.extend(ctx.process_assistant_response("<thinking>"));
        all.extend(ctx.process_assistant_response("\n"));
        all.extend(ctx.process_assistant_response("abc</thinking>\n\ntext"));
        all.extend(ctx.generate_final_events());

        let thinking = collect_thinking_content(&all);
        assert_eq!(
            thinking, "abc",
            "thinking should be 'abc', got: {:?}",
            thinking
        );

        let text = collect_text_content(&all);
        assert_eq!(text, "text", "text should be 'text', got: {:?}", text);
    }

    #[test]
    fn test_full_flow_maximally_split() {
        // 极端拆分：每个关键边界都在不同 chunk
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, true);
        let _initial_events = ctx.generate_initial_events();

        let mut all = Vec::new();
        // \n\n<thinking>\n 拆成多段
        all.extend(ctx.process_assistant_response("\n"));
        all.extend(ctx.process_assistant_response("\n"));
        all.extend(ctx.process_assistant_response("<thin"));
        all.extend(ctx.process_assistant_response("king>"));
        all.extend(ctx.process_assistant_response("\n"));
        all.extend(ctx.process_assistant_response("hello"));
        // </thinking>\n\n 拆成多段
        all.extend(ctx.process_assistant_response("</thi"));
        all.extend(ctx.process_assistant_response("nking>"));
        all.extend(ctx.process_assistant_response("\n"));
        all.extend(ctx.process_assistant_response("\n"));
        all.extend(ctx.process_assistant_response("world"));
        all.extend(ctx.generate_final_events());

        let thinking = collect_thinking_content(&all);
        assert_eq!(
            thinking, "hello",
            "thinking should be 'hello', got: {:?}",
            thinking
        );

        let text = collect_text_content(&all);
        assert_eq!(text, "world", "text should be 'world', got: {:?}", text);
    }

    #[test]
    fn test_thinking_only_sets_max_tokens_stop_reason() {
        // 整个流只有 thinking 块，没有 text 也没有 tool_use，stop_reason 应为 max_tokens
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, true);
        let _initial_events = ctx.generate_initial_events();

        let mut all_events = Vec::new();
        all_events.extend(ctx.process_assistant_response("<thinking>\nabc</thinking>"));
        all_events.extend(ctx.generate_final_events());

        let message_delta = all_events
            .iter()
            .find(|e| e.event == "message_delta")
            .expect("should have message_delta event");

        assert_eq!(
            message_delta.data["delta"]["stop_reason"], "max_tokens",
            "stop_reason should be max_tokens when only thinking is produced"
        );

        // 应补发一套完整的 text 事件（content_block_start + delta 空格 + content_block_stop）
        assert!(
            all_events.iter().any(|e| {
                e.event == "content_block_start" && e.data["content_block"]["type"] == "text"
            }),
            "should emit text content_block_start"
        );
        assert!(
            all_events.iter().any(|e| {
                e.event == "content_block_delta"
                    && e.data["delta"]["type"] == "text_delta"
                    && e.data["delta"]["text"] == " "
            }),
            "should emit text_delta with a single space"
        );
        // text block 应被 generate_final_events 自动关闭
        let text_block_index = all_events
            .iter()
            .find_map(|e| {
                if e.event == "content_block_start" && e.data["content_block"]["type"] == "text" {
                    e.data["index"].as_i64()
                } else {
                    None
                }
            })
            .expect("text block should exist");
        assert!(
            all_events.iter().any(|e| {
                e.event == "content_block_stop"
                    && e.data["index"].as_i64() == Some(text_block_index)
            }),
            "text block should be stopped"
        );
    }

    #[test]
    fn test_thinking_with_text_keeps_end_turn_stop_reason() {
        // thinking + text 的情况，stop_reason 应为 end_turn
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, true);
        let _initial_events = ctx.generate_initial_events();

        let mut all_events = Vec::new();
        all_events.extend(ctx.process_assistant_response("<thinking>\nabc</thinking>\n\nHello"));
        all_events.extend(ctx.generate_final_events());

        let message_delta = all_events
            .iter()
            .find(|e| e.event == "message_delta")
            .expect("should have message_delta event");

        assert_eq!(
            message_delta.data["delta"]["stop_reason"], "end_turn",
            "stop_reason should be end_turn when text is also produced"
        );
    }

    #[test]
    fn test_thinking_with_tool_use_keeps_tool_use_stop_reason() {
        // thinking + tool_use 的情况，stop_reason 应为 tool_use
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, true);
        let _initial_events = ctx.generate_initial_events();

        let mut all_events = Vec::new();
        all_events.extend(ctx.process_assistant_response("<thinking>\nabc</thinking>"));
        all_events.extend(
            ctx.process_tool_use(&crate::kiro::model::events::ToolUseEvent {
                name: "test_tool".to_string(),
                tool_use_id: "tool_1".to_string(),
                input: "{}".to_string(),
                stop: true,
            }),
        );
        all_events.extend(ctx.generate_final_events());

        let message_delta = all_events
            .iter()
            .find(|e| e.event == "message_delta")
            .expect("should have message_delta event");

        assert_eq!(
            message_delta.data["delta"]["stop_reason"], "tool_use",
            "stop_reason should be tool_use when tool_use is present"
        );
    }

    /// 复现 bug: max_tokens 先被设置，随后出现 tool_use。
    /// 期望 stop_reason == "tool_use"（tool_use 优先级最高），
    /// 当前实现会错误地保留 "max_tokens"，导致客户端只渲染不执行工具。
    #[test]
    fn repro_max_tokens_then_tool_use_must_be_tool_use() {
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, false);
        let _ = ctx.generate_initial_events();

        // 先收到 ContentLengthExceededException → 设 max_tokens
        ctx.process_kiro_event(&crate::kiro::model::events::Event::Exception {
            exception_type: "ContentLengthExceededException".to_string(),
            message: "too long".to_string(),
        });
        // 随后模型仍发起了工具调用
        ctx.process_tool_use(&crate::kiro::model::events::ToolUseEvent {
            name: "Write".to_string(),
            tool_use_id: "toolu_Z".to_string(),
            input: r#"{"file_path":"/tmp/x.py","content":"print(1)"}"#.to_string(),
            stop: true,
        });
        let finals = ctx.generate_final_events();
        let md = finals
            .iter()
            .find(|e| e.event == "message_delta")
            .expect("message_delta");
        assert_eq!(
            md.data["delta"]["stop_reason"], "tool_use",
            "存在 tool_use 时 stop_reason 必须是 tool_use，不能被 max_tokens 覆盖"
        );
    }

    /// 复现 bug: context 100% 先被设置（model_context_window_exceeded），随后 tool_use。
    #[test]
    fn repro_context_exceeded_then_tool_use_must_be_tool_use() {
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, false);
        let _ = ctx.generate_initial_events();

        ctx.process_kiro_event(&crate::kiro::model::events::Event::ContextUsage(
            crate::kiro::model::events::ContextUsageEvent {
                context_usage_percentage: 100.0,
            },
        ));
        ctx.process_tool_use(&crate::kiro::model::events::ToolUseEvent {
            name: "Read".to_string(),
            tool_use_id: "toolu_W".to_string(),
            input: r#"{"file_path":"/a"}"#.to_string(),
            stop: true,
        });
        let finals = ctx.generate_final_events();
        let md = finals
            .iter()
            .find(|e| e.event == "message_delta")
            .expect("message_delta");
        assert_eq!(
            md.data["delta"]["stop_reason"], "tool_use",
            "存在 tool_use 时 stop_reason 必须是 tool_use，不能被 model_context_window_exceeded 覆盖"
        );
    }

    #[test]
    fn test_empty_response_detected() {
        // 上游完全没发任何内容事件 → is_empty_response 为 true
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, false);
        let _ = ctx.generate_initial_events();
        assert!(
            ctx.is_empty_response(),
            "未收到任何内容事件时应判定为空响应"
        );
    }

    #[test]
    fn test_empty_response_oversized_context_by_threshold() {
        // contextUsage 本地化后所有模型按 1M 窗口，阈值 = 1M * 0.28 = 280_000
        // 大输入(>=28万)的空响应 → 判定为上下文过大
        let big = StreamContext::new_with_thinking("test-model", 300_000, true);
        assert!(
            big.empty_response_is_oversized_context(),
            "input 30万应判定为上下文过大"
        );
        // 小输入的空响应 → 视为偶发，可重试
        let small = StreamContext::new_with_thinking("test-model", 100_000, true);
        assert!(
            !small.empty_response_is_oversized_context(),
            "input 10万不应判定为上下文过大"
        );
    }

    #[test]
    fn test_non_empty_response_not_flagged() {
        // 收到了文本内容 → 不应判定为空响应
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, false);
        let _ = ctx.generate_initial_events();
        ctx.process_assistant_response("hello");
        assert!(!ctx.is_empty_response(), "已产生文本内容时不应判定为空响应");
    }

    #[test]
    fn test_tool_only_response_not_flagged() {
        // 只有工具调用、无文本 → 不是空响应
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, false);
        let _ = ctx.generate_initial_events();
        ctx.process_tool_use(&crate::kiro::model::events::ToolUseEvent {
            name: "Bash".to_string(),
            tool_use_id: "toolu_1".to_string(),
            input: "{}".to_string(),
            stop: true,
        });
        assert!(!ctx.is_empty_response(), "仅有工具调用时不应判定为空响应");
    }

    #[test]
    fn test_near_empty_response_oversized_context_flagged() {
        // 大上下文（>28万）+ 极短输出（< 30 tokens）+ 无工具调用 → 视为退化空响应
        let mut ctx = StreamContext::new_with_thinking("test-model", 300_000, false);
        let _ = ctx.generate_initial_events();
        // 模拟极短输出（手动设置 output_tokens 为一个小值）
        ctx.output_tokens = 10;
        assert!(
            ctx.is_empty_response(),
            "大上下文+极短输出应判定为近似空响应"
        );
    }

    #[test]
    fn test_near_empty_response_small_context_not_flagged() {
        // 小上下文 + 极短输出 → 不应判定为空响应（可能是正常的短回复）
        let mut ctx = StreamContext::new_with_thinking("test-model", 50_000, false);
        let _ = ctx.generate_initial_events();
        ctx.output_tokens = 10;
        assert!(
            !ctx.is_empty_response(),
            "小上下文+极短输出不应判定为空响应"
        );
    }

    #[test]
    fn test_near_empty_response_with_tool_use_not_flagged() {
        // 大上下文 + 极短输出 + 有工具调用 → 不应判定为空响应（工具调用是有效响应）
        let mut ctx = StreamContext::new_with_thinking("test-model", 300_000, false);
        let _ = ctx.generate_initial_events();
        ctx.output_tokens = 10;
        ctx.state_manager.set_has_tool_use(true);
        assert!(
            !ctx.is_empty_response(),
            "有工具调用时不应判定为空响应"
        );
    }

    #[test]
    fn test_context_window_opus_4_6_is_1m() {
        assert_eq!(context_window_for_model("claude-opus-4-6"), 1_000_000);
        assert_eq!(
            context_window_for_model("claude-opus-4-6-thinking"),
            1_000_000
        );
    }

    #[test]
    fn test_context_window_fable_5_is_1m() {
        assert_eq!(context_window_for_model("claude-fable-5"), 1_000_000);
        assert_eq!(
            context_window_for_model("claude-fable-5-thinking"),
            1_000_000
        );
    }

    #[test]
    fn test_context_window_all_models_unified_to_1m() {
        // contextUsage 本地化后所有模型统一返回 1M
        assert_eq!(
            context_window_for_model("claude-haiku-4-5-20251001"),
            1_000_000
        );
        assert_eq!(context_window_for_model("unknown-model"), 1_000_000);
        assert_eq!(context_window_for_model(""), 1_000_000);
    }

    #[test]
    fn test_context_window_existing_branches_unchanged() {
        // 回归：4-7 / 4-8 / sonnet-4-6 仍为 1M
        assert_eq!(context_window_for_model("claude-opus-4-7"), 1_000_000);
        assert_eq!(context_window_for_model("claude-opus-4-8"), 1_000_000);
        assert_eq!(context_window_for_model("claude-sonnet-4-6"), 1_000_000);
    }

}
