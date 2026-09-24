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

use super::calib::{cap_input_tokens, scale_for_client};
use super::helpers::count_token_chars;
use super::state::{SseEvent, SseStateManager};
use super::thinking::{
    find_char_boundary, find_real_thinking_end_tag, find_real_thinking_end_tag_at_buffer_end,
    find_real_thinking_start_tag,
};

mod metrics;
mod tool_use;

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
    /// 计费口径的输出字符累计（含 thinking），中文桶。见 `output_tokens()`
    pub output_chars_cn: i64,
    /// 计费口径的输出字符累计（含 thinking），非中文桶
    pub output_chars_other: i64,
    /// 上报口径的输出字符累计（不含 thinking），中文桶。见 `visible_output_tokens()`
    pub visible_chars_cn: i64,
    /// 上报口径的输出字符累计（不含 thinking），非中文桶
    pub visible_chars_other: i64,
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
    /// 请求的 effort 级别（output_config 存在时取值），随 usage 记录入库
    effort: Option<String>,
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
            output_chars_cn: 0,
            output_chars_other: 0,
            visible_chars_cn: 0,
            visible_chars_other: 0,
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
            effort: None,
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

    /// 设置 effort 级别（随 usage 记录入库）
    pub fn with_effort(mut self, effort: Option<String>) -> Self {
        self.effort = effort;
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

    /// 派生对客户端上报的 (non_cached_input, cache_creation, cache_read)。
    ///
    /// message_start 与末尾 message_delta 必须共用此逻辑：早前两者分别取
    /// `prompt_cache_usage` 随机模拟值与前缀估算，导致 message_start 报出
    /// cache_creation > 0 而 message_delta 报 0。客户端按字段取 max 记账，
    /// 于是每个请求都凭空多出一笔 cache write（实测 2.6k / 会话）。
    ///
    /// 优先级：
    ///   1. Kiro metering 透传 cache_read/creation（实测当前版本不透传，保留兜底；
    ///      message_start 阶段 metering 事件尚未到达，天然落到 2）
    ///   2. 前缀估算 prefix_estimated_tokens.min(input_tokens)  ← 主路径
    ///   3. PromptCacheUsage 模拟值（仅当前缀未注入）
    pub(crate) fn derive_report_cache_usage(&self, input_tokens: i32) -> (i32, i32, i32) {
        if let (Some(read), Some(creation)) = (
            self.metering_cache_read_tokens,
            self.metering_cache_creation_tokens,
        ) {
            let non_cached = input_tokens.saturating_sub(read).saturating_sub(creation);
            (non_cached, creation, read)
        } else if let Some(prefix) = self.prefix_estimated_tokens {
            let read = prefix.max(0).min(input_tokens);
            (input_tokens.saturating_sub(read), 0, read)
        } else {
            let sim = self.prompt_cache_usage.scale_to(input_tokens);
            (
                sim.input_tokens,
                sim.cache_creation_input_tokens,
                sim.cache_read_input_tokens,
            )
        }
    }

    /// 生成 message_start 事件
    pub fn create_message_start_event(&self) -> serde_json::Value {
        // 与末尾 message_delta 共用派生逻辑，避免 cache_* 口径不一致
        let input = cap_input_tokens(self.input_tokens, self.input_tokens, &self.model);
        let (non_cached, cache_creation, cache_read) = self.derive_report_cache_usage(input);
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
                    "input_tokens": scale_for_client(non_cached, &self.model),
                    "output_tokens": 1,
                    "cache_creation_input_tokens": scale_for_client(cache_creation, &self.model),
                    "cache_read_input_tokens": scale_for_client(cache_read, &self.model)
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
    pub(crate) fn process_assistant_response(&mut self, content: &str) -> Vec<SseEvent> {
        if content.is_empty() {
            return Vec::new();
        }

        // 累加字符数而非 token —— 取整统一留到 output_tokens() 收尾做一次
        let (cn, other) = count_token_chars(content);
        self.output_chars_cn += cn;
        self.output_chars_other += other;

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
            // 只统计真正发出的文本 —— 被状态机拒绝的 delta 客户端收不到，不应计入上报
            let (cn, other) = count_token_chars(text);
            self.visible_chars_cn += cn;
            self.visible_chars_other += other;
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
    pub(crate) fn process_tool_use(
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
            // tool input 是 JSON，整体归入非中文桶（与原 `(len+3)/4` 同口径，去掉逐次取整）
            let tool_input_chars = tool_use.input.len() as i64;
            // 计费口径与 text 分支一致（`process_assistant_response` 也是无条件累加）：
            // 上游已生成这段内容即已计费，代理侧状态机是否转发不改变上游成本。
            self.output_chars_other += tool_input_chars;

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
                // 上报口径只统计真正发出的内容 —— 被状态机拒绝的 delta 客户端收不到
                // （如同一 tool_use_id 在 stop 之后又收到迟到帧）。
                self.visible_chars_other += tool_input_chars;
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

        // 对客户端上报「可见输出」token（已排除 thinking），不再套用固定上限
        let reported_output_tokens = self.visible_output_tokens();
        // 计费口径（含 thinking），派生一次复用
        let billed_output_tokens = self.output_tokens();

        // 派生逻辑见 derive_report_cache_usage（与 message_start 共用同一口径）
        let (report_input, cache_creation, cache_read) =
            self.derive_report_cache_usage(final_input_tokens);
        let (report_cache_creation, report_cache_read) = (Some(cache_creation), Some(cache_read));

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
                let denom = final_input_tokens as f64 + 5.0 * billed_output_tokens as f64;
                if denom > 0.0 { c / denom * 1000.0 } else { 0.0 }
            });
            tracing::info!(
                "[usage] 入库: model={} input={} output={} metering_credits={:?} credits_per_ktok={:?} effective_rate={:?} cache_read={:?} cache_creation={:?} api_key={} credential={:?}",
                self.model,
                final_input_tokens,
                billed_output_tokens,
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
                billed_output_tokens,
                self.client_ip.clone(),
                self.metering_usage,
                report_cache_read,
                report_cache_creation,
                self.effort.clone(),
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
