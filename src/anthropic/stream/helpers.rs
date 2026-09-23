//! 流式响应处理模块
//!
//! 实现 Kiro → Anthropic 流式响应转换和 SSE 状态管理
/// 生成伪造的 thinking 签名（长度 >= 100 的 base64 形状字符串）
///
/// 上游不返回真实签名，流式与非流式路径共用这一份伪造实现，保证两端
/// thinking 块结构一致。
pub(crate) fn generate_fake_signature() -> String {
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

/// 按中文 / 非中文分桶统计字符数（不做取整）
///
/// 单独暴露分桶计数，是为了让跨多次调用的累加只在收尾做一次向上取整 ——
/// 每 chunk 各自 ceil 会累积出系统性高估。
pub(crate) fn count_token_chars(text: &str) -> (i64, i64) {
    let mut chinese = 0i64;
    let mut other = 0i64;
    for c in text.chars() {
        if ('\u{4E00}'..='\u{9FFF}').contains(&c) {
            chinese += 1;
        } else {
            other += 1;
        }
    }
    (chinese, other)
}

/// 由分桶字符数派生 token 数：中文约 1.5 字符/token，英文约 4 字符/token
///
/// 两桶皆空时返回 0 —— `is_empty_response` 依赖"零输出"这一状态，
/// 不能像单次估算那样兜底成 1。
pub(crate) fn tokens_from_chars(chinese: i64, other: i64) -> i32 {
    if chinese == 0 && other == 0 {
        return 0;
    }
    (((chinese * 2 + 2) / 3 + (other + 3) / 4) as i32).max(1)
}
