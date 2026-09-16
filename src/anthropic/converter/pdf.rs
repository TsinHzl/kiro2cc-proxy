// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! PDF 文本提取（未压缩 Tj/TJ 文本对象）

use base64::Engine;

/// 提取简单文本型 PDF 中的文本。覆盖 hvoy 与常见探针使用的未压缩 Tj/TJ 文本对象。
pub(super) fn extract_pdf_text_from_base64(data: &str) -> Option<String> {
    let data = data
        .rsplit_once(',')
        .map(|(_, tail)| tail)
        .unwrap_or(data)
        .trim();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .ok()?;
    extract_pdf_text_from_bytes(&bytes)
}

fn extract_pdf_text_from_bytes(bytes: &[u8]) -> Option<String> {
    let pdf = String::from_utf8_lossy(bytes);
    let mut texts = Vec::new();
    let bytes = pdf.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] != b'(' {
            i += 1;
            continue;
        }

        let Some((raw, next)) = parse_pdf_literal_string(&pdf, i) else {
            i += 1;
            continue;
        };
        i = next;

        let lookahead_end = (i + 32).min(bytes.len());
        let lookahead = &bytes[i..lookahead_end];
        if lookahead.windows(2).any(|w| w == b"Tj" || w == b"TJ") || lookahead.contains(&b'\'') {
            let text = raw.trim();
            if !text.is_empty() {
                texts.push(text.to_string());
            }
        }
    }

    if texts.is_empty() {
        None
    } else {
        Some(texts.join("\n"))
    }
}

fn parse_pdf_literal_string(pdf: &str, start: usize) -> Option<(String, usize)> {
    let bytes = pdf.as_bytes();
    if bytes.get(start) != Some(&b'(') {
        return None;
    }

    let mut out = String::new();
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                i += 1;
                if i >= bytes.len() {
                    break;
                }
                match bytes[i] {
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'b' => out.push('\u{0008}'),
                    b'f' => out.push('\u{000c}'),
                    b'(' => out.push('('),
                    b')' => out.push(')'),
                    b'\\' => out.push('\\'),
                    b'\n' | b'\r' => {}
                    c if (b'0'..=b'7').contains(&c) => {
                        let mut octal = vec![c];
                        for _ in 0..2 {
                            if i + 1 < bytes.len() && (b'0'..=b'7').contains(&bytes[i + 1]) {
                                i += 1;
                                octal.push(bytes[i]);
                            } else {
                                break;
                            }
                        }
                        if let Ok(value) =
                            u8::from_str_radix(std::str::from_utf8(&octal).unwrap_or_default(), 8)
                        {
                            out.push(value as char);
                        }
                    }
                    other => out.push(other as char),
                }
            }
            b')' => return Some((out, i + 1)),
            other => out.push(other as char),
        }
        i += 1;
    }

    None
}
