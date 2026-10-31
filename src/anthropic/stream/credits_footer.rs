// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 会话结束时在回复末尾追加 credits 汇总行（opt-in，仅 Claude Code 客户端）
//!
//! 效果对齐 Kiro CLI 每轮结束后的统计行：
//!
//! ```text
//! ▸ Credits: turn 0.86 • session 8.73 | Time: 12s
//! ```
//!
//! - `turn`：从用户最近一次发送消息起到本轮回复结束，所有上游请求（含工具调用往返、
//!   期间子代理发起的请求）的 credits 之和
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
    /// 本请求是否开启新的一轮（即不是工具结果回传的续轮）
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
        new_turn: !is_tool_result_continuation(messages),
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

/// 最后一条消息是否携带工具结果（同一轮内的续请求）。
///
/// Claude Code 会在 tool_result 旁边夹带 `<system-reminder>` 等文本块，所以只要出现
/// tool_result 就算续轮；纯文本的 user 消息才是用户的新一轮输入。
fn is_tool_result_continuation(messages: &[Message]) -> bool {
    let Some(last) = messages.last() else {
        return false;
    };
    last.role == "user"
        && last.content.as_array().is_some_and(|blocks| {
            blocks
                .iter()
                .any(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_result"))
        })
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
}

static SESSIONS: Mutex<Option<HashMap<String, SessionEntry>>> = Mutex::new(None);

/// 一次请求结束时的累计快照
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CreditsSnapshot {
    pub turn: f64,
    pub session: f64,
    pub elapsed: Duration,
}

/// 登记请求开始：新一轮则清零 turn 累计并重置计时起点。
///
/// `started_at` 是请求发起时刻（含上游建连），新一轮以它作为 Time 的起点。
///
/// 返回本请求是否为子代理请求：主对话 turn 进行中出现的陌生请求链。子代理的 credits
/// 计入主对话当前 turn，但不清零 turn、不输出汇总行。
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
    let open = entry.turn_open && now.saturating_duration_since(entry.touched) < TURN_IDLE_LIMIT;
    let is_sub = if new_turn {
        if open && chain != entry.main_chain {
            entry.sub_chains.insert(chain);
            true
        } else {
            entry.main_chain = chain;
            entry.sub_chains.clear();
            entry.turn_total = 0.0;
            entry.turn_started = started_at;
            entry.turn_open = true;
            false
        }
    } else {
        entry.sub_chains.contains(&chain)
    };
    if !is_sub && !entry.turn_open {
        entry.main_chain = chain;
        entry.turn_open = true;
    }
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

/// 汇总行正文（不含样式包裹）
fn footer_body(snapshot: &CreditsSnapshot) -> String {
    format!(
        "{FOOTER_PREFIX}{:.2}{FOOTER_SESSION_SEP}{:.2}{FOOTER_TIME_SEP}{}",
        snapshot.turn,
        snapshot.session,
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
    fn message_with_tool_result_is_continuation() {
        let cont = [msg(
            "user",
            json!([{"type": "tool_result", "tool_use_id": "t1", "content": "ok"}]),
        )];
        assert!(is_tool_result_continuation(&cont));

        let fresh_text = [msg("user", json!("帮我改一下"))];
        assert!(!is_tool_result_continuation(&fresh_text));

        // tool_result 旁夹带 system-reminder 文本块：仍是续轮
        let with_reminder = [msg(
            "user",
            json!([
                {"type": "tool_result", "tool_use_id": "t1", "content": "ok"},
                {"type": "text", "text": "<system-reminder>...</system-reminder>"}
            ]),
        )];
        assert!(is_tool_result_continuation(&with_reminder));

        // 纯文本块数组：用户新输入
        let text_blocks = [msg("user", json!([{"type": "text", "text": "新需求"}]))];
        assert!(!is_tool_result_continuation(&text_blocks));

        let assistant_last = [msg("assistant", json!([{"type": "text", "text": "x"}]))];
        assert!(!is_tool_result_continuation(&assistant_last));
        assert!(!is_tool_result_continuation(&[]));
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
