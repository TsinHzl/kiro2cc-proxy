// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 文件下载响应：把一段 CSV 文本包装成带 `Content-Disposition: attachment` 的 HTTP 响应。

use axum::{
    http::{HeaderMap, HeaderValue, header},
    response::{IntoResponse, Response},
};

/// 构造 CSV 附件响应。
///
/// `filename` 由调用方生成，只允许 ASCII 字母、数字与 `-_.`（调用点均为日期 / 数字 ID / 时间戳拼接）；
/// 含其他字符时退化为通用文件名，避免注入响应头。
pub fn csv_attachment(filename: &str, body: String) -> Response {
    let safe = !filename.is_empty()
        && filename
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    let filename = if safe { filename } else { "download.csv" };

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
            .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
    );
    // 内容含用户用量数据，禁止中间层缓存
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    (headers, body).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_csv_attachment_sets_headers() {
        let resp = csv_attachment("usage-logs-1.csv", "a,b\r\n".to_string());
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/csv; charset=utf-8"
        );
        assert_eq!(
            resp.headers().get(header::CONTENT_DISPOSITION).unwrap(),
            "attachment; filename=\"usage-logs-1.csv\""
        );
    }

    #[test]
    fn test_csv_attachment_rejects_unsafe_filename() {
        let resp = csv_attachment("a\"; x=\r\nInjected: 1", String::new());
        assert_eq!(
            resp.headers().get(header::CONTENT_DISPOSITION).unwrap(),
            "attachment; filename=\"download.csv\""
        );
    }
}
