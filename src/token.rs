// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Token 计算模块
//!
//! 使用 tiktoken-rs cl100k_base BPE 编码器估算 token 数量。
//! 编码器在首次调用时初始化（约 50ms），之后全局缓存复用。

use crate::anthropic::types::{
    CountTokensRequest, CountTokensResponse, Message, SystemMessage, Tool,
};
use crate::http_client::{ProxyConfig, build_client};
use crate::model::config::TlsBackend;
use std::sync::OnceLock;
use tiktoken_rs::CoreBPE;

fn get_bpe() -> &'static CoreBPE {
    tiktoken_rs::cl100k_base_singleton()
}

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

/// 计算文本的 token 数量（tiktoken cl100k_base BPE）
pub fn count_tokens(text: &str) -> u64 {
    if text.is_empty() {
        return 0;
    }
    get_bpe().encode_with_special_tokens(text).len() as u64
}

/// 尝试调用远程 count_tokens API，未配置或调用失败时返回 `None`（由调用方回退到本地计算）
fn try_remote_count_tokens(
    model: &str,
    system: &Option<Vec<SystemMessage>>,
    messages: &[Message],
    tools: &Option<Vec<Tool>>,
) -> Option<u64> {
    let config = get_config()?;
    let api_url = config.api_url.as_ref()?;

    let result = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(call_remote_count_tokens(
            api_url,
            config,
            model.to_string(),
            system,
            messages,
            tools,
        ))
    });

    match result {
        Ok(tokens) => {
            tracing::debug!("远程 count_tokens API 返回: {}", tokens);
            Some(tokens)
        }
        Err(e) => {
            tracing::warn!("远程 count_tokens API 调用失败，回退到本地计算: {}", e);
            None
        }
    }
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
    if let Some(tokens) = try_remote_count_tokens(&model, &system, &messages, &tools) {
        return tokens;
    }

    // 本地计算
    count_all_tokens_local(system, messages, tools)
}

/// 估算请求的输入 tokens，复用调用方已算好的前缀（system + tools + 除最后一条消息外的历史）token 数，
/// 避免与 `count_prefix_tokens` 重复对同一段内容做 BPE 编码。
///
/// 远程 API 路径与 `count_all_tokens` 完全一致；仅本地回退路径改为
/// `prefix_tokens + 最后一条消息的 token 数`，与 `count_all_tokens_local` 数学等价
/// （二者对 system/messages/tools 的遍历口径完全相同，唯一差异是历史消息部分改为复用已算好的值）。
/// `messages` 为空时视为 0（此时 `prefix_tokens` 本身已是 system+tools 全部 token）。
///
/// **契约：** `prefix_tokens` 必须由 `count_prefix_tokens(system, &messages[..len-1], tools)`
/// 产出；传入手工构造的值将破坏与 `count_all_tokens_local` 的数学等价性。
pub(crate) fn count_all_tokens_with_prefix(
    model: String,
    system: Option<Vec<SystemMessage>>,
    messages: Vec<Message>,
    tools: Option<Vec<Tool>>,
    prefix_tokens: u64,
) -> u64 {
    if let Some(tokens) = try_remote_count_tokens(&model, &system, &messages, &tools) {
        return tokens;
    }

    let last_message_tokens = messages.last().map(count_message_tokens).unwrap_or(0);
    (prefix_tokens + last_message_tokens).max(1)
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

/// 统计单条消息的 token 数（`content` 为字符串直接计数，为数组则逐 block 计数）
fn count_message_tokens(msg: &Message) -> u64 {
    let mut total = 0;
    if let serde_json::Value::String(s) = &msg.content {
        total += count_tokens(s);
    } else if let serde_json::Value::Array(arr) = &msg.content {
        for item in arr {
            total += count_content_block(item);
        }
    }
    total
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
        total += count_message_tokens(msg);
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
        total += count_message_tokens(msg);
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

    // tiktoken cl100k_base BPE 真实值测试

    #[test]
    fn test_count_tokens_hello_world() {
        // cl100k_base: "Hello" + " world" = 2 tokens
        assert_eq!(count_tokens("Hello world"), 2);
    }

    #[test]
    fn test_count_tokens_400_letters() {
        // 400 个 'a' 在 cl100k_base 下 BPE 合并为约 50 tokens
        let text = "a".repeat(400);
        assert_eq!(count_tokens(&text), 50);
    }

    #[test]
    fn test_count_tokens_4000_letters() {
        // 4000 个 'a' 在 cl100k_base 下约 500 tokens
        let text = "a".repeat(4000);
        assert_eq!(count_tokens(&text), 500);
    }

    #[test]
    fn test_count_tokens_chinese() {
        // "你好世界" = 5 tokens（cl100k_base 真实值）
        assert_eq!(count_tokens("你好世界"), 5);
    }

    #[test]
    fn test_count_tokens_1000_letters_range() {
        let text = "a".repeat(1000);
        let result = count_tokens(&text);
        // cl100k_base: 约 125 tokens
        assert!((100..=150).contains(&result), "got {}", result);
    }

    #[test]
    fn test_count_tokens_1000_digits_range() {
        let text = "1".repeat(1000);
        let result = count_tokens(&text);
        // cl100k_base: 数字 BPE 粒度细，约 334 tokens
        assert!((300..=400).contains(&result), "got {}", result);
    }

    #[test]
    fn test_count_tokens_100_symbols_range() {
        let text = "!".repeat(100);
        let result = count_tokens(&text);
        // cl100k_base: "!" 通常独立成 token，约 13 tokens（BPE 会合并连续符号）
        assert!((10..=20).contains(&result), "got {}", result);
    }

    #[test]
    fn test_count_tokens_1000_cjk_range() {
        let text = "中".repeat(1000);
        let result = count_tokens(&text);
        // cl100k_base: 每个 CJK 通常对应 1-3 个字节序列 token，约 1000 tokens
        assert!((950..=1050).contains(&result), "got {}", result);
    }

    #[test]
    fn test_count_tokens_empty_string_returns_zero() {
        // count_tokens 空字符串返回 0；调用方 count_all_tokens_local 等有 .max(1) 兜底
        assert_eq!(count_tokens(""), 0);
    }

    #[test]
    fn test_count_tokens_single_char_min_1() {
        assert_eq!(count_tokens("a"), 1);
    }

    #[test]
    fn test_count_tokens_mixed() {
        let text = "abcdefghij12345!@#中文";
        let result = count_tokens(text);
        assert!(result > 0, "混合文本 token 数应大于 0, got {}", result);
    }

    // ---------- content block 覆盖（tool_use / tool_result） ----------

    #[test]
    fn test_count_content_block_text() {
        let block = serde_json::json!({"type": "text", "text": "Hello world"});
        // cl100k_base: "Hello world" = 2 tokens
        assert_eq!(count_content_block(&block), 2);
    }

    #[test]
    fn test_count_content_block_tool_use() {
        // name="Read" + input={"file_path":"/foo/bar.txt"}
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
        // cl100k_base: "Hello world" = 2 tokens
        assert_eq!(count_content_block(&block), 2);
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
        // 2 + 5 = 7
        assert_eq!(count_content_block(&block), 7);
    }

    #[test]
    fn test_count_content_block_tool_use_missing_name() {
        let block = serde_json::json!({
            "type": "tool_use",
            "id": "toolu_x",
            "input": {"file_path": "/foo/bar.txt"}
        });
        let n = count_content_block(&block);
        assert!(n > 0, "缺 name 时应仅计 input, got {}", n);
    }

    #[test]
    fn test_count_content_block_tool_use_missing_input() {
        let block = serde_json::json!({
            "type": "tool_use",
            "id": "toolu_x",
            "name": "Read"
        });
        let n = count_content_block(&block);
        // "Read" = 1 token
        assert_eq!(n, 1);
    }

    #[test]
    fn test_count_content_block_tool_result_null_content() {
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
        let baseline_msgs = vec![Message {
            role: "user".into(),
            content: serde_json::json!([{"type": "text", "text": "hi"}]),
        }];
        let baseline = count_prefix_tokens(None, &baseline_msgs, None);

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

    // ---------- count_all_tokens_with_prefix 与 count_all_tokens_local 等价性 ----------

    fn sample_system() -> Option<Vec<SystemMessage>> {
        Some(vec![SystemMessage {
            text: "you are a helpful assistant".into(),
        }])
    }

    fn sample_tools() -> Option<Vec<Tool>> {
        let mut input_schema = std::collections::HashMap::new();
        input_schema.insert("type".to_string(), serde_json::json!("object"));
        input_schema.insert(
            "properties".to_string(),
            serde_json::json!({"file_path": {"type": "string"}}),
        );
        Some(vec![Tool {
            tool_type: None,
            name: "Read".into(),
            description: "读取文件内容".into(),
            input_schema,
            max_uses: None,
            defer_loading: None,
        }])
    }

    fn sample_messages() -> Vec<Message> {
        vec![
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
        ]
    }

    #[test]
    fn test_count_all_tokens_with_prefix_matches_local_combination() {
        let system = sample_system();
        let tools = sample_tools();
        let messages = sample_messages();

        let prior = &messages[..messages.len() - 1];
        let prefix = count_prefix_tokens(system.as_deref(), prior, tools.as_deref());

        let expected = count_all_tokens_local(system.clone(), messages.clone(), tools.clone());
        let actual = count_all_tokens_with_prefix(
            "claude-test".into(),
            system,
            messages,
            tools,
            prefix,
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_count_all_tokens_with_prefix_empty_messages() {
        let system = sample_system();
        let tools = sample_tools();
        let messages: Vec<Message> = vec![];

        let prefix = count_prefix_tokens(system.as_deref(), &[], tools.as_deref());

        let expected = count_all_tokens_local(system.clone(), messages.clone(), tools.clone());
        let actual = count_all_tokens_with_prefix(
            "claude-test".into(),
            system,
            messages,
            tools,
            prefix,
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_count_all_tokens_with_prefix_no_system_or_tools() {
        let messages = sample_messages();
        let prior = &messages[..messages.len() - 1];
        let prefix = count_prefix_tokens(None, prior, None);

        let expected = count_all_tokens_local(None, messages.clone(), None);
        let actual =
            count_all_tokens_with_prefix("claude-test".into(), None, messages, None, prefix);

        assert_eq!(actual, expected);
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
