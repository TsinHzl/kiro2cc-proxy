//! 流式响应处理模块
//!
//! 实现 Kiro → Anthropic 流式响应转换和 SSE 状态管理
use std::collections::HashMap;

use serde_json::json;

use super::calib::{NEAR_EMPTY_OUTPUT_THRESHOLD, scale_for_client};

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
    pub(crate) fn is_block_open_of_type(&self, index: i32, expected_type: &str) -> bool {
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
    pub(crate) fn has_non_thinking_blocks(&self) -> bool {
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
        //
        // 计数必须在本循环内累加：循环结束后所有块都已置 stopped，事后再统计
        // "未闭合数"恒为 0，拿不到任何诊断信号。
        //
        // 只统计 tool_use 块 —— text 块没有 `handle_content_block_stop` 调用点，仅在
        // tool_use 开始时被 `handle_content_block_start` 批量关闭，其余情况一律由本循环
        // 终结；计入会让每个纯文本响应都被误判为异常。tool_use 块正常由 `process_tool_use`
        // 在收到 `stop=true` 时关闭，走到这里说明上游没发收尾帧，是真实异常。
        // 另注：上游中途断流 / deadline 截断时若有 tool_use 块未闭合，也会计入此处。
        let mut auto_closed_tool = 0;
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
                if block.block_type == "tool_use" {
                    auto_closed_tool += 1;
                }
            }
        }

        // 发送 message_delta
        if !self.message_delta_sent {
            self.message_delta_sent = true;

            // [TOOLUSE-DIAG] 响应收尾诊断：记录块构成，用于定位空响应类故障。
            //
            // 仅在命中结构异常时用 warn 上报，正常响应降为 debug —— 无条件 warn 会让
            // 每个响应都产生一条，淹没 Admin UI 的"近 1 小时警告"指标并挤占 ring buffer。
            //
            // 注：埋点最初针对的"上游把工具调用当纯文本输出"形态（has_tool_use=false
            // 且块构成正常）不在下方 warn 判据内，已随根因修复降为 debug —— 该根因是
            // 上游 429 被错映射成 502，已在别处修复，不再需要常态告警。
            {
                let mut text_n = 0;
                let mut thinking_n = 0;
                let mut tool_n = 0;
                for b in self.active_blocks.values() {
                    match b.block_type.as_str() {
                        "text" => text_n += 1,
                        "thinking" => thinking_n += 1,
                        "tool_use" => tool_n += 1,
                        _ => {}
                    }
                }

                // 无可见内容且输出极少：疑似退化空响应，会卡住客户端 agentic 循环。
                // thinking-only 响应不会误报 —— 上层补发过空格 text_delta（见
                // `StreamContext::generate_final_events`），执行在本诊断之前，text_n 为 1。
                //
                // 注：`output_tokens` 是可见输出口径（已排除 thinking），与
                // `NEAR_EMPTY_OUTPUT_THRESHOLD` 在 `is_empty_response` 中的计费口径不同。
                // 当前累加口径下 text_n/tool_n 双零已蕴含可见输出为 0，阈值项恒真，仅作
                // 防御 —— 若日后可见 token 改为不依赖块存在即累加，该项才会真正参与判定。
                let near_empty =
                    text_n == 0 && tool_n == 0 && output_tokens < NEAR_EMPTY_OUTPUT_THRESHOLD;

                let detail = format!(
                    "has_tool_use={} raw_stop_reason={:?} final_stop_reason={} \
                     out_tokens={} blocks(text={},thinking={},tool={},total={}) \
                     auto_closed_tool={}",
                    self.has_tool_use,
                    self.stop_reason,
                    self.get_stop_reason(),
                    output_tokens,
                    text_n,
                    thinking_n,
                    tool_n,
                    self.active_blocks.len(),
                    auto_closed_tool,
                );

                if auto_closed_tool > 0 || near_empty {
                    tracing::warn!(
                        "[TOOLUSE-DIAG] 结构异常 (auto_closed_tool={} near_empty={}) {}",
                        auto_closed_tool,
                        near_empty,
                        detail,
                    );
                } else {
                    tracing::debug!("[TOOLUSE-DIAG] {}", detail);
                }
            }

            let mut usage = serde_json::Map::new();
            // 客户端展示缩放（output_tokens 不缩放，避免影响 max_tokens 计算）
            usage.insert(
                "input_tokens".into(),
                json!(scale_for_client(input_tokens, model)),
            );
            usage.insert("output_tokens".into(), json!(output_tokens));
            if let Some(v) = cache_creation_input_tokens {
                usage.insert(
                    "cache_creation_input_tokens".into(),
                    json!(scale_for_client(v, model)),
                );
            }
            if let Some(v) = cache_read_input_tokens {
                usage.insert(
                    "cache_read_input_tokens".into(),
                    json!(scale_for_client(v, model)),
                );
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
