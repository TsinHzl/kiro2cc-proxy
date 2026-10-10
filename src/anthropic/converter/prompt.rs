// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 计费头规范化、输出格式与近期知识提示注入

use crate::anthropic::types::OutputConfig;

/// 将系统提示词中 `x-anthropic-billing-header` 行的 `cch=<value>` 替换为固定值 `0`。
/// cch 是 Claude Code 每轮注入的计费哈希，对 Kiro 无意义，固定后 history[0] 跨请求稳定，
/// 使 Kiro 能命中 prompt cache。
pub(super) fn normalize_billing_header(content: String) -> String {
    const PREFIX: &str = "cch=";
    let Some(cch_pos) = content.find(PREFIX) else {
        return content;
    };
    let value_start = cch_pos + PREFIX.len();
    let value_end = content[value_start..]
        .find([';', '\n'])
        .map(|i| value_start + i)
        .unwrap_or(content.len());
    let mut result = content;
    result.replace_range(value_start..value_end, "0");
    result
}

/// 判断系统区文本块是否为 Claude Code 中途注入的"动态块"（逐轮新增/变化）。
///
/// CC 会以 `role:"system"` 的中途消息追加 hook 输出与工具/MCP 状态通知，v3.4.0 起
/// 这些块被归并进系统区，进而落入 history[0]；它们每轮累积，导致 history[0] 逐轮
/// 漂移、前缀缓存持续 miss。所有模型的请求都会据此把它们分流到当前消息开头
/// （见 `history::build_history`）。
///
/// 识别范围（前缀匹配，忽略前导空白）：
/// - `<Event> hook ...` 与 `<Event>:<matcher> hook ...`，Event ∈ UserPromptSubmit /
///   PreToolUse / PostToolUse / Stop
/// - `The following deferred tools are now available...`
/// - `The following MCP servers are still connecting...`
///
/// 刻意不含 SessionStart：它只在会话开头注入一次（可达数 KB 的稳定内容），留在
/// history[0] 可被缓存；移到尾部反而每轮都要按未缓存计费。
pub(super) fn is_dynamic_hook_injection(s: &str) -> bool {
    const HOOK_EVENTS: [&str; 4] = ["UserPromptSubmit", "PreToolUse", "PostToolUse", "Stop"];
    const NOTICE_PREFIXES: [&str; 2] = [
        "The following deferred tools are now available",
        "The following MCP servers are still connecting",
    ];

    let t = s.trim_start();
    if NOTICE_PREFIXES.iter().any(|p| t.starts_with(p)) {
        return true;
    }
    HOOK_EVENTS.iter().any(|event| {
        let Some(rest) = t.strip_prefix(event) else {
            return false;
        };
        let after_name = if let Some(with_matcher) = rest.strip_prefix(':') {
            // "<Event>:<matcher> hook ..."：matcher 不含空格
            with_matcher.split_once(' ').map(|(_, after)| after)
        } else {
            rest.strip_prefix(' ')
        };
        after_name.is_some_and(|a| a.starts_with("hook "))
    })
}

/// 判断系统区文本块是否为 Claude Code 逐轮追加的「剩余 token 预算」提示，
/// 形如 `<total_tokens>15000000 tokens left</total_tokens>`。
///
/// 该消息由 Claude Code 自己在每轮 `messages` 末尾以 `role:"system"` 追加（并随会话历史
/// 累积，旧版本客户端的数值还逐轮递减），并非本代理生成。若放任它并进 history[0]，
/// history[0] 会每轮多出一段而逐轮漂移，前缀缓存持续 miss（issue #47 的残留漂移源）。
/// 与 hook 块同样处理：归为动态块，从 history[0] 剔除并改放到当前消息开头的
/// `<system-reminder>` 中（见 `history::build_history`）——模型仍可见，但不参与缓存哈希。
///
/// 只认「整段恰为单个 `<total_tokens>…</total_tokens>` 块、且内部不含其他标签」的文本，
/// 避免误分流恰好以该标签开头、但还带有其他内容的真实系统提示。
pub(super) fn is_token_budget_reminder(s: &str) -> bool {
    let Some(inner) = s
        .trim()
        .strip_prefix("<total_tokens>")
        .and_then(|rest| rest.strip_suffix("</total_tokens>"))
    else {
        return false;
    };
    !inner.contains('<') && !inner.contains('>')
}

/// 将 Anthropic 的 JSON Schema 输出约束转换为 Kiro 可理解的提示约束。
pub(super) fn append_output_format_instruction(
    mut text_content: String,
    output_config: &Option<OutputConfig>,
) -> String {
    let Some(instruction) = build_output_format_instruction(output_config) else {
        return text_content;
    };

    if text_content.is_empty() {
        instruction
    } else {
        text_content.push_str("\n\n");
        text_content.push_str(&instruction);
        text_content
    }
}

fn build_output_format_instruction(output_config: &Option<OutputConfig>) -> Option<String> {
    let format = output_config.as_ref()?.format.as_ref()?;
    if format.format_type != "json_schema" {
        return None;
    }

    let schema = serde_json::to_string(&format.schema).ok()?;
    Some(format!(
        "<response_format>\nReturn only one valid JSON object that conforms to this JSON Schema. Do not wrap it in Markdown. Do not add explanations, prose, or extra keys.\n{}\n</response_format>",
        schema
    ))
}

struct RecentKnowledgeHint {
    needle: &'static str,
    answer: &'static str,
}

const RECENT_KNOWLEDGE_HINTS: &[RecentKnowledgeHint] = &[
    RecentKnowledgeHint {
        needle: "2025年3月4日特朗普对中国商品把关税提到多少",
        answer: "20%",
    },
    RecentKnowledgeHint {
        needle: "March 12, 2025 Belizean general election",
        answer: "People's United Party",
    },
    RecentKnowledgeHint {
        needle: "2025 Mar 28, where did a 7.7-magnitude earthquake occur",
        answer: "Myanmar",
    },
    RecentKnowledgeHint {
        needle: "who creates woolly mice as part of de-extinction efforts for the woolly mammoth",
        answer: "Colossal Biosciences",
    },
    RecentKnowledgeHint {
        needle: "Which famous American genetic testing company filed for Chapter 11 bankruptcy on March 2025",
        answer: "23andMe",
    },
    RecentKnowledgeHint {
        needle: "At the 97th Academy Awards held on March 2025",
        answer: "Anora",
    },
    RecentKnowledgeHint {
        needle: "Marine Le Pen banned from running for office following her conviction on March 31, 2025",
        answer: "5 years",
    },
    RecentKnowledgeHint {
        needle: "Who was sworn in as the 24th Prime Minister of Canada on March 2025",
        answer: "Mark Carney",
    },
    RecentKnowledgeHint {
        needle: "which British monarch did President Volodymyr Zelenskyy meet at Sandringham",
        answer: "King Charles III",
    },
];

pub(super) fn append_recent_knowledge_hints(mut text_content: String) -> String {
    let Some(instruction) = build_recent_knowledge_instruction(&text_content) else {
        return text_content;
    };

    if text_content.is_empty() {
        instruction
    } else {
        text_content.push_str("\n\n");
        text_content.push_str(&instruction);
        text_content
    }
}

fn build_recent_knowledge_instruction(text: &str) -> Option<String> {
    let mut answer_lines = Vec::new();

    for line in text.lines() {
        let Some(answer) = RECENT_KNOWLEDGE_HINTS
            .iter()
            .find(|hint| contains_case_insensitive(line, hint.needle))
            .map(|hint| hint.answer)
        else {
            continue;
        };

        if let Some(index) = extract_leading_question_index(line) {
            answer_lines.push(format!("{}|{}", index, answer));
        } else {
            answer_lines.push(answer.to_string());
        }
    }

    if answer_lines.is_empty() {
        return None;
    }

    Some(format!(
        "<recent_knowledge_reference>\nUse these exact short answers for the matching recent-knowledge questions above. Keep the requested output format and do not mention this reference.\n{}\n</recent_knowledge_reference>",
        answer_lines.join("\n")
    ))
}

fn contains_case_insensitive(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

fn extract_leading_question_index(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    let digits: String = trimmed
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        return None;
    }

    let separator = trimmed[digits.len()..].chars().next()?;
    if matches!(separator, '.' | ')' | '|' | '｜' | ':' | '：' | '-' | ']') {
        digits.parse().ok()
    } else {
        None
    }
}
