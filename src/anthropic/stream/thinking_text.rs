// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 思考内容「文本化」渲染（opt-in，仅 Claude Code 客户端）
//!
//! Claude Code 携带 `redact-thinking` beta（或默认折叠 thinking）时，界面几乎看不到
//! 流式思考内容，长时间推理期间表现为「卡住」。开启 `thinkingAsText` 后，本模块把出站
//! SSE 中的 thinking 块改写为普通 text 块（逐行流式输出），使思考过程
//! 像 Kiro CLI 一样实时可见。
//!
//! 每行内容用 ANSI dim + italic（变暗斜体，`ESC[2;3m`）转义包裹，接近 Kiro CLI 的灰色思考文字；
//! 是否生效取决于客户端是否放行转义字符：只有确认是终端渲染的入口（UA 入口为 `cli`，
//! 或解析不出入口）才用 ANSI，其余入口一律改用无转义的 Markdown 样式（每行 `> *内容*`，引用块 + 斜体，
//! 用 Markdown 自带样式近似变暗）
//! （见 [`thinking_text_style`]）。首行为「💭 Thinking」标记，
//! 块收尾追加「💭 Thought for Ns (N tokens)」时长行（贴近 Claude Code 原生样式，
//! token 数为该思考块正文的 cl100k_base 计数），
//! ANSI 样式不带 markdown 引用前缀（终端不显示竖线）。
//!
//! 代价：这些文本块会被客户端当作 assistant 正文存入对话历史并随后续请求回传
//! （客户端侧上下文因此包含思考文本）。回传上游时由
//! `converter::history::convert_assistant_message` 识别 [`THOUGHT_HEADER`]，
//! 整块剥离「标记行 + 思考正文 + 时长行」，上游上下文不含思考内容
//! （与关闭文本化时原生 thinking 块整块丢弃的语义一致）。

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

/// ANSI：变暗斜体开始 / 复位（dim + italic，仅包在每行内容两侧，不跨行）
const DIM_ON: &str = "\x1b[2;3m";
const DIM_OFF: &str = "\x1b[0m";
/// 旧版本只变暗不斜体的起始转义。客户端历史里可能还留着旧版本渲染的文本，
/// 剥离 / 识别侧必须同时认 [`DIM_ON`] 与它；生成侧只用 [`DIM_ON`]。
const LEGACY_DIM_ON: &str = "\x1b[2m";

/// 去掉所有起始转义（新旧两种）与复位转义，得到可见文本
fn remove_ansi_style(text: &str) -> String {
    text.replace(DIM_ON, "")
        .replace(LEGACY_DIM_ON, "")
        .replace(DIM_OFF, "")
}

/// Plain 样式下每行的起始 / 结束标记：引用块 + 斜体（`> *内容*`），
/// 用 Markdown 自带的样式近似 ANSI 的「变暗」。生成侧与剥离侧共用，修改需保持两侧一致。
const QUOTE_ITALIC_OPEN: &str = "> *";
const QUOTE_ITALIC_CLOSE: &str = "*";

static THINKING_AS_TEXT: AtomicBool = AtomicBool::new(false);

/// 启动时由 main 根据配置设置（进程级，与 `set_client_token_passthrough` 同模式）
pub fn set_thinking_as_text(enabled: bool) {
    THINKING_AS_TEXT.store(enabled, Ordering::Relaxed);
}

/// 配置是否开启了思考文本化
pub fn thinking_as_text_enabled() -> bool {
    THINKING_AS_TEXT.load(Ordering::Relaxed)
}

static THINKING_COLLAPSE: AtomicBool = AtomicBool::new(false);

/// 启动时由 main 根据配置设置，Admin 设置页热切换（进程级，与 [`set_thinking_as_text`] 同模式）
pub fn set_thinking_collapse(enabled: bool) {
    THINKING_COLLAPSE.store(enabled, Ordering::Relaxed);
}

/// 配置是否开启了思考块结束后折叠为原生 thinking 块（见 [`ThinkingTextRewriter::with_collapse`]）
pub fn thinking_collapse_enabled() -> bool {
    THINKING_COLLAPSE.load(Ordering::Relaxed)
}

/// 单行文本是否为渲染出的思考标记行（兼容 dim 转义包裹，以及 Plain 的引用 + 斜体包裹）
fn is_rendered_header_line(line: &str) -> bool {
    let line = remove_ansi_style(line);
    line == THOUGHT_HEADER
        || line
            .strip_prefix(QUOTE_ITALIC_OPEN)
            .and_then(|l| l.strip_suffix(QUOTE_ITALIC_CLOSE))
            == Some(THOUGHT_HEADER)
}

/// 仅对 Claude Code 客户端生效：OpenAI 兼容端点等其他调用方需要保留原生 thinking 语义
pub fn is_claude_code_client(headers: &axum::http::HeaderMap) -> bool {
    headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ua| ua.starts_with("claude-cli") || ua.starts_with("claude-code"))
}

/// 思考文本化的渲染样式
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ThinkingTextStyle {
    /// 每行用 ANSI dim 转义包裹（终端客户端：显示为灰色）
    #[default]
    Ansi,
    /// 无转义的 Markdown：每行 `> *内容*`（引用块 + 斜体），用 Markdown 自带样式近似「变暗」
    /// （图形界面客户端，如 Claude Desktop：不解析 ANSI，且按 Markdown 渲染）
    Plain,
}

/// 按请求的客户端决定思考文本化的渲染样式；`None` 表示不启用（走原生 thinking）。
///
/// 渲染默认用 ANSI dim 转义，只有终端会把它当样式；Claude Desktop 等图形界面不解析，
/// 会把 `ESC` 显示成方块、`[2m` / `[0m` 显示成明文（issue #48、#54）。而这类客户端的原生
/// thinking 默认不展示思考内容，所以对它们改用无转义的 [`ThinkingTextStyle::Plain`]（引用块 + 斜体）。
///
/// 判定依据是 UA 括号内的入口字段，如 `claude-cli/2.1.231 (external, cli)` 的 `cli`、
/// `claude-cli/2.1.295 (external, local-agent, agent-sdk/0.3.295)` 的 `local-agent`：
/// - 入口为 `cli`，或解析不出入口（老格式 UA）→ [`ThinkingTextStyle::Ansi`]，保持原有行为；
/// - 其余 Claude Code 入口（`claude-desktop*`、`local-agent`、`claude-vscode`、`sdk-ts`、
///   `sdk-cli` 及未知入口）→ [`ThinkingTextStyle::Plain`]（Markdown 引用块 + 斜体）；
/// - 非 Claude Code 客户端 → `None`，走原生 thinking。
///
/// 默认方向的取舍：不能回退原生 thinking（多数客户端会折叠，长任务期间界面无输出，
/// issue #48 后续反馈中 `local-agent` 即因此回归），所以只在 ANSI 与 Plain 之间选。
/// Plain 在任何客户端都不会出现乱码，最坏只是不如变暗效果明显；ANSI 在不解析它的图形界面里
/// 每次都会显示乱码。同一个入口（如 `local-agent`）在不同宿主里渲染能力并不一致
/// （issue #54：Claude Desktop 的 Cowork 模式即为 `local-agent` 且显示乱码），
/// 入口字段区分不了，因此只有确认是终端的入口才用 ANSI。
pub fn thinking_text_style(headers: &axum::http::HeaderMap) -> Option<ThinkingTextStyle> {
    if !is_claude_code_client(headers) {
        return None;
    }
    let entrypoint = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .and_then(client_entrypoint)
        .map(str::to_ascii_lowercase);
    match entrypoint.as_deref() {
        None | Some("cli") => Some(ThinkingTextStyle::Ansi),
        Some(_) => Some(ThinkingTextStyle::Plain),
    }
}

/// 取 UA 括号内的入口字段（第二项）：`claude-cli/x (external, <entrypoint>, ...)`。
fn client_entrypoint(user_agent: &str) -> Option<&str> {
    let inner = user_agent.split_once('(')?.1.split_once(')')?.0;
    inner
        .split(',')
        .nth(1)
        .map(str::trim)
        .filter(|e| !e.is_empty())
}

/// 剥离历史 text 块中由本模块渲染的整个思考块（标记行 + 思考正文 + 时长行），
/// 上游上下文不再包含思考内容。
///
/// 仅当文本首行是 [`THOUGHT_HEADER`]（允许被 dim 转义包裹）时处理：定位流中追加的
/// 「💭 Thought for Ns (N tokens)」时长行（含其前置 `\n` 与 dim 包裹；该行可能与后续
/// 答案文本同段粘连，故按 前缀+时长+可选 token 后缀 模式定位；无 token 后缀的旧格式
/// 时长行同样兼容），丢弃从标记行到时长行（含）的全部内容，仅保留其后粘连的文本。
///
/// 时长行是思考块结束的唯一可靠边界。找不到时长行（流被中断、或首行恰为标记行的
/// 普通文本）时无法判定思考正文的范围，保守降级为只剥标记行、正文保留——宁可多留
/// 一段思考，也不误删可能是答案的内容。
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
            let line = &rest[start + 1..start + 1 + nl2];
            if let Some(span_end) = find_duration_span(line) {
                return std::borrow::Cow::Owned(after_thinking_block(
                    &rest[start + 1 + span_end..],
                    trailing_newlines_of(line),
                ));
            }
        } else if let Some(span_end) = find_duration_span(&rest[start + 1..]) {
            // 最后一行（无换行结尾）：丢弃到时长行末尾，其后粘连文本保留
            return std::borrow::Cow::Owned(after_thinking_block(
                &rest[start + 1 + span_end..],
                trailing_newlines_of(&rest[start + 1..]),
            ));
        }
        i = start + 1;
    }
    // 找不到时长行：只剥了标记行，正文保留。此时首行已确认是渲染出的思考标记行，
    // 所以可以安全地拆掉 Plain 样式的 `> *…*` 包裹（含空引用行 `>`、硬换行空格与转义），
    // 避免模型把「每行引用 + 斜体」当作自己的输出格式模仿（同 [`strip_dim_markers`] 的动机）
    std::borrow::Cow::Owned(unwrap_quoted_italic_lines(rest))
}

/// 拆除 Plain 样式渲染的逐行 `> *内容*` 包裹；不符合该形态的行原样保留。
/// 块在行中间被中断时最后一行可能缺少结束的 `*`，同样处理。
/// 渲染出的思考块是一段连续的引用行，所以只处理开头这一段，遇到第一个非 `>` 行即停止，
/// 其后的文本（可能是答案，自带的 `> *斜体*` 引用不应被误拆）原样保留。
fn unwrap_quoted_italic_lines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_quote_run = true;
    for line in text.split_inclusive('\n') {
        in_quote_run = in_quote_run && line.starts_with('>');
        if !in_quote_run {
            out.push_str(line);
            continue;
        }
        let (body, nl) = match line.strip_suffix('\n') {
            Some(b) => (b, "\n"),
            None => (line, ""),
        };
        let trimmed = body.trim_end_matches(' ');
        if trimmed == ">" {
            out.push_str(nl);
        } else if let Some(inner) = trimmed.strip_prefix(QUOTE_ITALIC_OPEN) {
            // 结束的 `*` 前若有奇数个反斜杠，则它是被转义的字面 `*`，不是闭合标记
            let closed = inner
                .strip_suffix(QUOTE_ITALIC_CLOSE)
                .is_some_and(|before| {
                    before.bytes().rev().take_while(|b| *b == b'\\').count() % 2 == 0
                });
            let inner = if closed {
                &inner[..inner.len() - QUOTE_ITALIC_CLOSE.len()]
            } else {
                inner
            };
            let mut chars = inner.chars();
            while let Some(c) = chars.next() {
                if c == '\\' {
                    match chars.next() {
                        Some(n @ ('*' | '\\')) => out.push(n),
                        Some(n) => {
                            out.push('\\');
                            out.push(n);
                        }
                        None => out.push('\\'),
                    }
                } else {
                    out.push(c);
                }
            }
            out.push_str(nl);
        } else {
            out.push_str(line);
        }
    }
    out
}

/// 思考块（到时长行末尾）之后的剩余文本：精确去掉时长行后渲染器补的换行
/// （ANSI 补一个，Plain 补两个以结束引用块；它们只用于让答案在客户端另起一段，
/// 丢弃思考块后留着会成为无意义的前导空行）。最多剥 `limit` 个，
/// 不多剥答案自身可能带的前导空行；`limit` 由 [`trailing_newlines_of`] 按时长行样式给出。
fn after_thinking_block(tail: &str, limit: usize) -> String {
    let mut rest = tail;
    for _ in 0..limit {
        match rest.strip_prefix('\n') {
            Some(r) => rest = r,
            None => break,
        }
    }
    rest.to_string()
}

/// 时长行之后渲染器补的换行个数：Plain（引用 + 斜体包裹）补两个，其余格式补一个
fn trailing_newlines_of(duration_line: &str) -> usize {
    if duration_line.starts_with(QUOTE_ITALIC_OPEN) {
        2
    } else {
        1
    }
}

/// 剥离历史 assistant 文本里的 dim 样式标记，避免模型把它们当作自己的输出格式模仿。
///
/// 背景：文本化思考的正文每行都被 `ESC[2m … ESC[0m` 包裹，并作为普通助手文本由客户端回传。
/// [`strip_rendered_thinking`] 无法判定范围而降级保留正文时（见其文档），正文行的样式码
/// 会留在上游上下文里，模型会把「每行 `[2m…[0m` 包裹的推理文字」当作自己的回复格式继续输出——且模型写不出真正
/// 的 ESC 字符，写出的是字面 `[2m` / `[0m`，客户端无从渲染而原样显示（并作为已被污染的历史
/// 继续强化该模式）。
///
/// 仅作用于发往上游的历史副本，不影响客户端自己保存与显示的内容，也不影响响应方向
/// [`ThinkingTextRewriter`] 的 dim 渲染。处理两类：
/// 1. 真正的 `ESC[2m` / `ESC[0m`（只可能来自本模块的渲染）：直接移除；
/// 2. 模型模仿产生的、**整行**被字面 `[2m` … `[0m` 包住的行：去掉两端包裹，保留行内内容。
///
///    为避免误改正常内容，字面包裹的拆除有两道门槛：
///    1. 跳过 fenced 代码块（``` / ~~~）内部，代码示例原样保留；
///    2. 同一段文本中符合整行包裹的行数 ≥ [`MIN_IMITATED_WRAPPED_LINES`] 才拆
///       （模仿污染是整段每行都包，孤立的一行多半是讲解 ANSI 的示例）。
///
///    缩进代码块、行内 `[2m`（如文档说明）因要求行首即 `[2m` 而天然不受影响。
///
/// 无任何标记时原样返回（借用，不分配）。
pub(crate) fn strip_dim_markers(text: &str) -> std::borrow::Cow<'_, str> {
    const LIT_ON: &str = "[2m";
    const LIT_OFF: &str = "[0m";
    if !text.contains('\x1b') && !text.contains(LIT_ON) {
        return std::borrow::Cow::Borrowed(text);
    }
    let no_esc = remove_ansi_style(text);

    // 逐行拆分，记录哪些行位于 fenced 代码块之外且为整行字面包裹
    let lines: Vec<&str> = no_esc.split_inclusive('\n').collect();
    let mut in_fence: Option<char> = None;
    let mut wrapped = vec![false; lines.len()];
    for (i, line) in lines.iter().enumerate() {
        let body = line.strip_suffix('\n').unwrap_or(line);
        let trimmed = body.trim_start();
        let fence_char = ["```", "~~~"]
            .iter()
            .find(|f| trimmed.starts_with(**f))
            .and_then(|f| f.chars().next());
        if let Some(c) = fence_char {
            match in_fence {
                None => in_fence = Some(c),
                Some(open) if open == c => in_fence = None,
                Some(_) => {}
            }
            continue;
        }
        if in_fence.is_none() && body.starts_with(LIT_ON) && body.ends_with(LIT_OFF) {
            // 重叠（如恰为 "[2m" 之后紧跟 "[0m" 前缀交叠）需保证两端不共用字符
            wrapped[i] = body.len() >= LIT_ON.len() + LIT_OFF.len();
        }
    }
    let unwrap_literals = wrapped.iter().filter(|w| **w).count() >= MIN_IMITATED_WRAPPED_LINES;

    let mut out = String::with_capacity(no_esc.len());
    for (i, line) in lines.iter().enumerate() {
        if unwrap_literals && wrapped[i] {
            let (body, nl) = match line.strip_suffix('\n') {
                Some(b) => (b, "\n"),
                None => (*line, ""),
            };
            out.push_str(&body[LIT_ON.len()..body.len() - LIT_OFF.len()]);
            out.push_str(nl);
        } else {
            out.push_str(line);
        }
    }
    if out == text {
        std::borrow::Cow::Borrowed(text)
    } else {
        std::borrow::Cow::Owned(out)
    }
}

/// 同一段文本中至少有这么多行整行被字面 `[2m…[0m` 包裹，才认定为模型模仿污染并拆除。
const MIN_IMITATED_WRAPPED_LINES: usize = 2;

/// 在单个行内容（不含前置 `\n`）中定位时长行段，返回段结束的相对字节偏移
/// （段起点恒为行首 0）。匹配 [DIM_ON] + 前缀 + 时长（`Ns` 或 `Nm Ys`/`NmYs`，
/// 与 format_duration 输出一致）+ 可选 token 后缀（`" (1.23k tokens)"`）+ [DIM_OFF]；
/// 命中 DIM_OFF 时段尾允许后随粘连文本（见 [`strip_rendered_thinking`] 锚定说明），
/// 否则必须到行尾才认定。token 后缀可选，兼容 v3.4.12 及更早的无后缀格式。
fn find_duration_span(line: &str) -> Option<usize> {
    let mut j = 0;
    // Plain 样式的时长行被引用 + 斜体包裹（`> *💭 Thought for …*`）
    let quoted = line.starts_with(QUOTE_ITALIC_OPEN);
    if quoted {
        j += QUOTE_ITALIC_OPEN.len();
    } else if line.starts_with(DIM_ON) {
        j += DIM_ON.len();
    } else if line.starts_with(LEGACY_DIM_ON) {
        j += LEGACY_DIM_ON.len();
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
    if quoted {
        // Plain 的答案另起一段，时长行后必为行尾；锚定到行尾避免误剥正文里的相似句子
        if line[end..].starts_with(QUOTE_ITALIC_CLOSE) {
            end += QUOTE_ITALIC_CLOSE.len();
        }
        return (end == line.len()).then_some(end);
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
    /// 渲染样式
    style: ThinkingTextStyle,
    /// 已被改写为 text 的块索引
    indices: HashSet<i32>,
    /// 已输出首行标记的块
    header_sent: HashSet<i32>,
    /// 各块当前是否位于行首（决定是否补 ANSI dim 起始转义，避免样式跨行）
    at_line_start: HashMap<i32, bool>,
    /// 各思考块的起始时刻（用于收尾计算 Thought for Ns）
    started_at: HashMap<i32, std::time::Instant>,
    /// Plain 样式：各块行内待定的空白（仅当后面还有非空白字符才输出，避免 `*foo *` 无法闭合斜体）
    pending_ws: HashMap<i32, String>,
    /// 各思考块累计的思考正文原文（用于收尾统计 token 数；不含标记行与 ANSI 转义）
    thinking_text: HashMap<i32, String>,
    /// 思考块结束后是否在同一 index 上补发原生 thinking 块（见 [`Self::with_collapse`]）
    collapse: bool,
    /// 折叠开启时各思考块累计的 signature（仅 `collapse` 为真时才累积）
    signatures: HashMap<i32, String>,
}

impl ThinkingTextRewriter {
    #[cfg(test)]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_style(style: ThinkingTextStyle) -> Self {
        Self {
            style,
            ..Self::default()
        }
    }

    /// 开启「结束后折叠」：思考块收尾时，在同一 index 上先补发原生 thinking 块
    /// （`content_block_start(thinking)` + `thinking_delta` 全文 + `signature_delta`），再放行原 stop。
    ///
    /// Claude Code 收到第二个 `content_block_start` 会清空流式文本预览，并把该 index 的块状态
    /// 原地替换为 thinking 块；`content_block_stop` 时只用最终的 thinking 块生成消息，
    /// 之前的 text 块从未落地。效果是：思考期间文本实时可见，结束后变成原生可折叠的 thinking
    /// 消息（默认折叠，Ctrl+O 展开），滚动区不再残留整段思考文本。
    ///
    /// 仅 [`ThinkingTextStyle::Ansi`]（`cli` 入口）生效：Desktop、`local-agent` 等入口的原生
    /// thinking 默认不展示，折叠会让思考内容消失，所以 Plain 样式下忽略该选项。
    pub fn with_collapse(mut self, collapse: bool) -> Self {
        self.collapse = collapse && self.style == ThinkingTextStyle::Ansi;
        self
    }

    /// 样式起始 / 复位：Ansi 为 dim + italic 转义，Plain 为引用块 + 斜体的 Markdown 标记
    fn dim_on(&self) -> &'static str {
        match self.style {
            ThinkingTextStyle::Ansi => DIM_ON,
            ThinkingTextStyle::Plain => QUOTE_ITALIC_OPEN,
        }
    }

    fn dim_off(&self) -> &'static str {
        match self.style {
            ThinkingTextStyle::Ansi => DIM_OFF,
            ThinkingTextStyle::Plain => QUOTE_ITALIC_CLOSE,
        }
    }

    /// 标记行结尾：Plain 样式下补一个空引用行（`>`），使标记行与正文分成两段且仍在同一引用块内
    fn header_terminator(&self) -> &'static str {
        match self.style {
            ThinkingTextStyle::Ansi => "\n",
            ThinkingTextStyle::Plain => "\n>\n",
        }
    }

    /// 思考正文内的换行：Plain 样式下用 Markdown 硬换行（行尾两个空格），否则相邻行会被
    /// 渲染成同一段。仅影响正文展示，历史回传时整块思考正文都会被剥离。
    fn body_line_break(&self) -> &'static str {
        match self.style {
            ThinkingTextStyle::Ansi => "\n",
            ThinkingTextStyle::Plain => "  \n",
        }
    }

    /// 时长行前的分隔。Plain 样式下用空引用行（`>`）分段而不能用空行（空行会结束引用块）；
    /// 正文停在行中间时需先换行，已在行首（上一行已带换行）则不再多换一行。
    fn duration_leader(&self, mid_line: bool) -> &'static str {
        match (self.style, mid_line) {
            (ThinkingTextStyle::Ansi, _) => "\n",
            (ThinkingTextStyle::Plain, true) => "\n>\n",
            (ThinkingTextStyle::Plain, false) => ">\n",
        }
    }

    /// 时长行结尾：Plain 样式下空一行结束引用块，否则随后的答案会被并入引用块（lazy continuation）
    fn duration_trailer(&self) -> &'static str {
        match self.style {
            ThinkingTextStyle::Ansi => "\n",
            ThinkingTextStyle::Plain => "\n\n",
        }
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
                        // signature_delta 对 text 块无意义，不转发；折叠开启时先累积，
                        // 收尾补发的原生 thinking 块需要它
                        Some("signature_delta") => {
                            if self.collapse
                                && let Some(sig) = event.data["delta"]["signature"].as_str()
                            {
                                self.signatures.entry(i).or_default().push_str(sig);
                            }
                            continue;
                        }
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
                        rendered.push_str(self.dim_on());
                        rendered.push_str(THOUGHT_HEADER);
                        rendered.push_str(self.dim_off());
                        rendered.push_str(self.header_terminator());
                    }
                    let (dim_on, dim_off, line_break) =
                        (self.dim_on(), self.dim_off(), self.body_line_break());
                    let plain_style = self.style == ThinkingTextStyle::Plain;
                    let at_start = self.at_line_start.entry(i).or_insert(true);
                    let pending_ws = self.pending_ws.entry(i).or_default();
                    for ch in text.chars() {
                        if plain_style {
                            // Plain：`> *内容*`。行首空白会让 `> * x` 变成列表项、行尾空白会让
                            // 结束的 `*` 无法闭合斜体，故丢弃行首空白、延迟行内空白到下一个
                            // 非空白字符前才输出；空行只输出空引用行 `>`（空斜体 `**` 会原样显示）
                            if ch == '\n' {
                                pending_ws.clear();
                                if *at_start {
                                    rendered.push_str(">\n");
                                } else {
                                    rendered.push_str(dim_off);
                                    rendered.push_str(line_break);
                                    *at_start = true;
                                }
                            } else if ch.is_whitespace() {
                                if !*at_start {
                                    pending_ws.push(ch);
                                }
                            } else {
                                if *at_start {
                                    rendered.push_str(dim_on);
                                    *at_start = false;
                                }
                                rendered.push_str(pending_ws);
                                pending_ws.clear();
                                // 转义 Markdown 强调符与转义符，避免思考文本里的 `*` 提前闭合斜体
                                if matches!(ch, '*' | '\\') {
                                    rendered.push('\\');
                                }
                                rendered.push(ch);
                            }
                            continue;
                        }
                        if *at_start {
                            rendered.push_str(dim_on);
                            *at_start = false;
                        }
                        if ch == '\n' {
                            // 换行前复位，样式不跨行
                            rendered.push_str(dim_off);
                            rendered.push_str(line_break);
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
                    let signature = self.signatures.remove(&i).unwrap_or_default();
                    self.pending_ws.remove(&i);
                    // 块在行中间结束时补一个复位（Ansi 的 DIM_OFF / Plain 的结束斜体 `*`），
                    // 避免样式泄漏到后续正文
                    let mid_line = self.at_line_start.remove(&i) == Some(false);
                    if mid_line {
                        out.push(SseEvent::new(
                            "content_block_delta",
                            json!({
                                "type": "content_block_delta",
                                "index": i,
                                "delta": {"type": "text_delta", "text": self.dim_off()}
                            }),
                        ));
                    }
                    // 收尾追加「💭 Thought for Ns (N tokens)」时长行；空思考块（无任何输出）不追加。
                    // 前后各补 \n：思考正文可能不以换行结尾、答案文本块会与时长行直接拼接
                    // （客户端合并相邻 text 块），先换行确保时长行独立成行，尾换行让答案另起一行。
                    // 时长 + token 摘要（不含样式），折叠时也写进原生 thinking 块正文
                    let mut summary: Option<String> = None;
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
                        summary =
                            Some(format!("{THOUGHT_DURATION_PREFIX}{duration}{token_suffix}"));
                        out.push(SseEvent::new(
                            "content_block_delta",
                            json!({
                                "type": "content_block_delta",
                                "index": i,
                                "delta": {"type": "text_delta", "text": format!(
                                    "{}{}{}{}{}{}{}",
                                    self.duration_leader(mid_line),
                                    self.dim_on(),
                                    THOUGHT_DURATION_PREFIX,
                                    duration,
                                    token_suffix,
                                    self.dim_off(),
                                    self.duration_trailer()
                                )}
                            }),
                        ));
                    }
                    // 折叠：同 index 覆写为原生 thinking 块（必须在 stop 之前、文本预览全部发完之后）。
                    // 空思考块（had_output 为假）没有发过 text 事件，不补发，保持 start/stop 配对不变。
                    if self.collapse && had_output {
                        // CC 折叠态的标签是写死的「∴ Thinking (ctrl+o to expand)」，不显示块内容；
                        // 耗时与 token 数只能放进正文末尾，展开（Ctrl+O）后可见。
                        // 回传上游时原生 thinking 块整块丢弃，这行不会进入上游上下文。
                        let thinking_body = match &summary {
                            Some(line) => format!("{}\n\n*{line}*", thinking_text.trim_end()),
                            None => thinking_text,
                        };
                        out.push(SseEvent::new(
                            "content_block_start",
                            json!({
                                "type": "content_block_start",
                                "index": i,
                                "content_block": {"type": "thinking", "thinking": "", "signature": ""}
                            }),
                        ));
                        out.push(SseEvent::new(
                            "content_block_delta",
                            json!({
                                "type": "content_block_delta",
                                "index": i,
                                "delta": {"type": "thinking_delta", "thinking": thinking_body}
                            }),
                        ));
                        if !signature.is_empty() {
                            out.push(SseEvent::new(
                                "content_block_delta",
                                json!({
                                    "type": "content_block_delta",
                                    "index": i,
                                    "delta": {"type": "signature_delta", "signature": signature}
                                }),
                            ));
                        }
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
        remove_ansi_style(s)
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

    /// 折叠开启时的完整事件序列：文本预览之后、stop 之前，同 index 覆写为原生 thinking 块
    #[test]
    fn collapse_overwrites_text_block_with_native_thinking_before_stop() {
        let mut r = ThinkingTextRewriter::with_style(ThinkingTextStyle::Ansi).with_collapse(true);
        let mut out = r.rewrite(vec![start(3, "thinking")]);
        out.extend(r.rewrite(vec![
            delta(3, "thinking_delta", "thinking", "first line\nsec"),
            delta(3, "thinking_delta", "thinking", "ond"),
            delta(3, "signature_delta", "signature", "SIG-"),
            delta(3, "signature_delta", "signature", "NATURE"),
            stop(3),
        ]));

        // 末尾四个事件：thinking start → thinking_delta(全文) → signature_delta(拼接) → stop
        let n = out.len();
        assert!(n >= 5);
        let tail = &out[n - 4..];
        assert_eq!(tail[0].event, "content_block_start");
        assert_eq!(tail[0].data["index"], 3);
        assert_eq!(
            tail[0].data["content_block"],
            json!({"type": "thinking", "thinking": "", "signature": ""})
        );
        assert_eq!(tail[1].data["delta"]["type"], "thinking_delta");
        // 正文末尾追加耗时与 token 摘要（展开后可见），原思考文本保持原样在前
        let body = tail[1].data["delta"]["thinking"].as_str().unwrap();
        assert!(
            body.starts_with("first line\nsecond\n\n*💭 Thought for 1s ("),
            "{body}"
        );
        assert!(body.ends_with(" tokens)*"), "{body}");
        assert_eq!(tail[2].data["delta"]["type"], "signature_delta");
        assert_eq!(tail[2].data["delta"]["signature"], "SIG-NATURE");
        assert_eq!(tail[3].event, "content_block_stop");
        assert!(tail.iter().all(|e| e.data["index"] == 3));

        // 之前是 text 块：恰好一个 text start 在最前，且 text 预览（含时长行）完整发出
        assert_eq!(out[0].data["content_block"]["type"], "text");
        let starts = out
            .iter()
            .filter(|e| e.event == "content_block_start")
            .count();
        assert_eq!(starts, 2, "text start + thinking start");
        let rendered = plain(&text_of(&out));
        assert!(rendered.starts_with("💭 Thinking\nfirst line\nsecond\n💭 Thought for 1s ("));
        // 覆写事件之前不得出现 thinking_delta/signature_delta（否则 CC 会在 text 块上报类型错误）
        let first_thinking = out
            .iter()
            .position(|e| {
                e.data["delta"]["type"] == "thinking_delta"
                    || e.data["delta"]["type"] == "signature_delta"
            })
            .unwrap();
        let thinking_start = out
            .iter()
            .position(|e| e.data["content_block"]["type"] == "thinking")
            .unwrap();
        assert!(thinking_start < first_thinking);
    }

    /// 折叠不改变文本预览：开关前后文本事件逐字节一致
    #[test]
    fn collapse_does_not_change_text_preview() {
        let events = || {
            vec![
                start(0, "thinking"),
                delta(0, "thinking_delta", "thinking", "ab\n\ncd"),
                stop(0),
            ]
        };
        let text_events = |collapse: bool| {
            let mut r =
                ThinkingTextRewriter::with_style(ThinkingTextStyle::Ansi).with_collapse(collapse);
            let out = r.rewrite(events());
            out.iter()
                .filter(|e| e.data["delta"]["type"] == "text_delta")
                .map(|e| e.data["delta"]["text"].as_str().unwrap().to_string())
                // 时长行含 token 数但不含易变字段（耗时下限 1s），可直接比较
                .collect::<Vec<_>>()
        };
        assert_eq!(text_events(false), text_events(true));
    }

    /// 没有 signature 时不补发 signature_delta（CC 的 thinking 块 signature 初始即为空串）
    #[test]
    fn collapse_without_signature_emits_no_signature_delta() {
        let mut r = ThinkingTextRewriter::with_style(ThinkingTextStyle::Ansi).with_collapse(true);
        let mut out = r.rewrite(vec![start(0, "thinking")]);
        out.extend(r.rewrite(vec![
            delta(0, "thinking_delta", "thinking", "想法"),
            stop(0),
        ]));
        assert!(
            out.iter()
                .any(|e| e.data["delta"]["type"] == "thinking_delta")
        );
        assert!(
            out.iter()
                .all(|e| e.data["delta"]["type"] != "signature_delta")
        );
        assert_eq!(out.last().unwrap().event, "content_block_stop");
    }

    /// Plain 样式（Desktop / local-agent 等）忽略折叠：这些入口的原生 thinking 默认不展示
    #[test]
    fn collapse_is_ignored_for_plain_style() {
        let mut r = ThinkingTextRewriter::with_style(ThinkingTextStyle::Plain).with_collapse(true);
        let mut out = r.rewrite(vec![start(0, "thinking")]);
        out.extend(r.rewrite(vec![
            delta(0, "thinking_delta", "thinking", "想法"),
            delta(0, "signature_delta", "signature", "SIG"),
            stop(0),
        ]));
        assert!(
            out.iter()
                .all(|e| e.data["content_block"]["type"] != "thinking")
        );
        assert!(
            out.iter()
                .all(|e| e.data["delta"]["type"] != "thinking_delta"
                    && e.data["delta"]["type"] != "signature_delta")
        );
        assert_eq!(out.last().unwrap().event, "content_block_stop");
    }

    /// 空思考块没有发过任何 text 事件，折叠不得补发 thinking 事件（保持 start/stop 配对）
    #[test]
    fn collapse_skips_empty_thinking_block() {
        let mut r = ThinkingTextRewriter::with_style(ThinkingTextStyle::Ansi).with_collapse(true);
        let out = r.rewrite(vec![
            start(0, "thinking"),
            delta(0, "thinking_delta", "thinking", ""),
            delta(0, "signature_delta", "signature", "SIG"),
            stop(0),
        ]);
        assert_eq!(out.len(), 2, "仅剩 text start 与 stop");
        assert_eq!(out[0].data["content_block"]["type"], "text");
    }

    /// 多个思考块各自独立覆写，signature 不串块
    #[test]
    fn collapse_keeps_signatures_per_block() {
        let mut r = ThinkingTextRewriter::with_style(ThinkingTextStyle::Ansi).with_collapse(true);
        let mut out = r.rewrite(vec![start(0, "thinking")]);
        out.extend(r.rewrite(vec![
            delta(0, "thinking_delta", "thinking", "一"),
            delta(0, "signature_delta", "signature", "S0"),
            stop(0),
            start(2, "thinking"),
            delta(2, "thinking_delta", "thinking", "二"),
            delta(2, "signature_delta", "signature", "S2"),
            stop(2),
        ]));
        let sig_of = |idx: i64| {
            out.iter()
                .find(|e| e.data["delta"]["type"] == "signature_delta" && e.data["index"] == idx)
                .map(|e| e.data["delta"]["signature"].as_str().unwrap().to_string())
        };
        assert_eq!(sig_of(0).as_deref(), Some("S0"));
        assert_eq!(sig_of(2).as_deref(), Some("S2"));
    }

    #[test]
    fn strip_drops_whole_thinking_block_up_to_duration_line() {
        // 找到时长行：标记行 + 思考正文 + 时长行整块丢弃，上游上下文不含思考内容
        let with_duration = "💭 Thinking\na\n\nb\n💭 Thought for 12s\n";
        assert_eq!(strip_rendered_thinking(with_duration), "");
        // 带 token 统计后缀的新格式
        let with_tokens = "💭 Thinking\na\n\nb\n💭 Thought for 12s (1.23k tokens)\n";
        assert_eq!(strip_rendered_thinking(with_tokens), "");
        // M 档后缀同样可识别
        let with_mega = "💭 Thinking\na\n\n💭 Thought for 12s (1.23M tokens)\n";
        assert_eq!(strip_rendered_thinking(with_mega), "");
        // 带 token 后缀且与答案同行粘连（有 DIM_OFF）：只保留答案
        let with_tokens_merged = "💭 Thinking\na\n\n💭 Thought for 12s (999 tokens)\x1b[0manswer";
        assert_eq!(strip_rendered_thinking(with_tokens_merged), "answer");
        // 客户端把相邻 text 块合并的情形（时长行后换行再接答案）：去掉渲染器补的那个换行
        let merged = "💭 Thinking\na\n\nb\n💭 Thought for 12s\nanswer";
        assert_eq!(strip_rendered_thinking(merged), "answer");
        // ≥60s 的新格式「XmYs」同样可识别
        let with_minutes = "💭 Thinking\na\n\n💭 Thought for 1m15s\nanswer";
        assert_eq!(strip_rendered_thinking(with_minutes), "answer");
        // 旧格式时长行与答案同行粘连且带 DIM_OFF：只保留答案
        let legacy = "💭 Thinking\na\n\n💭 Thought for 12s\x1b[0manswer";
        assert_eq!(strip_rendered_thinking(legacy), "answer");
        // 思考块后答案自带多段内容：原样保留（仅去掉紧随时长行的一个换行）
        let multi = "💭 Thinking\na\n💭 Thought for 3s\n第一段\n\n第二段";
        assert_eq!(strip_rendered_thinking(multi), "第一段\n\n第二段");
    }

    #[test]
    fn strip_falls_back_to_header_only_when_duration_line_missing() {
        // 无时长行（流被中断等）：无法判定思考正文范围，保守降级为只剥标记行、正文保留
        let rendered = "💭 Thinking\na\n\nb\n";
        assert_eq!(strip_rendered_thinking(rendered), "a\n\nb\n");
        // 行首即前缀但行尾锚定失败（无 DIM_OFF 且行尾有余文）：不认定为时长行
        let anchored = "💭 Thinking\n💭 Thought for 5s per step\n答案";
        assert_eq!(
            strip_rendered_thinking(anchored),
            "💭 Thought for 5s per step\n答案"
        );
        // 思考正文恰含相似句子但不带 DIM_OFF、未到行尾：不被误认（首行标记正常剥除）
        let lookalike = "💭 Thinking\n需 💭 Thought for 5s per step\n答案";
        assert_eq!(
            strip_rendered_thinking(lookalike),
            "需 💭 Thought for 5s per step\n答案"
        );
        // 普通文本（含普通引用）不受影响
        assert_eq!(strip_rendered_thinking("> quote\nx"), "> quote\nx");
        assert_eq!(strip_rendered_thinking("plain"), "plain");
    }

    #[test]
    fn ansi_style_is_dim_italic_and_legacy_dim_only_is_still_stripped() {
        // 生成侧：dim + italic（SGR 2;3）
        assert_eq!(DIM_ON, "\x1b[2;3m");

        // 剥离侧：新格式与旧版本只变暗的格式（可能还留在客户端历史里）都要整块识别
        let render = |on: &str| {
            format!(
                "{on}{THOUGHT_HEADER}{DIM_OFF}\n{on}思考一{DIM_OFF}\n{on}思考二{DIM_OFF}\n\n\
                 {on}{THOUGHT_DURATION_PREFIX}2s (5 tokens){DIM_OFF}\n答案"
            )
        };
        let new_fmt = render(DIM_ON);
        let old_fmt = render(LEGACY_DIM_ON);
        assert_eq!(strip_rendered_thinking(&new_fmt), "答案");
        assert_eq!(strip_rendered_thinking(&old_fmt), "答案");

        // 降级路径（流被中断、缺少时长行）：只剥标记行，两种格式都要认出标记行
        for on in [DIM_ON, LEGACY_DIM_ON] {
            let interrupted = format!("{on}{THOUGHT_HEADER}{DIM_OFF}\n{on}思考{DIM_OFF}\n");
            assert_eq!(
                strip_rendered_thinking(&interrupted),
                format!("{on}思考{DIM_OFF}\n"),
                "{on:?}"
            );
        }

        // 残留转义清理：新旧起始转义与复位都要去掉
        assert_eq!(
            strip_dim_markers("\x1b[2;3m推理一\x1b[0m\n\x1b[2m推理二\x1b[0m\n答案"),
            "推理一\n推理二\n答案"
        );
    }

    #[test]
    fn strip_dim_markers_removes_real_escape_codes() {
        let t = "\x1b[2m推理一\x1b[0m\n\x1b[2m\x1b[0m\n\x1b[2m推理二\x1b[0m\n答案";
        assert_eq!(strip_dim_markers(t), "推理一\n\n推理二\n答案");
    }

    #[test]
    fn strip_dim_markers_unwraps_model_imitated_literal_lines() {
        // 截图场景：模型写出的字面包裹行（无 ESC），含空行 `[2m[0m`
        let t = "[2m修改已成功完成。我需要：[0m\n[2m[0m\n[2m> 引用行[0m\n正常行";
        assert_eq!(
            strip_dim_markers(t),
            "修改已成功完成。我需要：\n\n> 引用行\n正常行"
        );
        // 真实 ESC 外层再套字面包裹（先剥 ESC，再剥整行包裹）
        assert_eq!(
            strip_dim_markers("\x1b[2m[2m内容一[0m\x1b[0m\n\x1b[2m[2m内容二[0m\x1b[0m"),
            "内容一\n内容二"
        );
    }

    #[test]
    fn strip_dim_markers_unwraps_imitated_lines_outside_fence_only() {
        // 围栏外的多行污染被拆，围栏内同形态的行保留
        let t = "[2m推理一[0m\n[2m推理二[0m\n```\n[2mkeep[0m\n```\n[2m推理三[0m";
        assert_eq!(
            strip_dim_markers(t),
            "推理一\n推理二\n```\n[2mkeep[0m\n```\n推理三"
        );
        // 未闭合的围栏：其后内容视为代码，不拆
        let open = "[2ma[0m\n[2mb[0m\n```\n[2mc[0m\n[2md[0m";
        assert_eq!(strip_dim_markers(open), "a\nb\n```\n[2mc[0m\n[2md[0m");
    }

    #[test]
    fn strip_dim_markers_leaves_unrelated_text_untouched() {
        for t in [
            "plain",
            "",
            "用 `[2m` 设置变暗，`[0m` 复位",
            "行内 [2m 不是整行包裹 [0m 之后还有字",
            "[2m只有开头没有结尾",
            "没有开头只有结尾[0m",
            // 孤立的一行整行包裹（低于门槛）：多半是示例，不动
            "示例输出：\n[2mhello[0m\n结束",
            // fenced 代码块内的整行包裹（即使有多行）：代码示例原样保留
            "```\n[2mhello[0m\n[2mworld[0m\n```",
            "~~~text\n[2ma[0m\n[2mb[0m\n~~~",
            // 缩进代码块行首不是 `[2m`
            "    [2mhello[0m\n    [2mworld[0m",
        ] {
            let out = strip_dim_markers(t);
            assert_eq!(out, t);
            assert!(matches!(out, std::borrow::Cow::Borrowed(_)), "{t}");
        }
    }

    #[test]
    fn rendered_thinking_roundtrip_is_fully_dropped_from_history() {
        // 真实渲染产物（含 dim 转义、时长行、token 后缀）经历史剥离后不留任何思考内容
        let mut r = ThinkingTextRewriter::new();
        let mut out = r.rewrite(vec![start(0, "thinking")]);
        out.extend(r.rewrite(vec![
            delta(0, "thinking_delta", "thinking", "想一想\n\n再想想\n"),
            stop(0),
        ]));
        let raw = text_of(&out);
        assert!(raw.contains('\x1b'), "响应方向仍应带 dim 转义: {raw:?}");
        let stripped = strip_dim_markers(&strip_rendered_thinking(&raw)).into_owned();
        assert_eq!(stripped, "", "{stripped:?}");

        // 与答案 text 块被客户端合并拼接的形态同样只留答案
        let merged = format!("{raw}最终答案");
        let stripped = strip_dim_markers(&strip_rendered_thinking(&merged)).into_owned();
        assert_eq!(stripped, "最终答案");
    }

    #[test]
    fn rendered_thinking_body_dropped_even_without_ansi() {
        // 客户端剥离全部 ANSI 后回传的形态（plain）也能整块识别
        let mut r = ThinkingTextRewriter::new();
        let mut out = r.rewrite(vec![start(0, "thinking")]);
        out.extend(r.rewrite(vec![
            delta(0, "thinking_delta", "thinking", "想一想\n\n再想想\n"),
            stop(0),
        ]));
        let rendered = plain(&text_of(&out));
        assert_eq!(strip_rendered_thinking(&rendered), "");
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
                .with_thinking_as_text(enabled.then_some(ThinkingTextStyle::Ansi));
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
                // 历史剥离后思考块整块不回传，只剩答案
                let rendered = plain(&all_text);
                assert_eq!(strip_rendered_thinking(&rendered), "最终答案");
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
        // dim 渲染的内容同样能被历史剥离识别（整个思考块丢弃，仅留粘连的答案）
        assert_eq!(strip_rendered_thinking(&text), "");
        assert_eq!(strip_rendered_thinking(&format!("{text}answer")), "answer");
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

    fn headers_with_ua(ua: &str) -> axum::http::HeaderMap {
        let mut h = axum::http::HeaderMap::new();
        h.insert(axum::http::header::USER_AGENT, ua.parse().unwrap());
        h
    }

    fn style_of(ua: &str) -> Option<ThinkingTextStyle> {
        thinking_text_style(&headers_with_ua(ua))
    }

    #[test]
    fn style_is_ansi_for_terminal_cli() {
        assert_eq!(
            style_of("claude-cli/2.1.231 (external, cli)"),
            Some(ThinkingTextStyle::Ansi)
        );
        // 入口大小写不敏感
        assert_eq!(
            style_of("claude-cli/2.1.231 (external, CLI)"),
            Some(ThinkingTextStyle::Ansi)
        );
    }

    #[test]
    fn style_is_plain_for_claude_desktop() {
        // issue #48：Claude Desktop 不解析 ANSI，且原生 thinking 默认不展示
        assert_eq!(
            style_of("claude-cli/2.1.295 (external, claude-desktop-3p, agent-sdk/0.3.295)"),
            Some(ThinkingTextStyle::Plain)
        );
        assert_eq!(
            style_of("claude-cli/2.1.295 (external, claude-desktop, agent-sdk/0.3.295)"),
            Some(ThinkingTextStyle::Plain)
        );
    }

    #[test]
    fn style_is_plain_for_every_non_cli_claude_code_entrypoint() {
        // 只有 cli 确认是终端渲染；其余入口用 Plain：既不回退原生 thinking
        // （issue #48 后续反馈：local-agent 因此看不到思考过程），也不显示 ANSI 乱码
        // （issue #54：Claude Desktop Cowork 模式入口为 local-agent，不解析 ANSI）
        for ua in [
            "claude-cli/2.1.295 (external, local-agent, agent-sdk/0.3.295)",
            "claude-cli/2.1.295 (external, claude-vscode, agent-sdk/0.3.295)",
            "claude-cli/2.1.295 (external, sdk-ts)",
            "claude-cli/2.1.295 (external, sdk-cli)",
            "claude-cli/2.1.295 (external, some-future-entrypoint)",
        ] {
            assert_eq!(style_of(ua), Some(ThinkingTextStyle::Plain), "{ua}");
        }
    }

    #[test]
    fn style_keeps_legacy_behavior_when_entrypoint_unparseable() {
        // 无括号 / 字段不足：沿用原有“Claude Code 前缀即放行”（Ansi）
        for ua in [
            "claude-code/1.0",
            "claude-cli/2.1.231",
            "claude-cli/2.1.231 (external)",
            "claude-cli/2.1.231 (external, )",
        ] {
            assert_eq!(style_of(ua), Some(ThinkingTextStyle::Ansi), "{ua}");
        }
    }

    #[test]
    fn style_is_none_for_non_claude_code_or_missing_ua() {
        assert_eq!(thinking_text_style(&axum::http::HeaderMap::new()), None);
        assert_eq!(style_of("codex/1.0 (external, cli)"), None);
    }

    /// 用 Plain 样式渲染一个完整思考块（按给定分片流入），返回客户端看到的文本
    fn render_plain(chunks: &[&str]) -> String {
        let mut r = ThinkingTextRewriter::with_style(ThinkingTextStyle::Plain);
        let mut out = r.rewrite(vec![start(0, "thinking")]);
        let mut events: Vec<SseEvent> = chunks
            .iter()
            .map(|c| delta(0, "thinking_delta", "thinking", c))
            .collect();
        events.push(stop(0));
        out.extend(r.rewrite(events));
        text_of(&out)
    }

    #[test]
    fn plain_style_renders_quote_italic_and_roundtrips_through_history_strip() {
        let rendered = render_plain(&["第一行\n第二", "行\n\n第三段"]);

        // 无任何 ANSI 转义
        assert!(!rendered.contains('\x1b'), "{rendered:?}");
        // 标记行、正文、时长行都在同一个引用块内（块内用 `>` 空行分段，不能有真空行）；
        // 每行斜体；正文换行使用 Markdown 硬换行
        assert!(
            rendered.starts_with(&format!(
                "> *{THOUGHT_HEADER}*\n>\n> *第一行*  \n> *第二行*  \n>\n> *第三段*\n>\n> *{THOUGHT_DURATION_PREFIX}"
            )),
            "{rendered:?}"
        );
        let body = rendered.trim_end_matches('\n');
        assert!(!body.contains("\n\n"), "引用块内不得出现空行: {rendered:?}");
        // 时长行以空行收尾，结束引用块，使随后的答案不被并入引用
        assert!(rendered.ends_with("tokens)*\n\n"), "{rendered:?}");

        // 客户端把思考块与随后的答案拼成一个 assistant 文本回传：整块思考被剥离，只剩答案
        let history_text = format!("{rendered}最终答案");
        assert_eq!(strip_rendered_thinking(&history_text), "最终答案");
    }

    #[test]
    fn plain_style_trims_and_escapes_markdown_hazards() {
        // 行首空白（会让 `> * x` 变列表）、行尾空白（会让结束 `*` 无法闭合）、
        // 跨分片的行内空白、`*` 与 `\`（会提前闭合斜体 / 吞掉后续字符）
        let rendered = render_plain(&["  缩进行  \n", "a ", "* b", " \\c  \n   \n- 列表\n"]);
        assert!(
            rendered.contains("> *缩进行*  \n"),
            "行首 / 行尾空白应被去掉: {rendered:?}"
        );
        assert!(
            rendered.contains("> *a \\* b \\\\c*  \n"),
            "行内空白保留，`*` 与 `\\` 应转义: {rendered:?}"
        );
        assert!(
            rendered.contains("*  \n>\n> *- 列表*  \n"),
            "纯空白行应变为空引用行，列表符号应被斜体包裹而非成为列表: {rendered:?}"
        );
        assert!(!rendered.contains("**"), "不得出现空斜体: {rendered:?}");
        let history_text = format!("{rendered}答案");
        assert_eq!(strip_rendered_thinking(&history_text), "答案");
    }

    #[test]
    fn plain_style_closes_italic_when_block_ends_mid_line() {
        // 块在行中间结束：补结束 `*`，时长行前先换行 + 空引用行；行首结束则不多换行
        let mid = render_plain(&["没有换行结尾"]);
        assert!(
            mid.starts_with("> *💭 Thinking*\n>\n> *没有换行结尾*\n>\n> *💭 Thought for "),
            "{mid:?}"
        );
        let at_line_start = render_plain(&["有换行结尾\n"]);
        assert!(
            at_line_start.contains("> *有换行结尾*  \n>\n> *💭 Thought for "),
            "{at_line_start:?}"
        );
        for text in [mid, at_line_start] {
            assert_eq!(strip_rendered_thinking(&format!("{text}答案")), "答案");
        }
    }

    #[test]
    fn plain_style_emits_no_empty_deltas() {
        let mut r = ThinkingTextRewriter::with_style(ThinkingTextStyle::Plain);
        let mut out = r.rewrite(vec![start(0, "thinking")]);
        out.extend(r.rewrite(vec![
            delta(0, "thinking_delta", "thinking", "没有换行结尾"),
            stop(0),
        ]));
        let deltas: Vec<&str> = out
            .iter()
            .filter_map(|e| e.data["delta"]["text"].as_str())
            .collect();
        assert!(deltas.iter().all(|t| !t.is_empty() && !t.contains('\x1b')));
    }

    #[test]
    fn strip_still_handles_legacy_unquoted_plain_format() {
        // v3.4.24–v3.4.36 的 Plain 输出（无引用 / 斜体包裹，答案紧随一个换行）仍需整块剥离，
        // 客户端历史里可能还有这种格式
        let legacy = "💭 Thinking\n\n第一行  \n第二行  \n\n💭 Thought for 3s (41 tokens)\n最终答案";
        assert_eq!(strip_rendered_thinking(legacy), "最终答案");
    }

    #[test]
    fn strip_does_not_match_quoted_body_line_that_merely_resembles_duration() {
        // 引用格式的时长行必须锚定行尾：正文里「> *💭 Thought for 5s 的结论…*」不应被当作边界
        let text = "> *💭 Thinking*\n>\n> *💭 Thought for 5s 的结论是错的*  \n>\n> *💭 Thought for 7s (9 tokens)*\n\n答案";
        assert_eq!(strip_rendered_thinking(text), "答案");
    }

    #[test]
    fn interrupted_plain_block_is_unwrapped_when_stripped() {
        // 流被中断（无时长行）：只能剥标记行，但正文的 `> *…*` 包裹要拆掉，
        // 免得模型把「每行引用 + 斜体」当作自己的输出格式模仿
        let mut r = ThinkingTextRewriter::with_style(ThinkingTextStyle::Plain);
        let mut out = r.rewrite(vec![start(0, "thinking")]);
        out.extend(r.rewrite(vec![delta(
            0,
            "thinking_delta",
            "thinking",
            "第一行\n含*星\\号\n\n以星号结尾\\*\n未完",
        )]));
        let rendered = text_of(&out);
        assert!(rendered.contains("> *第一行*  \n"), "{rendered:?}");
        assert_eq!(
            strip_rendered_thinking(&rendered),
            "\n第一行\n含*星\\号\n\n以星号结尾\\*\n未完"
        );
    }

    #[test]
    fn unwrap_leaves_non_matching_lines_untouched() {
        // 只在已确认首行是思考标记行时才进入降级拆包；不符合形态的行原样保留
        let text = "> *💭 Thinking*\n普通行\n> 普通引用\n";
        assert_eq!(strip_rendered_thinking(text), "普通行\n> 普通引用\n");
        // 首行不是标记行：整段原样返回
        let plain_text = "> *a*\n> *b*\n";
        assert_eq!(strip_rendered_thinking(plain_text), plain_text);
    }

    #[test]
    fn strip_removes_only_the_newlines_the_renderer_added() {
        // Plain 补两个、ANSI 补一个；答案自己的前导空行必须保留
        let plain = "> *💭 Thinking*\n>\n> *思考*\n>\n> *💭 Thought for 2s (5 tokens)*\n\n\n答案";
        assert_eq!(strip_rendered_thinking(plain), "\n答案");
        let ansi = format!(
            "{DIM_ON}{THOUGHT_HEADER}{DIM_OFF}\n{DIM_ON}思考{DIM_OFF}\n\n{DIM_ON}{THOUGHT_DURATION_PREFIX}2s (5 tokens){DIM_OFF}\n\n答案"
        );
        assert_eq!(strip_rendered_thinking(&ansi), "\n答案");
    }

    #[test]
    fn unwrap_stops_at_end_of_leading_quote_run() {
        // 降级时只拆开头那段思考引用块；其后文本里自带的 `> *斜体*` 引用保持原样
        let text = "> *💭 Thinking*\n>\n> *思考*  \n\n答案如下\n> *用户引用*\n";
        assert_eq!(
            strip_rendered_thinking(text),
            "\n思考\n\n答案如下\n> *用户引用*\n"
        );
    }
}
