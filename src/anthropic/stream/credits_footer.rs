// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 会话结束时在回复末尾追加 credits 汇总行（opt-in，仅 Claude Code 客户端）
//!
//! 效果对齐 Kiro CLI 每轮结束后的统计行：
//!
//! ```text
//! ▸ Credits: turn 0.86 • session 8.73 | Time: 12s
//! ```
//!
//! - `turn`：从用户最近一次发送消息起到本轮回复结束（对应 Claude Code 的
//!   `✻ Brewed for Ns`），所有上游请求的 credits 之和，包括工具调用往返、期间子代理发起的请求，
//!   以及 Stop hook 拦截收尾后继续的部分
//! - `session`：同一 conversationId 下自开启以来所有请求的 credits 之和
//! - `Time`：本轮从第一个请求发起到当前响应结束的墙钟时间（含工具执行耗时）
//!
//! 上游 `meteringEvent` 只返回单次请求的 `usage`（实测同一会话连续请求也不累计），
//! 所以 turn / session 由本模块在代理侧累加，进程重启后清零。
//!
//! 追加的文本会被客户端存入对话历史并随后续请求回传，回传上游前由
//! `converter::history` 调用 [`strip_credits_footer`] 剥离，上游上下文不含该行
//! （同时保持历史逐字节稳定，不破坏 prompt cache）。

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::thinking_text::{ThinkingTextStyle, thinking_text_style};
use crate::anthropic::types::Message;

/// 汇总行正文前缀。同时是历史剥离的识别依据，修改需保持两侧一致。
const FOOTER_PREFIX: &str = "▸ Credits: turn ";
const FOOTER_SESSION_SEP: &str = " • session ";
const FOOTER_TIME_SEP: &str = " | Time: ";

/// ANSI：变暗开始 / 复位（只变暗不斜体，对齐 Kiro CLI 的灰色统计行）
const DIM_ON: &str = "\x1b[2m";
const DIM_OFF: &str = "\x1b[0m";
/// 思考文本化使用的「变暗 + 斜体」起始转义；历史剥离时也要认
const DIM_ITALIC_ON: &str = "\x1b[2;3m";

/// 登记表容量上限；超出时淘汰最久未活动的会话
const MAX_SESSIONS: usize = 2000;
/// 会话闲置超过此时长即可被淘汰
const SESSION_TTL: Duration = Duration::from_secs(24 * 3600);
/// 主对话 turn 超过此时长没有任何请求，就不再把陌生请求链当作子代理（用户中断后重新提问的兜底）
const TURN_IDLE_LIMIT: Duration = Duration::from_secs(600);

static SHOW_SESSION_CREDITS: AtomicBool = AtomicBool::new(false);

/// 启动时由 main 根据配置设置，Admin 设置页热切换（进程级，与 `set_thinking_as_text` 同模式）
pub fn set_show_session_credits(enabled: bool) {
    SHOW_SESSION_CREDITS.store(enabled, Ordering::Relaxed);
}

/// 配置是否开启了 credits 汇总行
pub fn show_session_credits_enabled() -> bool {
    SHOW_SESSION_CREDITS.load(Ordering::Relaxed)
}

/// 单个请求的汇总行参数（由 handler 在请求入口决定，传入 `StreamContext`）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CreditsFooterRequest {
    /// 渲染样式（终端用 ANSI 变暗，其余入口用无转义的 Markdown 斜体）
    pub style: ThinkingTextStyle,
    /// 本请求是否开启新的一轮（用户亲自发送的新消息；工具回传与 Claude Code 在同一轮内
    /// 注入的续请求都不算，见 [`is_new_user_turn`]）
    pub new_turn: bool,
    /// 请求链标识：首条消息文本的哈希。主对话与每个子代理的首条消息各不相同，据此区分
    pub chain: u64,
}

/// 判断本请求是否需要追加汇总行；不需要返回 `None`。
///
/// 条件：开关开启、客户端是 Claude Code、请求带工具定义。
/// 不带工具的请求是标题生成、对话压缩、建议预测等辅助调用，它们的输出会被客户端
/// 当结构化数据解析，追加文本会破坏结果。
pub fn credits_footer_request(
    headers: &axum::http::HeaderMap,
    messages: &[Message],
    has_tools: bool,
) -> Option<CreditsFooterRequest> {
    if !show_session_credits_enabled() || !has_tools {
        return None;
    }
    let style = thinking_text_style(headers)?;
    Some(CreditsFooterRequest {
        style,
        new_turn: is_new_user_turn(messages),
        chain: chain_key(messages),
    })
}

fn chain_key(messages: &[Message]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    match messages.first().map(|m| &m.content) {
        Some(serde_json::Value::String(s)) => s.hash(&mut hasher),
        Some(serde_json::Value::Array(blocks)) => {
            for text in blocks
                .iter()
                .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
            {
                text.hash(&mut hasher);
            }
        }
        _ => {}
    }
    hasher.finish()
}

/// Claude Code 在同一轮（一次 query 循环）内自行注入、并非用户输入的纯文本前缀。
///
/// `<system-reminder>`：附件与 hook 反馈的统一包装（Stop hook 拦截后的
/// `Stop hook blocking error ...` 也是这种形式）；`<task-notification>`：后台任务 /
/// asyncRewake hook 的通知。其余是 query 循环因输出被截断或工具调用异常发出的重试提示。
const INJECTED_TEXT_PREFIXES: [&str; 6] = [
    "<system-reminder>",
    "<task-notification>",
    "Output token limit hit.",
    "The previous response failed to produce a valid tool call.",
    "Your tool call was malformed and could not be parsed.",
    "[Your previous response had no visible output.",
];

fn is_injected_text(text: &str) -> bool {
    let text = text.trim_start();
    INJECTED_TEXT_PREFIXES.iter().any(|p| text.starts_with(p))
}

/// 文本是否含用户亲自输入的内容：剥离开头连续的完整 `<system-reminder>…</system-reminder>`
/// 附件块后，剩余部分仍非空且不是注入文案。
///
/// 提醒块与用户正文可能分成独立文本块，也可能合并在同一段文本里，两种形态都要识别。
/// 未闭合的提醒不剥离，整段仍按注入处理。
fn has_human_text(text: &str) -> bool {
    const OPEN: &str = "<system-reminder>";
    const CLOSE: &str = "</system-reminder>";
    let mut rest = text.trim_start();
    while let Some(after) = rest.strip_prefix(OPEN) {
        let Some(end) = after.find(CLOSE) else { break };
        rest = after[end + CLOSE.len()..].trim_start();
    }
    !rest.is_empty() && !is_injected_text(rest)
}

fn block_type(block: &serde_json::Value) -> Option<&str> {
    block.get("type").and_then(|t| t.as_str())
}

/// 最后一条消息是否是用户亲自发送的新一轮输入（对应 Claude Code 输出
/// `✻ Brewed for Ns` 的那个「轮」的起点）。
///
/// 一轮 = Claude Code 的一次 query 循环，循环内部会多次请求上游，且最后一条 user 消息不一定带
/// tool_result：Stop hook 拦截收尾后的反馈、后台任务通知、输出截断后的恢复提示，都是
/// 纯文本 user 消息，但它们仍属于同一轮。区分依据是内容：
/// - 带 tool_result：工具往返的续请求（旁边夹带的 queued 用户消息同样属于本轮）
/// - 文本块全部是 Claude Code 注入的内容（见 [`INJECTED_TEXT_PREFIXES`]）：循环内续请求
/// - 含用户亲自输入的文本（见 [`has_human_text`]），或图片 / 文档块：新一轮。提醒块与
///   用户正文可能是独立文本块，也可能合并成同一段文本，两种形态都按此判断
fn is_new_user_turn(messages: &[Message]) -> bool {
    let Some(last) = messages.last() else {
        return false;
    };
    if last.role != "user" {
        return false;
    }
    match &last.content {
        serde_json::Value::String(s) => has_human_text(s),
        serde_json::Value::Array(blocks) => {
            if blocks.iter().any(|b| block_type(b) == Some("tool_result")) {
                return false;
            }
            blocks.iter().any(|b| match block_type(b) {
                Some("text") => b
                    .get("text")
                    .and_then(|t| t.as_str())
                    .is_some_and(has_human_text),
                Some("image" | "document") => true,
                _ => false,
            })
        }
        _ => false,
    }
}

#[derive(Debug)]
struct SessionEntry {
    session_total: f64,
    turn_total: f64,
    turn_started: Instant,
    touched: Instant,
    /// 当前 turn 主对话的请求链标识
    main_chain: u64,
    /// 主对话的 turn 是否进行中（end_turn 后关闭）
    turn_open: bool,
    /// 当前 turn 内识别出的子代理请求链
    sub_chains: HashSet<u64>,
}

impl SessionEntry {
    fn new(now: Instant) -> Self {
        Self {
            session_total: 0.0,
            turn_total: 0.0,
            turn_started: now,
            touched: now,
            main_chain: 0,
            turn_open: false,
            sub_chains: HashSet::new(),
        }
    }

    /// 开启新的主对话 turn：从零累计、重新计时，并丢弃上一轮识别出的子代理链
    fn start_turn(&mut self, chain: u64, started_at: Instant) {
        self.main_chain = chain;
        self.sub_chains.clear();
        self.turn_total = 0.0;
        self.turn_started = started_at;
        self.turn_open = true;
    }
}

static SESSIONS: Mutex<Option<HashMap<String, SessionEntry>>> = Mutex::new(None);

/// 一次请求结束时的累计快照
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CreditsSnapshot {
    pub turn: f64,
    pub session: f64,
    pub elapsed: Duration,
}

/// 登记请求开始：新一轮（用户亲自发送的消息）清零 turn 累计并重置计时起点。
///
/// `started_at` 是请求发起时刻（含上游建连），新一轮以它作为 Time 的起点。
///
/// 返回本请求是否为子代理请求：主对话 turn 进行中出现的陌生请求链。子代理的 credits
/// 计入主对话当前 turn，但不清零 turn、不输出汇总行。
///
/// 主对话 end_turn 关闭 turn 后，同链又到达非用户输入的请求（Stop hook 拦截后的反馈等）
/// 视为同一轮的延续，沿用已有累计与计时。
pub fn begin_request(
    conversation_id: &str,
    chain: u64,
    new_turn: bool,
    started_at: Instant,
) -> bool {
    let mut guard = SESSIONS.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    evict_if_full(map);
    let entry = map
        .entry(conversation_id.to_string())
        .or_insert_with(|| SessionEntry::new(started_at));
    let now = Instant::now();
    let idle = now.saturating_duration_since(entry.touched) >= TURN_IDLE_LIMIT;
    let open = entry.turn_open && !idle;
    let is_sub = if new_turn {
        if open && chain != entry.main_chain {
            entry.sub_chains.insert(chain);
            true
        } else {
            entry.start_turn(chain, started_at);
            false
        }
    } else if entry.sub_chains.contains(&chain) {
        true
    } else {
        if !entry.turn_open {
            // turn 已被 end_turn 关闭，却又来了同链的非用户输入请求：Claude Code 的同一轮
            // 仍在继续（Stop hook 拦截收尾后的反馈、后台任务通知等），沿用已有的累计与计时，
            // 汇总行才能覆盖到 `Brewed for` 的整段。
            // 链不同、或闲置过久（会话恢复等）则不属于这一轮，按新 turn 从零累计。
            if chain == entry.main_chain && !idle {
                entry.turn_open = true;
            } else {
                entry.start_turn(chain, started_at);
            }
        }
        false
    };
    entry.touched = now;
    is_sub
}

/// 主对话以 end_turn 结束：关闭当前 turn，之后出现的陌生请求链不再被当作子代理
pub fn finish_turn(conversation_id: &str) {
    let mut guard = SESSIONS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(entry) = guard.as_mut().and_then(|m| m.get_mut(conversation_id)) {
        entry.turn_open = false;
    }
}

/// 登记请求结束并计入本次 credits，返回累计快照
pub fn record_credits(conversation_id: &str, credits: f64) -> CreditsSnapshot {
    let mut guard = SESSIONS.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    let now = Instant::now();
    let entry = map
        .entry(conversation_id.to_string())
        .or_insert_with(|| SessionEntry::new(now));
    entry.session_total += credits;
    entry.turn_total += credits;
    entry.touched = now;
    CreditsSnapshot {
        turn: entry.turn_total,
        session: entry.session_total,
        elapsed: now.saturating_duration_since(entry.turn_started),
    }
}

fn evict_if_full(map: &mut HashMap<String, SessionEntry>) {
    if map.len() < MAX_SESSIONS {
        return;
    }
    let now = Instant::now();
    map.retain(|_, e| now.saturating_duration_since(e.touched) < SESSION_TTL);
    // 仍然超限：丢弃最久未活动的一半
    if map.len() >= MAX_SESSIONS {
        let mut ages: Vec<(Instant, String)> =
            map.iter().map(|(k, e)| (e.touched, k.clone())).collect();
        ages.sort();
        for (_, key) in ages.into_iter().take(MAX_SESSIONS / 2) {
            map.remove(&key);
        }
    }
}

/// 时长格式：<60s 为 `Ns`，≥60s 为 `XmYs`（秒数向下取整）
fn format_elapsed(elapsed: Duration) -> String {
    let secs = elapsed.as_secs();
    if secs >= 60 {
        format!("{}m{}s", secs / 60, secs % 60)
    } else {
        format!("{secs}s")
    }
}

/// credits 数值格式：保留 3 位小数，末位是 0 时省略该 0（0.060 → 0.06，0.123 → 0.123，1.000 → 1.00）
fn format_credits(value: f64) -> String {
    let mut s = format!("{value:.3}");
    if s.ends_with('0') {
        s.pop();
    }
    s
}

/// 汇总行正文（不含样式包裹）
fn footer_body(snapshot: &CreditsSnapshot) -> String {
    format!(
        "{FOOTER_PREFIX}{}{FOOTER_SESSION_SEP}{}{FOOTER_TIME_SEP}{}",
        format_credits(snapshot.turn),
        format_credits(snapshot.session),
        format_elapsed(snapshot.elapsed)
    )
}

/// 渲染要追加到回复末尾的文本。
///
/// `visible_so_far` 是此前已输出的可见文本，用来补足换行，使汇总行与正文之间恰好隔一个空行。
pub fn render_footer(
    snapshot: &CreditsSnapshot,
    style: ThinkingTextStyle,
    visible_so_far: &str,
) -> String {
    let existing_newlines = visible_so_far
        .chars()
        .rev()
        .take_while(|c| *c == '\n')
        .count();
    let lead = "\n".repeat(2usize.saturating_sub(existing_newlines));
    let body = footer_body(snapshot);
    match style {
        ThinkingTextStyle::Ansi => format!("{lead}{DIM_ON}{body}{DIM_OFF}"),
        ThinkingTextStyle::Plain => format!("{lead}*{body}*"),
    }
}

/// 一行文本去掉样式包裹后是否是汇总行
fn is_footer_line(line: &str) -> bool {
    let plain = line
        .replace(DIM_ITALIC_ON, "")
        .replace(DIM_ON, "")
        .replace(DIM_OFF, "");
    let plain = plain.trim();
    let plain = plain
        .strip_prefix('*')
        .and_then(|s| s.strip_suffix('*'))
        .unwrap_or(plain);
    // 完整结构匹配：`<前缀><turn>< • session ><session>< | Time: ><时长>`，数值必须可解析，
    // 避免模型正文末行复述类似文字时被误剥
    let Some(rest) = plain.strip_prefix(FOOTER_PREFIX) else {
        return false;
    };
    let Some((turn, rest)) = rest.split_once(FOOTER_SESSION_SEP) else {
        return false;
    };
    let Some((session, time)) = rest.split_once(FOOTER_TIME_SEP) else {
        return false;
    };
    turn.parse::<f64>().is_ok() && session.parse::<f64>().is_ok() && is_elapsed(time)
}

/// `format_elapsed` 的输出：`Ns` 或 `XmYs`
fn is_elapsed(s: &str) -> bool {
    let digits = |x: &str| !x.is_empty() && x.bytes().all(|b| b.is_ascii_digit());
    let Some(body) = s.strip_suffix('s') else {
        return false;
    };
    match body.split_once('m') {
        Some((m, sec)) => digits(m) && digits(sec),
        None => digits(body),
    }
}

/// 历史 assistant 文本回传上游前剥离末尾的汇总行（含它前面的空行）。
/// 末行不是汇总行时原样返回，对普通文本幂等。
pub(crate) fn strip_credits_footer(text: &str) -> std::borrow::Cow<'_, str> {
    let trimmed = text.trim_end();
    let line_start = trimmed.rfind('\n').map_or(0, |i| i + 1);
    if !is_footer_line(&trimmed[line_start..]) {
        return std::borrow::Cow::Borrowed(text);
    }
    std::borrow::Cow::Owned(trimmed[..line_start].trim_end().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn snap(turn: f64, session: f64, secs: u64) -> CreditsSnapshot {
        CreditsSnapshot {
            turn,
            session,
            elapsed: Duration::from_secs(secs),
        }
    }

    fn msg(role: &str, content: serde_json::Value) -> Message {
        Message {
            role: role.to_string(),
            content,
        }
    }

    #[test]
    fn footer_matches_kiro_cli_format() {
        let body = footer_body(&snap(0.86, 8.73, 12));
        assert_eq!(body, "▸ Credits: turn 0.86 • session 8.73 | Time: 12s");
    }

    #[test]
    fn credits_use_three_decimals_and_drop_last_zero() {
        assert_eq!(format_credits(0.06), "0.06");
        assert_eq!(format_credits(0.0), "0.00");
        assert_eq!(format_credits(1.0), "1.00");
        assert_eq!(format_credits(0.123), "0.123");
        assert_eq!(format_credits(8.7), "8.70");
        assert_eq!(format_credits(0.1234), "0.123");
        // 四舍五入后末位为 0 同样省略
        assert_eq!(format_credits(0.0596), "0.06");
        assert_eq!(format_credits(12.3456), "12.346");
    }

    #[test]
    fn footer_with_three_decimals_is_still_stripped() {
        let text = format!(
            "答案{}",
            render_footer(&snap(0.123, 8.456, 5), ThinkingTextStyle::Plain, "答案")
        );
        assert!(text.contains("turn 0.123 • session 8.456"), "text={text:?}");
        assert_eq!(strip_credits_footer(&text), "答案");
    }

    #[test]
    fn elapsed_formats_minutes_and_floors_seconds() {
        assert_eq!(format_elapsed(Duration::from_millis(900)), "0s");
        assert_eq!(format_elapsed(Duration::from_millis(59_999)), "59s");
        assert_eq!(format_elapsed(Duration::from_secs(60)), "1m0s");
        assert_eq!(format_elapsed(Duration::from_secs(135)), "2m15s");
    }

    #[test]
    fn render_pads_to_exactly_one_blank_line() {
        let s = snap(1.0, 2.0, 3);
        let ansi = |prev: &str| render_footer(&s, ThinkingTextStyle::Ansi, prev);
        assert!(ansi("答案").starts_with("\n\n\x1b[2m▸ Credits"));
        assert!(ansi("答案\n").starts_with("\n\x1b[2m▸"));
        assert!(ansi("答案\n\n").starts_with("\x1b[2m▸"));
        assert!(ansi("答案\n\n\n").starts_with("\x1b[2m▸"));
        assert!(ansi("答案").ends_with("\x1b[0m"));
    }

    #[test]
    fn render_plain_uses_italic_without_escapes() {
        let out = render_footer(&snap(1.0, 2.0, 3), ThinkingTextStyle::Plain, "答案");
        assert_eq!(out, "\n\n*▸ Credits: turn 1.00 • session 2.00 | Time: 3s*");
        assert!(!out.contains('\x1b'));
    }

    #[test]
    fn strip_removes_footer_in_both_styles() {
        let s = snap(0.86, 8.73, 12);
        for style in [ThinkingTextStyle::Ansi, ThinkingTextStyle::Plain] {
            let text = format!(
                "最终答案\n第二行{}",
                render_footer(&s, style, "最终答案\n第二行")
            );
            assert_eq!(strip_credits_footer(&text), "最终答案\n第二行");
        }
    }

    #[test]
    fn strip_handles_footer_only_and_trailing_whitespace() {
        let line = "\x1b[2m▸ Credits: turn 1.00 • session 2.00 | Time: 3s\x1b[0m";
        assert_eq!(strip_credits_footer(line), "");
        assert_eq!(strip_credits_footer(&format!("答案\n\n{line}\n")), "答案");
    }

    #[test]
    fn strip_leaves_ordinary_text_untouched() {
        for t in [
            "",
            "普通文本",
            "提到 ▸ Credits: turn 但不是汇总行",
            // 汇总行出现在中间而不是末行：不动
            "▸ Credits: turn 1.00 • session 2.00 | Time: 3s\n后面还有正文",
            // 结构像但数值/时长不合法：不是汇总行
            "▸ Credits: turn x • session y | Time: z",
            "▸ Credits: turn 1.00 • session 2.00 | Time: soon",
            "结果如下\n▸ Credits: turn <n> • session <n> | Time: <t>",
        ] {
            assert_eq!(strip_credits_footer(t), t, "text={t:?}");
        }
    }

    #[test]
    fn new_turn_requires_human_input_in_last_user_message() {
        let text = |t: &str| json!({"type": "text", "text": t});
        let reminder = |t: &str| text(&format!("<system-reminder>\n{t}\n</system-reminder>"));
        let tool_result = json!({"type": "tool_result", "tool_use_id": "t1", "content": "ok"});

        // 用户亲自输入：纯字符串、文本块数组、附件块 + 提示文本、纯图片
        assert!(is_new_user_turn(&[msg("user", json!("帮我改一下"))]));
        assert!(is_new_user_turn(&[msg("user", json!([text("新需求")]))]));
        assert!(is_new_user_turn(&[msg(
            "user",
            json!([reminder("date changed"), text("新需求")])
        )]));
        assert!(is_new_user_turn(&[msg(
            "user",
            json!([{"type": "image", "source": {}}])
        )]));
        assert!(is_new_user_turn(&[msg(
            "user",
            json!([{"type": "document", "source": {}}])
        )]));
        // 提醒与用户正文合并成同一段文本（字符串或单个文本块）：仍是新一轮
        let merged = "<system-reminder>规则</system-reminder>\n<system-reminder>日期</system-reminder>\n请修复代码。";
        assert!(is_new_user_turn(&[msg("user", json!(merged))]));
        assert!(is_new_user_turn(&[msg("user", json!([text(merged)]))]));
        // 仅有提醒、没有正文（含未闭合的提醒）：Claude Code 注入的续请求
        for only_reminder in [
            "<system-reminder>规则</system-reminder>",
            "<system-reminder>规则</system-reminder>\n<system-reminder>日期</system-reminder>\n",
            "<system-reminder>未闭合提醒也不能当作用户输入",
        ] {
            assert!(!is_new_user_turn(&[msg("user", json!(only_reminder))]));
            assert!(!is_new_user_turn(&[msg(
                "user",
                json!([text(only_reminder)])
            )]));
        }
        // 提醒之后紧跟注入的重试提示：仍不是用户输入
        assert!(!is_new_user_turn(&[msg(
            "user",
            json!("<system-reminder>x</system-reminder>\nOutput token limit hit. Resume directly.")
        )]));

        // 带 tool_result：工具往返续请求，旁边夹带的 system-reminder / 用户排队消息都不改变结论
        assert!(!is_new_user_turn(&[msg(
            "user",
            json!([tool_result.clone()])
        )]));
        assert!(!is_new_user_turn(&[msg(
            "user",
            json!([tool_result.clone(), reminder("task tools")])
        )]));
        assert!(!is_new_user_turn(&[msg(
            "user",
            json!([tool_result, text("顺便再加个需求")])
        )]));

        // 同一轮内 Claude Code 注入的纯文本 user 消息：Stop hook 反馈、后台任务通知、重试提示
        for injected in [
            "<system-reminder>\nStop hook blocking error from command \"x\": 先问是否 CR\n</system-reminder>",
            "<task-notification>\n<summary>done</summary>\n</task-notification>",
            "Output token limit hit. Resume directly — no apology, no recap of what you were doing.",
            "Your tool call was malformed and could not be parsed. Please retry.",
            "[Your previous response had no visible output. Please continue and produce a user-visible response.]",
        ] {
            assert!(
                !is_new_user_turn(&[msg("user", json!([text(injected)]))]),
                "{injected}"
            );
            assert!(
                !is_new_user_turn(&[msg("user", json!(injected))]),
                "{injected}"
            );
        }

        // 空文本、非 user、空列表
        assert!(!is_new_user_turn(&[msg("user", json!([text("  ")]))]));
        assert!(!is_new_user_turn(&[msg(
            "assistant",
            json!([{"type": "text", "text": "x"}])
        )]));
        assert!(!is_new_user_turn(&[]));
    }

    #[test]
    fn turn_resets_on_new_turn_but_session_accumulates() {
        let conv = "test-conv-accumulate";
        let t0 = Instant::now();

        begin_request(conv, 1, true, t0);
        let a = record_credits(conv, 0.5);
        assert_eq!((a.turn, a.session), (0.5, 0.5));

        // 同一轮的续请求（工具回传）：turn 与 session 同步累加
        begin_request(conv, 1, false, Instant::now());
        let b = record_credits(conv, 0.25);
        assert_eq!((b.turn, b.session), (0.75, 0.75));

        // 新一轮：turn 清零，session 保留
        begin_request(conv, 1, true, Instant::now());
        let c = record_credits(conv, 1.0);
        assert_eq!((c.turn, c.session), (1.0, 1.75));
    }

    #[test]
    fn continuation_after_turn_closed_keeps_accumulating() {
        let conv = "test-conv-reopen";
        let started = Instant::now() - Duration::from_secs(5);
        begin_request(conv, 1, true, started);
        record_credits(conv, 0.5);
        finish_turn(conv);

        // end_turn 之后同链又来了非用户输入的请求（Stop hook 拦截后的反馈）：仍是同一轮，
        // turn 沿用已有累计与计时起点，汇总行覆盖到 `Brewed for` 的整段
        assert!(!begin_request(conv, 1, false, Instant::now()));
        let s = record_credits(conv, 0.25);
        assert_eq!((s.turn, s.session), (0.75, 0.75));
        assert!(s.elapsed >= Duration::from_secs(5));

        // 再收尾后用户发新消息：才清零
        finish_turn(conv);
        begin_request(conv, 1, true, Instant::now());
        let n = record_credits(conv, 1.0);
        assert_eq!((n.turn, n.session), (1.0, 1.75));
    }

    #[test]
    fn continuation_with_other_chain_after_turn_closed_starts_new_turn() {
        let conv = "test-conv-reopen-other-chain";
        begin_request(conv, 1, true, Instant::now());
        record_credits(conv, 0.5);
        finish_turn(conv);

        // 链不同（如会话恢复、首条消息被改写）：不属于刚结束的那一轮
        assert!(!begin_request(conv, 2, false, Instant::now()));
        let s = record_credits(conv, 0.25);
        assert_eq!((s.turn, s.session), (0.25, 0.75));
    }

    #[test]
    fn subagent_requests_merge_into_main_turn() {
        let conv = "test-conv-subagent";
        assert!(!begin_request(conv, 1, true, Instant::now()));
        record_credits(conv, 0.5);
        // 子代理首个请求：陌生请求链，不清零 turn
        assert!(begin_request(conv, 2, true, Instant::now()));
        record_credits(conv, 0.25);
        // 子代理工具回传
        assert!(begin_request(conv, 2, false, Instant::now()));
        record_credits(conv, 0.25);
        // 主对话续请求
        assert!(!begin_request(conv, 1, false, Instant::now()));
        let s = record_credits(conv, 1.0);
        assert_eq!((s.turn, s.session), (2.0, 2.0));

        // 主对话 end_turn 后，陌生请求链是新一轮
        finish_turn(conv);
        assert!(!begin_request(conv, 3, true, Instant::now()));
        let n = record_credits(conv, 0.5);
        assert_eq!((n.turn, n.session), (0.5, 2.5));
    }

    #[test]
    fn same_chain_new_turn_resets_when_previous_turn_never_ended() {
        let conv = "test-conv-interrupted";
        begin_request(conv, 1, true, Instant::now());
        record_credits(conv, 0.5);
        // 上一轮未 end_turn（用户中断后重新提问）：同一请求链的新消息仍清零 turn
        assert!(!begin_request(conv, 1, true, Instant::now()));
        let s = record_credits(conv, 0.25);
        assert_eq!((s.turn, s.session), (0.25, 0.75));
    }

    #[test]
    fn chain_key_follows_first_message_text() {
        let m = |t: &str| msg("user", json!(t));
        let a = chain_key(&[m("A"), m("后续1")]);
        assert_eq!(a, chain_key(&[m("A"), m("后续2")]));
        assert_ne!(a, chain_key(&[m("B")]));
    }

    #[test]
    fn sessions_are_isolated_by_conversation() {
        let t0 = Instant::now();
        begin_request("test-conv-iso-a", 1, true, t0);
        begin_request("test-conv-iso-b", 1, true, t0);
        record_credits("test-conv-iso-a", 2.0);
        let b = record_credits("test-conv-iso-b", 3.0);
        assert_eq!((b.turn, b.session), (3.0, 3.0));
    }

    #[test]
    fn new_turn_time_starts_at_request_start() {
        let conv = "test-conv-time";
        let started = Instant::now() - Duration::from_secs(7);
        begin_request(conv, 1, true, started);
        let s = record_credits(conv, 0.1);
        assert!(s.elapsed >= Duration::from_secs(7));
        assert!(s.elapsed < Duration::from_secs(10));
    }

    #[test]
    fn eviction_keeps_table_bounded() {
        let mut map: HashMap<String, SessionEntry> = HashMap::new();
        let now = Instant::now();
        for i in 0..MAX_SESSIONS {
            map.insert(format!("k{i}"), SessionEntry::new(now));
        }
        evict_if_full(&mut map);
        assert!(map.len() < MAX_SESSIONS);
    }

    #[test]
    fn request_disabled_without_switch_tools_or_claude_code() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            axum::http::header::USER_AGENT,
            "claude-cli/2.1.231 (external, cli)".parse().unwrap(),
        );
        let msgs = [msg("user", json!("hi"))];

        set_show_session_credits(false);
        assert!(credits_footer_request(&headers, &msgs, true).is_none());

        set_show_session_credits(true);
        // 无工具定义：辅助请求，不追加
        assert!(credits_footer_request(&headers, &msgs, false).is_none());
        // 非 Claude Code 客户端
        assert!(credits_footer_request(&axum::http::HeaderMap::new(), &msgs, true).is_none());
        let req = credits_footer_request(&headers, &msgs, true).unwrap();
        assert_eq!(req.style, ThinkingTextStyle::Ansi);
        assert!(req.new_turn);

        set_show_session_credits(false);
    }
}
