// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 思考内容「文本化」渲染（opt-in，仅 Claude Code 客户端）
//!
//! Claude Code 携带 `redact-thinking` beta（或默认折叠 thinking）时，界面几乎看不到
//! 流式思考内容，长时间推理期间表现为「卡住」。开启 `thinkingAsText` 后，本模块把出站
//! SSE 中的 thinking 块改写为普通 text 块（逐行流式输出），使思考过程
//! 像 Kiro CLI 一样实时可见。
//!
//! 每行内容用 ANSI dim（变暗）转义包裹，接近 Kiro CLI 的灰色思考文字；
//! 是否生效取决于客户端是否放行转义字符。首行为「💭 Thinking」标记，
//! 块收尾追加「💭 Thought for Ns (N tokens)」时长行（贴近 Claude Code 原生样式，
//! token 数为该思考块正文的 cl100k_base 计数），
//! 不带 markdown 引用前缀（客户端不显示竖线）。
//!
//! 代价：这些文本块会被客户端当作 assistant 正文存入对话历史并随后续请求回传。
//! 回传时由 `converter::history::convert_assistant_message` 识别 [`THOUGHT_HEADER`]
//! 并剥离标记行，思考正文按普通助手文本保留回传（上下文略增，已确认接受）。

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::json;

use super::state::SseEvent;
use crate::token::count_tokens;

/// 文本化思考块的首行标记（不含换行符）。同时是历史剥离的识别依据，修改需保持两侧一致。
/// 不带 `> ` 引用前缀（客户端渲染时无竖线）；历史回传仅按此标记行识别并剥离。
pub(crate) const THOUGHT_HEADER: &str = "💭 Thinking";

/// 思考块收尾追加的时长行前缀（如「💭 Thought for 1m15s」）。秒数向下取整（截断），
/// 不足 1s 显示 "Thought for 1s"；≥60s 格式化为 "XmYs"，<60s 为 "Ns"；
/// 历史剥离按前缀识别（strip_rendered_thinking）。
pub(crate) const THOUGHT_DURATION_PREFIX: &str = "💭 Thought for ";

/// 时长行 token 统计后缀的起始标记（`" ("`）。生成侧与剥离侧共用，修改需保持两侧一致。
const THOUGHT_TOKEN_SUFFIX_OPEN: &str = " (";

/// 时长行 token 统计后缀的结束标记（`" tokens)"`），与 [`THOUGHT_TOKEN_SUFFIX_OPEN`] 配套。
const THOUGHT_TOKEN_SUFFIX_CLOSE: &str = " tokens)";

/// ANSI：变暗开始 / 复位（dim 样式，仅包在每行内容两侧，不跨行）
const DIM_ON: &str = "\x1b[2m";
const DIM_OFF: &str = "\x1b[0m";

static THINKING_AS_TEXT: AtomicBool = AtomicBool::new(false);

/// 启动时由 main 根据配置设置（进程级，与 `set_client_token_passthrough` 同模式）
pub fn set_thinking_as_text(enabled: bool) {
    THINKING_AS_TEXT.store(enabled, Ordering::Relaxed);
}

/// 配置是否开启了思考文本化
pub fn thinking_as_text_enabled() -> bool {
    THINKING_AS_TEXT.load(Ordering::Relaxed)
}

/// 单行文本是否为渲染出的思考标记行（兼容 dim 转义包裹）
fn is_rendered_header_line(line: &str) -> bool {
    line.replace(DIM_ON, "").replace(DIM_OFF, "") == THOUGHT_HEADER
}

/// 仅对 Claude Code 客户端生效：OpenAI 兼容端点等其他调用方需要保留原生 thinking 语义
pub fn is_claude_code_client(headers: &axum::http::HeaderMap) -> bool {
    headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ua| ua.starts_with("claude-cli") || ua.starts_with("claude-code"))
}

/// 剥离历史 text 块中由本模块渲染的思考**标记行**（思考正文保留回传上游）。
///
/// 仅当文本首行是 [`THOUGHT_HEADER`]（允许被 dim 转义包裹）时处理：移除该标记行，
/// 并移除流中追加的「💭 Thought for Ns (N tokens)」时长行（含其前置 `\n` 与 dim 包裹；
/// 该行可能与后续答案文本同段粘连，故按 前缀+时长+可选 token 后缀 模式定位后整体剥除；
/// 无 token 后缀的旧格式时长行同样兼容）。
/// 无标记时原样返回 —— 故对非文本化思考的普通文本是幂等的。
pub(crate) fn strip_rendered_thinking(text: &str) -> std::borrow::Cow<'_, str> {
    // 首行不是思考标记行：原样返回（普通文本幂等）
    let Some((_, rest)) = text
        .split_once('\n')
        .filter(|(first, _)| is_rendered_header_line(first))
    else {
        return std::borrow::Cow::Borrowed(text);
    };
    // 线性扫描时长行段：'\n' + [DIM_ON] + 前缀 + 数字 + 's' + [DIM_OFF]
    // 锚定规则：真实时长行恒带 DIM_OFF（此时允许行尾粘连后续答案文本，因客户端会把
    // 相邻 text 块合并拼接）；无 DIM_OFF 时必须命中行尾/文本结束才认定，避免误剥
    // 思考正文中恰好含「💭 Thought for 5s …」样式的普通句子。
    let mut i = 0;
    while let Some(nl) = rest[i..].find('\n') {
        let start = i + nl; // 时长行前置 \n 的位置（随行一并剥除）
        if let Some(nl2) = rest[start + 1..].find('\n') {
            // 仅对当前行匹配，行尾锚定以换行为界；span_end 停在时长行末尾，
            // 其后的粘连文本（旧格式时长行无尾 \n 时与答案同行）原样保留
            if let Some(span_end) = find_duration_span(&rest[start + 1..start + 1 + nl2]) {
                let end = start + 1 + span_end;
                let mut out = String::with_capacity(rest.len() - (end - start));
                out.push_str(&rest[..start]);
                out.push_str(&rest[end..]);
                return std::borrow::Cow::Owned(out);
            }
        } else if let Some(span_end) = find_duration_span(&rest[start + 1..]) {
            // 最后一行（无换行结尾）：剥除到时长行末尾，其后粘连文本保留
            let mut out = String::with_capacity(rest.len() - (start + 1 + span_end));
            out.push_str(&rest[..start]);
            out.push_str(&rest[start + 1 + span_end..]);
            return std::borrow::Cow::Owned(out);
        }
        i = start + 1;
    }
    std::borrow::Cow::Borrowed(rest)
}

/// 在单个行内容（不含前置 `\n`）中定位时长行段，返回段结束的相对字节偏移
/// （段起点恒为行首 0）。匹配 [DIM_ON] + 前缀 + 时长（`Ns` 或 `Nm Ys`/`NmYs`，
/// 与 format_duration 输出一致）+ 可选 token 后缀（`" (1.23k tokens)"`）+ [DIM_OFF]；
/// 命中 DIM_OFF 时段尾允许后随粘连文本（见 [`strip_rendered_thinking`] 锚定说明），
/// 否则必须到行尾才认定。token 后缀可选，兼容 v3.4.12 及更早的无后缀格式。
fn find_duration_span(line: &str) -> Option<usize> {
    let mut j = 0;
    if line.starts_with(DIM_ON) {
        j += DIM_ON.len();
    }
    if !line[j..].starts_with(THOUGHT_DURATION_PREFIX) {
        return None;
    }
    let after_prefix = j + THOUGHT_DURATION_PREFIX.len();
    // 时长段：可选分钟部分（`<数字>m`）+ 秒部分（`<数字>s`）
    let mut k = after_prefix;
    let mins = line[k..].bytes().take_while(u8::is_ascii_digit).count();
    if mins > 0 && line.as_bytes().get(k + mins) == Some(&b'm') {
        k += mins + 1;
    }
    let digits = line[k..].bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 || line.as_bytes().get(k + digits) != Some(&b's') {
        return None;
    }
    let mut end = k + digits + 1;
    // 可选 token 统计后缀「 (1.23k tokens)」：仅当计数段紧跟结束标记时认定。
    // 计数字符集覆盖 `999` / `1.23k` / `1.23M` 三种形态（见 format_token_count）。
    if let Some(after) = line[end..].strip_prefix(THOUGHT_TOKEN_SUFFIX_OPEN) {
        let count_len = after
            .bytes()
            .take_while(|b| b.is_ascii_digit() || matches!(b, b'.' | b'k' | b'M'))
            .count();
        if count_len > 0 && after[count_len..].starts_with(THOUGHT_TOKEN_SUFFIX_CLOSE) {
            end += THOUGHT_TOKEN_SUFFIX_OPEN.len() + count_len + THOUGHT_TOKEN_SUFFIX_CLOSE.len();
        }
    }
    let has_dim_off = line[end..].starts_with(DIM_OFF);
    if has_dim_off {
        end += DIM_OFF.len();
    }
    if has_dim_off || end == line.len() {
        Some(end)
    } else {
        None
    }
}

/// token 数紧凑格式化（`1234` → `1.23k`，`1234567` → `1.23M`）。
/// 精度随量级递减：值 <10 保留两位小数、<100 保留一位、≥100 取整。
/// `999_500` 是 k 档取整后的进位边界（`999_500 / 1000 = 999.5` 会舍入为 `1000k`），
/// 达到该值即改用 M 档，避免出现 `1000k` 这类进位残留。
fn format_token_count(n: u64) -> String {
    if n < 1_000 {
        return n.to_string();
    }
    let (unit, scale) = if n < 999_500 {
        ("k", 1_000u64)
    } else {
        ("M", 1_000_000u64)
    };
    let value = n as f64 / scale as f64;
    let decimals = if value < 10.0 {
        2
    } else if value < 100.0 {
        1
    } else {
        0
    };
    format!("{value:.decimals$}{unit}")
}

/// 把 thinking 块事件改写为 text 块事件的有状态转换器
#[derive(Debug, Default)]
pub struct ThinkingTextRewriter {
    /// 已被改写为 text 的块索引
    indices: HashSet<i32>,
    /// 已输出首行标记的块
    header_sent: HashSet<i32>,
    /// 各块当前是否位于行首（决定是否补 ANSI dim 起始转义，避免样式跨行）
    at_line_start: HashMap<i32, bool>,
    /// 各思考块的起始时刻（用于收尾计算 Thought for Ns）
    started_at: HashMap<i32, std::time::Instant>,
    /// 各思考块累计的思考正文原文（用于收尾统计 token 数；不含标记行与 ANSI 转义）
    thinking_text: HashMap<i32, String>,
}

impl ThinkingTextRewriter {
    pub fn new() -> Self {
        Self::default()
    }

    /// 改写一批事件；非 thinking 块事件原样透传
    pub fn rewrite(&mut self, events: Vec<SseEvent>) -> Vec<SseEvent> {
        let mut out = Vec::with_capacity(events.len());
        for event in events {
            let index = event
                .data
                .get("index")
                .and_then(|v| v.as_i64())
                .map(|i| i as i32);
            match (event.event.as_str(), index) {
                ("content_block_start", Some(i))
                    if event.data["content_block"]["type"] == "thinking" =>
                {
                    self.indices.insert(i);
                    self.at_line_start.insert(i, true);
                    self.started_at
                        .entry(i)
                        .or_insert_with(std::time::Instant::now);
                    out.push(SseEvent::new(
                        "content_block_start",
                        json!({
                            "type": "content_block_start",
                            "index": i,
                            "content_block": {"type": "text", "text": ""}
                        }),
                    ));
                }
                ("content_block_delta", Some(i)) if self.indices.contains(&i) => {
                    match event.data["delta"]["type"].as_str() {
                        Some("thinking_delta") => {}
                        // signature_delta 对 text 块无意义，丢弃
                        Some("signature_delta") => continue,
                        // 已是改写后的 text_delta 等：原样透传（幂等）
                        _ => {
                            out.push(event);
                            continue;
                        }
                    }
                    let text = event.data["delta"]["thinking"].as_str().unwrap_or("");
                    if text.is_empty() {
                        continue;
                    }
                    // 累计思考正文原文，供收尾统计 token（不含标记行与 ANSI 转义）
                    self.thinking_text.entry(i).or_default().push_str(text);
                    let mut rendered = String::new();
                    if self.header_sent.insert(i) {
                        // 首行：dim(💭 Thinking)，即 THOUGHT_HEADER + 换行
                        rendered.push_str(DIM_ON);
                        rendered.push_str(THOUGHT_HEADER);
                        rendered.push_str(DIM_OFF);
                        rendered.push('\n');
                    }
                    let at_start = self.at_line_start.entry(i).or_insert(true);
                    for ch in text.chars() {
                        if *at_start {
                            rendered.push_str(DIM_ON);
                            *at_start = false;
                        }
                        if ch == '\n' {
                            // 换行前复位，样式不跨行
                            rendered.push_str(DIM_OFF);
                            rendered.push('\n');
                            *at_start = true;
                        } else {
                            rendered.push(ch);
                        }
                    }
                    out.push(SseEvent::new(
                        "content_block_delta",
                        json!({
                            "type": "content_block_delta",
                            "index": i,
                            "delta": {"type": "text_delta", "text": rendered}
                        }),
                    ));
                }
                ("content_block_stop", Some(i)) if self.indices.remove(&i) => {
                    // header_sent 的存在代表该块输出过标记行（即有实际思考文本）
                    let had_output = self.header_sent.remove(&i);
                    // 无论是否追加时长行都要取出并清理累计正文，避免空块残留占用内存
                    let thinking_text = self.thinking_text.remove(&i).unwrap_or_default();
                    // 块在行中间结束时补一个复位，避免 dim 样式泄漏到后续正文
                    if self.at_line_start.remove(&i) == Some(false) {
                        out.push(SseEvent::new(
                            "content_block_delta",
                            json!({
                                "type": "content_block_delta",
                                "index": i,
                                "delta": {"type": "text_delta", "text": DIM_OFF}
                            }),
                        ));
                    }
                    // 收尾追加「💭 Thought for Ns (N tokens)」时长行；空思考块（无任何输出）不追加。
                    // 前后各补 \n：思考正文可能不以换行结尾、答案文本块会与时长行直接拼接
                    // （客户端合并相邻 text 块），先换行确保时长行独立成行，尾换行让答案另起一行。
                    if let (Some(started), true) = (self.started_at.remove(&i), had_output) {
                        let secs = started.elapsed().as_secs().max(1);
                        // ≥60s 格式化为 "XmYs"（如 1m15s），<60s 为 "Ns"
                        let duration = if secs >= 60 {
                            format!("{}m{}s", secs / 60, secs % 60)
                        } else {
                            format!("{secs}s")
                        };
                        // token 数按思考正文原文统计（cl100k_base 原始值，不乘展示缩放系数）。
                        // 同步 BPE 编码在此处每思考块仅执行一次，且 cl100k_base_singleton 已全局
                        // 缓存词表；开销低于请求路径上的 cache/fingerprint.rs 逐段计数，无需异步化。
                        let token_suffix = format!(
                            "{}{}{}",
                            THOUGHT_TOKEN_SUFFIX_OPEN,
                            format_token_count(count_tokens(&thinking_text)),
                            THOUGHT_TOKEN_SUFFIX_CLOSE
                        );
                        out.push(SseEvent::new(
                            "content_block_delta",
                            json!({
                                "type": "content_block_delta",
                                "index": i,
                                "delta": {"type": "text_delta", "text": format!(
                                    "\n{}{}{}{}{}\n",
                                    DIM_ON, THOUGHT_DURATION_PREFIX, duration, token_suffix, DIM_OFF
                                )}
                            }),
                        ));
                    }
                    out.push(event);
                }
                _ => out.push(event),
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start(i: i32, ty: &str) -> SseEvent {
        SseEvent::new(
            "content_block_start",
            json!({"type":"content_block_start","index":i,"content_block":{"type":ty}}),
        )
    }
    fn delta(i: i32, kind: &str, field: &str, v: &str) -> SseEvent {
        SseEvent::new(
            "content_block_delta",
            json!({"type":"content_block_delta","index":i,"delta":{"type":kind,(field):v}}),
        )
    }
    fn stop(i: i32) -> SseEvent {
        SseEvent::new(
            "content_block_stop",
            json!({"type":"content_block_stop","index":i}),
        )
    }

    /// 去掉 dim 转义，便于断言可见文本
    fn plain(s: &str) -> String {
        s.replace(DIM_ON, "").replace(DIM_OFF, "")
    }

    fn text_of(events: &[SseEvent]) -> String {
        events
            .iter()
            .filter_map(|e| e.data["delta"]["text"].as_str())
            .collect()
    }

    #[test]
    fn thinking_block_becomes_quoted_text_block() {
        let mut r = ThinkingTextRewriter::new();
        let mut out = r.rewrite(vec![start(0, "thinking")]);
        out.extend(r.rewrite(vec![
            delta(0, "thinking_delta", "thinking", "first line\nsec"),
            delta(0, "thinking_delta", "thinking", "ond\n\nthird"),
            delta(0, "signature_delta", "signature", "SIG"),
            stop(0),
        ]));
        assert_eq!(out[0].data["content_block"]["type"], "text");
        // 时长行带 token 统计后缀，数值随正文长度变化，故只断言形态
        let rendered = plain(&text_of(&out));
        assert!(
            rendered.starts_with("💭 Thinking\nfirst line\nsecond\n\nthird\n💭 Thought for 1s ("),
            "时长行前缀不符: {rendered}"
        );
        assert!(
            rendered.ends_with(" tokens)\n"),
            "时长行后缀不符: {rendered}"
        );
        assert!(
            out.iter()
                .all(|e| e.data["delta"]["type"] != "thinking_delta"
                    && e.data["delta"]["type"] != "signature_delta"),
            "不得残留 thinking/signature delta"
        );
        assert_eq!(out.last().unwrap().event, "content_block_stop");
    }

    #[test]
    fn non_thinking_blocks_pass_through_untouched() {
        let mut r = ThinkingTextRewriter::new();
        let input = vec![
            start(1, "text"),
            delta(1, "text_delta", "text", "hi"),
            stop(1),
            start(2, "tool_use"),
            delta(2, "input_json_delta", "partial_json", "{}"),
        ];
        let out = r.rewrite(input.clone());
        assert_eq!(out.len(), input.len());
        for (a, b) in out.iter().zip(input.iter()) {
            assert_eq!(a.data, b.data);
        }
    }

    #[test]
    fn empty_thinking_emits_no_header() {
        let mut r = ThinkingTextRewriter::new();
        let out = r.rewrite(vec![
            start(0, "thinking"),
            delta(0, "thinking_delta", "thinking", ""),
            delta(0, "signature_delta", "signature", "SIG"),
            stop(0),
        ]);
        assert_eq!(out.len(), 2, "仅剩 start 与 stop");
        assert_eq!(text_of(&out), "");
    }

    #[test]
    fn strip_removes_only_header_and_keeps_thinking_body() {
        // 仅剥「💭 Thinking」标记行与尾行时长标记，思考正文保留（作为普通助手文本回传上游）
        let rendered = "💭 Thinking\na\n\nb\n";
        assert_eq!(strip_rendered_thinking(rendered), "a\n\nb\n");
        // 带尾行「💭 Thought for Ns」时同样剥离，正文保留（尾随换行来自时长行后的换行）
        let with_duration = "💭 Thinking\na\n\nb\n💭 Thought for 12s\n";
        assert_eq!(strip_rendered_thinking(with_duration), "a\n\nb\n");
        // 带 token 统计后缀的新格式：整行（含后缀）一并剥除
        let with_tokens = "💭 Thinking\na\n\nb\n💭 Thought for 12s (1.23k tokens)\n";
        assert_eq!(strip_rendered_thinking(with_tokens), "a\n\nb\n");
        // 带 token 后缀且与答案同行粘连（有 DIM_OFF）：仅剥到 DIM_OFF，答案保留
        let with_tokens_merged = "💭 Thinking\na\n\n💭 Thought for 12s (999 tokens)\x1b[0manswer";
        assert_eq!(strip_rendered_thinking(with_tokens_merged), "a\nanswer");
        // M 档后缀同样可剥
        let with_mega = "💭 Thinking\na\n\n💭 Thought for 12s (1.23M tokens)\n";
        assert_eq!(strip_rendered_thinking(with_mega), "a\n\n");
        // 客户端把相邻 text 块合并的情形（时长行后粘连答案）；含新格式 XmYs
        let merged = "💭 Thinking\na\n\nb\n💭 Thought for 12s\nanswer";
        assert_eq!(strip_rendered_thinking(merged), "a\n\nb\nanswer");
        // ≥60s 的新格式「XmYs」同样可剥
        let with_minutes = "💭 Thinking\na\n\n💭 Thought for 1m15s\nanswer";
        assert_eq!(strip_rendered_thinking(with_minutes), "a\n\nanswer");
        // 旧格式时长行与答案同行粘连且带 DIM_OFF（时长 delta 自带前置 \n）：
        // 仅剥到 DIM_OFF，答案保留
        let legacy = "💭 Thinking\na\n\n💭 Thought for 12s\x1b[0manswer";
        assert_eq!(strip_rendered_thinking(legacy), "a\nanswer");
        // 行首即前缀但行尾锚定失败（无 DIM_OFF 且行尾有余文）：不剥。
        // 已知取舍：客户端剥离全部 ANSI 后与答案同行粘连的旧格式时长行同样不剥
        //（历史多一行噪声，但不丢答案正文）
        let anchored = "💭 Thinking\n💭 Thought for 5s per step\n答案";
        assert_eq!(
            strip_rendered_thinking(anchored),
            "💭 Thought for 5s per step\n答案"
        );
        // 普通文本（含普通引用）不受影响
        assert_eq!(strip_rendered_thinking("> quote\nx"), "> quote\nx");
        assert_eq!(strip_rendered_thinking("plain"), "plain");
        // 思考正文恰含相似句子但不带 DIM_OFF、未到行尾：不被误剥（首行标记正常剥除）
        let lookalike = "💭 Thinking\n需 💭 Thought for 5s per step\n答案";
        assert_eq!(
            strip_rendered_thinking(lookalike),
            "需 💭 Thought for 5s per step\n答案"
        );
    }

    #[test]
    fn rendered_thinking_body_survives_history_strip() {
        let mut r = ThinkingTextRewriter::new();
        let mut out = r.rewrite(vec![start(0, "thinking")]);
        out.extend(r.rewrite(vec![
            delta(0, "thinking_delta", "thinking", "想一想\n\n再想想\n"),
            stop(0),
        ]));
        let rendered = plain(&text_of(&out));
        let stripped = strip_rendered_thinking(&rendered);
        // 尾行时长标记被剥掉，思考正文（含尾部换行与中间空行）原样保留；
        // 正文本身以 \n 结尾，时长行剥除后留一个空行，无害
        assert_eq!(stripped, "想一想\n\n再想想\n\n");
        assert!(stripped.ends_with('\n'));
        assert_eq!(stripped.lines().count(), 4);
    }

    #[test]
    fn stream_context_renders_native_reasoning_as_text_blocks() {
        use crate::anthropic::stream::StreamContext;
        use crate::kiro::model::events::{AssistantResponseEvent, Event, ReasoningContentEvent};

        let reasoning = |text: &str, sig: &str| {
            Event::ReasoningContent(
                serde_json::from_value::<ReasoningContentEvent>(
                    json!({"text": text, "signature": sig}),
                )
                .unwrap(),
            )
        };
        let answer = Event::AssistantResponse(
            serde_json::from_value::<AssistantResponseEvent>(json!({"content": "最终答案"}))
                .unwrap(),
        );

        for enabled in [false, true] {
            let mut ctx = StreamContext::new_with_thinking("claude-sonnet-4-6", 100, true)
                .with_thinking_as_text(enabled);
            let mut events = ctx.generate_initial_events();
            events.extend(ctx.process_kiro_event(&reasoning("第一行\n第二", "")));
            events.extend(ctx.process_kiro_event(&reasoning("行", "SIG")));
            events.extend(ctx.process_kiro_event(&answer));
            events.extend(ctx.generate_final_events());

            let has_thinking = events.iter().any(|e| {
                e.data["content_block"]["type"] == "thinking"
                    || e.data["delta"]["type"] == "thinking_delta"
                    || e.data["delta"]["type"] == "signature_delta"
            });
            let all_text: String = events
                .iter()
                .filter_map(|e| e.data["delta"]["text"].as_str())
                .collect();
            if enabled {
                assert!(!has_thinking, "开启后不应再出现 thinking 块/delta");
                // 历史剥离后时长行不回传，答案与思考正文间保留时长行位置的换行（不粘连）
                let rendered = plain(&all_text);
                assert_eq!(
                    strip_rendered_thinking(&rendered),
                    "第一行\n第二行\n最终答案"
                );
            } else {
                assert!(has_thinking, "关闭时保持原生 thinking 块");
                assert_eq!(all_text, "最终答案");
            }
        }
    }

    #[test]
    fn dim_style_wraps_each_line_and_resets_at_block_end() {
        let mut r = ThinkingTextRewriter::new();
        let mut out = r.rewrite(vec![start(0, "thinking")]);
        out.extend(r.rewrite(vec![
            delta(0, "thinking_delta", "thinking", "ab\n\ncd"),
            stop(0),
        ]));
        let text = text_of(&out);
        // 尾行为时长行（前置 \n + dim 包裹 + token 统计后缀）
        assert!(
            text.contains(&format!("\n{DIM_ON}{THOUGHT_DURATION_PREFIX}1s (")),
            "时长行前缀不符: {text:?}"
        );
        assert!(
            text.ends_with(&format!(" tokens){DIM_OFF}\n")),
            "时长行后缀不符: {text:?}"
        );
        // 行中结束 → 末尾补复位；stop 事件仍在最后
        assert_eq!(out.last().unwrap().event, "content_block_stop");
        // dim 渲染的内容同样能被历史剥离识别（仅剥标记行与时长行，正文保留；
        // 正文 "cd" 后残留时长行原本所在的空行，无害）
        let stripped = strip_rendered_thinking(&text);
        assert_eq!(plain(&stripped), "ab\n\ncd\n");
        assert_eq!(
            strip_rendered_thinking(&format!("{text}answer")),
            format!("{stripped}answer")
        );
    }

    #[test]
    fn format_token_count_tiers() {
        assert_eq!(format_token_count(0), "0");
        assert_eq!(format_token_count(999), "999");
        assert_eq!(format_token_count(1_234), "1.23k");
        assert_eq!(format_token_count(10_000), "10.0k");
        assert_eq!(format_token_count(123_456), "123k");
        assert_eq!(format_token_count(999_499), "999k");
        // 进位提升：999_500/1000 = 999.5 会舍入为 1000k，故改用 M 档
        assert_eq!(format_token_count(999_500), "1.00M");
        assert_eq!(format_token_count(1_234_567), "1.23M");
    }

    #[test]
    fn claude_code_user_agent_detection() {
        let mut h = axum::http::HeaderMap::new();
        assert!(!is_claude_code_client(&h));
        h.insert(
            axum::http::header::USER_AGENT,
            "claude-cli/2.1.231 (external, cli)".parse().unwrap(),
        );
        assert!(is_claude_code_client(&h));
        h.insert(axum::http::header::USER_AGENT, "codex/1.0".parse().unwrap());
        assert!(!is_claude_code_client(&h));
    }
}
