// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Token 计算模块
//!
//! 提供文本 token 数量计算功能。
//!
//! # 计算规则
//! - 非西文字符：每个计 4.0 个字符单位
//! - 西文字符：每个计 1 个字符单位
//! - 4 个字符单位 = 1 token（向上取整）

use crate::anthropic::types::{
    CountTokensRequest, CountTokensResponse, Message, SystemMessage, Tool,
};
use crate::http_client::{ProxyConfig, build_client};
use crate::model::config::TlsBackend;
use std::sync::OnceLock;

/// Count Tokens API 配置
#[derive(Clone, Default)]
pub struct CountTokensConfig {
    /// 外部 count_tokens API 地址
    pub api_url: Option<String>,
    /// count_tokens API 密钥
    pub api_key: Option<String>,
    /// count_tokens API 认证类型（"x-api-key" 或 "bearer"）
    pub auth_type: String,
    /// 代理配置
    pub proxy: Option<ProxyConfig>,

    pub tls_backend: TlsBackend,
}

/// 全局配置存储
static COUNT_TOKENS_CONFIG: OnceLock<CountTokensConfig> = OnceLock::new();

/// 初始化 count_tokens 配置
///
/// 应在应用启动时调用一次
pub fn init_config(config: CountTokensConfig) {
    let _ = COUNT_TOKENS_CONFIG.set(config);
}

/// 获取配置
fn get_config() -> Option<&'static CountTokensConfig> {
    COUNT_TOKENS_CONFIG.get()
}

/// 判断字符是否为非西文字符（已不再被 count_tokens 使用，保留供历史调用方）
#[allow(dead_code)]
fn is_non_western_char(c: char) -> bool {
    !matches!(c,
        // 基本 ASCII
        '\u{0000}'..='\u{007F}' |
        // 拉丁字母扩展-A (Latin Extended-A)
        '\u{0080}'..='\u{00FF}' |
        // 拉丁字母扩展-B (Latin Extended-B)
        '\u{0100}'..='\u{024F}' |
        // 拉丁字母扩展附加 (Latin Extended Additional)
        '\u{1E00}'..='\u{1EFF}' |
        // 拉丁字母扩展-C/D/E
        '\u{2C60}'..='\u{2C7F}' |
        '\u{A720}'..='\u{A7FF}' |
        '\u{AB30}'..='\u{AB6F}'
    )
}

/// 计算文本的 token 数量（四分类加权）
///
/// # 计算规则
/// - ASCII 字母 (A-Za-z): 每字符 / 4.5（英文 BPE 平均 ~4.5 chars/token）
/// - 数字 (0-9): 每字符 / 2.0（数字 BPE 拆分粒度细）
/// - 其他 ASCII (符号、空白): 每字符 / 1.5（符号常单独成 token）
/// - 非 ASCII (CJK 等): 每字符 / 1.5（中文 BPE 平均 ~1.5 chars/token）
/// - 向上取整，最少 1 token
pub fn count_tokens(text: &str) -> u64 {
    let mut letters: usize = 0;
    let mut digits: usize = 0;
    let mut ascii_symbols: usize = 0;
    let mut non_ascii: usize = 0;

    for c in text.chars() {
        match c {
            'A'..='Z' | 'a'..='z' => letters += 1,
            '0'..='9' => digits += 1,
            c if (c as u32) < 0x80 => ascii_symbols += 1,
            _ => non_ascii += 1,
        }
    }

    let units = letters as f64 / 4.5
        + digits as f64 / 2.0
        + ascii_symbols as f64 / 1.5
        + non_ascii as f64 / 1.5;

    (units.ceil() as u64).max(1)
}

/// 估算请求的输入 tokens
///
/// 优先调用远程 API，失败时回退到本地计算
pub(crate) fn count_all_tokens(
    model: String,
    system: Option<Vec<SystemMessage>>,
    messages: Vec<Message>,
    tools: Option<Vec<Tool>>,
) -> u64 {
    // 检查是否配置了远程 API
    if let Some(config) = get_config()
        && let Some(api_url) = &config.api_url
    {
        // 尝试调用远程 API
        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(call_remote_count_tokens(
                api_url, config, model, &system, &messages, &tools,
            ))
        });

        match result {
            Ok(tokens) => {
                tracing::debug!("远程 count_tokens API 返回: {}", tokens);
                return tokens;
            }
            Err(e) => {
                tracing::warn!("远程 count_tokens API 调用失败，回退到本地计算: {}", e);
            }
        }
    }

    // 本地计算
    count_all_tokens_local(system, messages, tools)
}

/// 调用远程 count_tokens API
async fn call_remote_count_tokens(
    api_url: &str,
    config: &CountTokensConfig,
    model: String,
    system: &Option<Vec<SystemMessage>>,
    messages: &[Message],
    tools: &Option<Vec<Tool>>,
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    let client = build_client(config.proxy.as_ref(), 300, config.tls_backend)?;

    // 构建请求体
    let request = CountTokensRequest {
        model, // 模型名称用于 token 计算
        messages: messages.to_vec(),
        system: system.clone(),
        tools: tools.clone(),
    };

    // 构建请求
    let mut req_builder = client.post(api_url);

    // 设置认证头
    if let Some(api_key) = &config.api_key {
        if config.auth_type == "bearer" {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", api_key));
        } else {
            req_builder = req_builder.header("x-api-key", api_key);
        }
    }

    // 发送请求
    let response = req_builder
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(format!("API 返回错误状态: {}", response.status()).into());
    }

    let result: CountTokensResponse = response.json().await?;
    Ok(result.input_tokens as u64)
}

/// 统计单个 content block 的 token 数。
///
/// 覆盖三类 block：
/// - `text` —— 取 `text` 字段
/// - `tool_use` —— 取 `name` + `input` 的 JSON 序列化
/// - `tool_result` —— `content` 是字符串则直接计数；是数组则递归取每个 text 子项
///
/// 其它类型（如 `image`）无法字符估算，跳过。
fn count_content_block(item: &serde_json::Value) -> u64 {
    let block_type = item.get("type").and_then(|v| v.as_str());

    match block_type {
        Some("tool_use") => {
            let mut t = 0;
            if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
                t += count_tokens(name);
            }
            if let Some(input) = item.get("input") {
                let input_json = serde_json::to_string(input).unwrap_or_default();
                t += count_tokens(&input_json);
            }
            t
        }
        Some("tool_result") => {
            let mut t = 0;
            match item.get("content") {
                Some(serde_json::Value::String(s)) => {
                    t += count_tokens(s);
                }
                Some(serde_json::Value::Array(arr)) => {
                    for sub in arr {
                        // 递归:子元素也按 block 处理,与 image 等类型扩展时保持一致行为。
                        // 低估方向安全(clamp 由上游保证)。
                        t += count_content_block(sub);
                    }
                }
                _ => {}
            }
            t
        }
        _ => item
            .get("text")
            .and_then(|v| v.as_str())
            .map(count_tokens)
            .unwrap_or(0),
    }
}

/// 本地计算请求的输入 tokens
fn count_all_tokens_local(
    system: Option<Vec<SystemMessage>>,
    messages: Vec<Message>,
    tools: Option<Vec<Tool>>,
) -> u64 {
    let mut total = 0;

    // 系统消息
    if let Some(ref system) = system {
        for msg in system {
            total += count_tokens(&msg.text);
        }
    }

    // 用户消息
    for msg in &messages {
        if let serde_json::Value::String(s) = &msg.content {
            total += count_tokens(s);
        } else if let serde_json::Value::Array(arr) = &msg.content {
            for item in arr {
                total += count_content_block(item);
            }
        }
    }

    // 工具定义
    if let Some(ref tools) = tools {
        for tool in tools {
            total += count_tokens(&tool.name);
            total += count_tokens(&tool.description);
            let input_schema_json = serde_json::to_string(&tool.input_schema).unwrap_or_default();
            total += count_tokens(&input_schema_json);
        }
    }

    total.max(1)
}

/// 估算"缓存前缀"token 数：system + tools + history 中除最后一条 user 之外的全部内容。
///
/// 用于 cache_read 派生：Anthropic 协议下 prompt cache 仅覆盖前缀，
/// 当前 user turn 不进缓存。tools 跨请求基本稳定，恒视为前缀（即使首条请求也不计入新输入）。
///
/// 估算口径与 `count_all_tokens_local` 完全一致（同一 `count_tokens` 加权公式），
/// 保证 `prefix.min(input_total)` 不会因口径差异溢出。
pub(crate) fn count_prefix_tokens(
    system: Option<&[SystemMessage]>,
    prior_messages: &[Message],
    tools: Option<&[Tool]>,
) -> u64 {
    let mut total = 0;

    if let Some(system) = system {
        for msg in system {
            total += count_tokens(&msg.text);
        }
    }

    for msg in prior_messages {
        if let serde_json::Value::String(s) = &msg.content {
            total += count_tokens(s);
        } else if let serde_json::Value::Array(arr) = &msg.content {
            for item in arr {
                total += count_content_block(item);
            }
        }
    }

    if let Some(tools) = tools {
        for tool in tools {
            total += count_tokens(&tool.name);
            total += count_tokens(&tool.description);
            let input_schema_json = serde_json::to_string(&tool.input_schema).unwrap_or_default();
            total += count_tokens(&input_schema_json);
        }
    }

    total
}

#[cfg(test)]
mod tests {
    use super::*;

    // 新公式（四分类加权）测试：
    //   ASCII 字母 / 4.5
    //   数字 / 2.0
    //   其他 ASCII (符号、空白) / 1.5
    //   非 ASCII (CJK 等) / 1.5

    #[test]
    fn test_count_tokens_hello_world() {
        // "Hello world" = 10 字母 + 1 空格
        // = 10/4.5 + 1/1.5 = 2.222 + 0.667 = 2.889 → ceil = 3
        assert_eq!(count_tokens("Hello world"), 3);
    }

    #[test]
    fn test_count_tokens_400_letters() {
        // 400 个 ASCII 字母 = 400/4.5 = 88.89 → ceil = 89
        let text = "a".repeat(400);
        assert_eq!(count_tokens(&text), 89);
    }

    #[test]
    fn test_count_tokens_4000_letters() {
        // 4000 个 ASCII 字母 = 4000/4.5 = 888.89 → ceil = 889
        let text = "a".repeat(4000);
        assert_eq!(count_tokens(&text), 889);
    }

    #[test]
    fn test_count_tokens_chinese() {
        // 4 个 CJK = 4/1.5 = 2.67 → ceil = 3
        assert_eq!(count_tokens("你好世界"), 3);
    }

    // ---------- B4 新增覆盖 ----------

    #[test]
    fn test_count_tokens_1000_letters_range() {
        let text = "a".repeat(1000);
        let result = count_tokens(&text);
        // 1000/4.5 = 222.2 → ceil = 223
        assert!((200..=240).contains(&result), "got {}", result);
    }

    #[test]
    fn test_count_tokens_1000_digits_range() {
        let text = "1".repeat(1000);
        let result = count_tokens(&text);
        // 1000/2.0 = 500
        assert!((480..=520).contains(&result), "got {}", result);
    }

    #[test]
    fn test_count_tokens_100_symbols_range() {
        let text = "!".repeat(100);
        let result = count_tokens(&text);
        // 100/1.5 = 66.7 → ceil = 67
        assert!((60..=80).contains(&result), "got {}", result);
    }

    #[test]
    fn test_count_tokens_1000_cjk_range() {
        let text = "中".repeat(1000);
        let result = count_tokens(&text);
        // 1000/1.5 = 666.7 → ceil = 667
        assert!((660..=700).contains(&result), "got {}", result);
    }

    #[test]
    fn test_count_tokens_empty_string_min_1() {
        assert_eq!(count_tokens(""), 1);
    }

    #[test]
    fn test_count_tokens_single_char_min_1() {
        assert_eq!(count_tokens("a"), 1);
    }

    #[test]
    fn test_count_tokens_mixed() {
        // 10 字母 + 5 数字 + 3 符号 + 2 CJK
        // = 10/4.5 + 5/2.0 + 3/1.5 + 2/1.5
        // = 2.222 + 2.5 + 2.0 + 1.333 = 8.056 → ceil = 9
        let text = "abcdefghij12345!@#中文";
        assert_eq!(count_tokens(text), 9);
    }

    // ---------- content block 覆盖（tool_use / tool_result） ----------

    #[test]
    fn test_count_content_block_text() {
        let block = serde_json::json!({"type": "text", "text": "Hello world"});
        assert_eq!(count_content_block(&block), 3);
    }

    #[test]
    fn test_count_content_block_tool_use() {
        // name="Read" + input={"file_path":"/foo/bar.txt"}
        // 应包含 name 和 input JSON 序列化两部分,合计远大于 1
        let block = serde_json::json!({
            "type": "tool_use",
            "id": "toolu_x",
            "name": "Read",
            "input": {"file_path": "/foo/bar.txt"}
        });
        let n = count_content_block(&block);
        assert!(n > 5, "tool_use block 应计入 name + input JSON, got {}", n);
    }

    #[test]
    fn test_count_content_block_tool_result_string() {
        let block = serde_json::json!({
            "type": "tool_result",
            "tool_use_id": "toolu_x",
            "content": "Hello world"
        });
        // "Hello world" 估算 = 3
        assert_eq!(count_content_block(&block), 3);
    }

    #[test]
    fn test_count_content_block_tool_result_array() {
        let block = serde_json::json!({
            "type": "tool_result",
            "tool_use_id": "toolu_x",
            "content": [
                {"type": "text", "text": "Hello world"},
                {"type": "text", "text": "你好世界"}
            ]
        });
        // 3 + 3 = 6
        assert_eq!(count_content_block(&block), 6);
    }

    #[test]
    fn test_count_content_block_tool_use_missing_name() {
        // 缺 name:仅计入 input
        let block = serde_json::json!({
            "type": "tool_use",
            "id": "toolu_x",
            "input": {"file_path": "/foo/bar.txt"}
        });
        let n = count_content_block(&block);
        assert!(n > 0, "缺 name 时应仅计 input,got {}", n);
    }

    #[test]
    fn test_count_content_block_tool_use_missing_input() {
        // 缺 input:仅计入 name
        let block = serde_json::json!({
            "type": "tool_use",
            "id": "toolu_x",
            "name": "Read"
        });
        let n = count_content_block(&block);
        // "Read" = 4 字母 / 4.5 = 0.89 → ceil = 1
        assert_eq!(n, 1);
    }

    #[test]
    fn test_count_content_block_tool_result_null_content() {
        // content 为 null:返回 0,不 panic
        let block = serde_json::json!({
            "type": "tool_result",
            "tool_use_id": "toolu_x",
            "content": null
        });
        assert_eq!(count_content_block(&block), 0);
    }

    #[test]
    fn test_count_prefix_tokens_with_tool_messages() {
        use crate::anthropic::types::Message;
        // 仅 text 基线
        let baseline_msgs = vec![Message {
            role: "user".into(),
            content: serde_json::json!([{"type": "text", "text": "hi"}]),
        }];
        let baseline = count_prefix_tokens(None, &baseline_msgs, None);

        // 含 tool_use + tool_result 的对话
        let with_tools = vec![
            Message {
                role: "user".into(),
                content: serde_json::json!([{"type": "text", "text": "hi"}]),
            },
            Message {
                role: "assistant".into(),
                content: serde_json::json!([{
                    "type": "tool_use",
                    "id": "toolu_1",
                    "name": "Read",
                    "input": {"file_path": "/very/long/path/to/file.txt"}
                }]),
            },
            Message {
                role: "user".into(),
                content: serde_json::json!([{
                    "type": "tool_result",
                    "tool_use_id": "toolu_1",
                    "content": "file contents with some words and numbers 12345"
                }]),
            },
        ];
        let extended = count_prefix_tokens(None, &with_tools, None);

        assert!(
            extended > baseline,
            "含 tool_use+tool_result 的前缀估算应显著大于纯 text 基线: extended={}, baseline={}",
            extended,
            baseline
        );
    }
}

/// 估算输出 tokens
pub(crate) fn estimate_output_tokens(content: &[serde_json::Value]) -> i32 {
    let mut total = 0;

    for block in content {
        if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
            total += count_tokens(text) as i32;
        }
        if block.get("type").and_then(|v| v.as_str()) == Some("tool_use") {
            // 工具调用开销
            if let Some(input) = block.get("input") {
                let input_str = serde_json::to_string(input).unwrap_or_default();
                total += count_tokens(&input_str) as i32;
            }
        }
    }

    total.max(1)
}
