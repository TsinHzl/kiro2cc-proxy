//! Claude Code Read 图片工具结果的回归测试；只检验协议转换，不请求上游。

use super::super::convert::convert_request_with_chunking;
use super::super::message::process_message_content;
use crate::anthropic::types::MessagesRequest;
use crate::kiro::model::requests::conversation::Message;
use crate::kiro::model::requests::tool::ToolResult;
use serde_json::{Value, json};

// 转换器把 base64 当作不透明载荷转发，使用不同载荷辨别图片是否串位。
fn image(media_type: &str, data: &str) -> Value {
    json!({
        "type": "image",
        "source": {"type": "base64", "media_type": media_type, "data": data}
    })
}

fn tool_result(id: &str, content: Value) -> Value {
    json!({"type": "tool_result", "tool_use_id": id, "content": content})
}

fn result_text(result: &ToolResult) -> String {
    result
        .content
        .iter()
        .map(|block| block.get("text").and_then(Value::as_str).unwrap())
        .collect::<Vec<_>>()
        .join("\n")
}

// n 是该图片在整条 user 消息 images 里的位置（从 1 开始，含顶层图片）。
fn image_marker(id: &str, n: usize) -> String {
    format!("[Image {n} attached from tool_result {id}]")
}

// 图片未能转发时的占位，不含任何图片载荷。
fn omitted_marker(id: &str) -> String {
    format!("[Image omitted from tool_result {id}: unsupported or invalid image]")
}

#[test]
fn mixed_text_and_image_tool_result_preserves_text_and_image_position() {
    let content = json!([
        {"type": "text", "text": "Check this screenshot."},
        tool_result("read_mixed", json!([
            {"type": "text", "text": "Screenshot begins"},
            image("image/png", "c2NyZWVuc2hvdA=="),
            {"type": "text", "text": "Screenshot ends"}
        ]))
    ]);

    let (text, images, results) = process_message_content(&content).unwrap();

    assert_eq!(text, "Check this screenshot.");
    assert_eq!(images.len(), 1, "工具图片应进入同一条 user 消息的 images");
    assert_eq!(images[0].format, "png");
    assert_eq!(images[0].source.bytes, "c2NyZWVuc2hvdA==");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].tool_use_id, "read_mixed");
    assert_eq!(
        result_text(&results[0]),
        format!(
            "Screenshot begins\n{}\nScreenshot ends",
            image_marker("read_mixed", 1)
        )
    );
    assert_eq!(results[0].status.as_deref(), Some("success"));
    assert!(!results[0].is_error);
}

#[test]
fn image_only_tool_result_remains_nonempty_and_paired() {
    let content = json!([tool_result(
        "read_only",
        json!([image("image/jpeg", "anBlZy1vbmx5")])
    )]);

    let (_, images, results) = process_message_content(&content).unwrap();

    assert_eq!(images.len(), 1);
    assert_eq!(images[0].format, "jpeg");
    assert_eq!(images[0].source.bytes, "anBlZy1vbmx5");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].tool_use_id, "read_only");
    assert_eq!(result_text(&results[0]), image_marker("read_only", 1));
}

#[test]
fn tool_result_with_image_keeps_error_status() {
    let content = json!([{
        "type": "tool_result", "tool_use_id": "read_error", "is_error": true,
        "content": [
            {"type": "text", "text": "Partial screenshot; capture failed."},
            image("image/png", "cGFydGlhbA==")
        ]
    }]);

    let (_, images, results) = process_message_content(&content).unwrap();

    assert_eq!(images.len(), 1);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].tool_use_id, "read_error");
    assert!(results[0].is_error);
    assert_eq!(results[0].status.as_deref(), Some("error"));
    assert_eq!(
        result_text(&results[0]),
        format!(
            "Partial screenshot; capture failed.\n{}",
            image_marker("read_error", 1)
        )
    );
}

#[test]
fn multiple_tool_images_and_top_level_images_keep_original_order() {
    let content = json!([
        image("image/png", "dG9wLTE="),
        tool_result(
            "read_a",
            json!([
                image("image/jpeg", "dG9vbC0y"),
                {"type": "text", "text": "Between the two tool images"},
                image("image/webp", "dG9vbC0z")
            ])
        ),
        image("image/gif", "dG9wLTQ="),
        tool_result("read_b", json!([image("image/png", "dG9vbC01")]))
    ]);

    let (_, images, results) = process_message_content(&content).unwrap();

    let payloads = images
        .iter()
        .map(|image| image.source.bytes.as_str())
        .collect::<Vec<_>>();
    let formats = images
        .iter()
        .map(|image| image.format.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        payloads,
        ["dG9wLTE=", "dG9vbC0y", "dG9vbC0z", "dG9wLTQ=", "dG9vbC01"]
    );
    assert_eq!(formats, ["png", "jpeg", "webp", "gif", "png"]);
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].tool_use_id, "read_a");
    assert_eq!(results[1].tool_use_id, "read_b");
    assert_eq!(result_text(&results[1]), image_marker("read_b", 5));
    assert!(!result_text(&results[0]).contains("read_b"));
    assert_eq!(
        result_text(&results[0]),
        format!(
            "{}\nBetween the two tool images\n{}",
            image_marker("read_a", 2),
            image_marker("read_a", 3)
        )
    );
}

#[test]
fn historical_and_current_tool_images_stay_with_matching_tool_ids() {
    let req: MessagesRequest = serde_json::from_value(json!({
        "model": "claude-sonnet-4-6", "max_tokens": 32,
        "messages": [
            {"role": "user", "content": "Compare the first screenshot with the next one."},
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "read_history", "name": "Read", "input": {"file_path": "/tmp/first.png"}}
            ]},
            {"role": "user", "content": [tool_result("read_history", json!([
                image("image/png", "aGlzdG9yeQ==")
            ]))]},
            {"role": "assistant", "content": [
                {"type": "text", "text": "I have the first screenshot."},
                {"type": "tool_use", "id": "read_current", "name": "Read", "input": {"file_path": "/tmp/next.png"}}
            ]},
            {"role": "user", "content": [tool_result("read_current", json!([
                image("image/jpeg", "Y3VycmVudA==")
            ]))]}
        ]
    })).unwrap();

    let state = convert_request_with_chunking(&req, 0)
        .unwrap()
        .conversation_state;
    let historical_result_messages = state
        .history
        .iter()
        .filter_map(|message| match message {
            Message::User(message)
                if !message
                    .user_input_message
                    .user_input_message_context
                    .tool_results
                    .is_empty() =>
            {
                Some(&message.user_input_message)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(historical_result_messages.len(), 1);
    let historical = historical_result_messages[0];
    assert_eq!(historical.images.len(), 1);
    assert_eq!(historical.images[0].source.bytes, "aGlzdG9yeQ==");
    let historical_results = &historical.user_input_message_context.tool_results;
    assert_eq!(historical_results.len(), 1);
    assert_eq!(historical_results[0].tool_use_id, "read_history");
    assert_eq!(
        result_text(&historical_results[0]),
        image_marker("read_history", 1)
    );

    let current = &state.current_message.user_input_message;
    assert_eq!(current.images.len(), 1);
    assert_eq!(current.images[0].source.bytes, "Y3VycmVudA==");
    let current_results = &current.user_input_message_context.tool_results;
    assert_eq!(
        current_results.len(),
        1,
        "当前 tool_result 不应被孤立结果过滤器删除"
    );
    assert_eq!(current_results[0].tool_use_id, "read_current");
    assert_eq!(
        result_text(&current_results[0]),
        image_marker("read_current", 1)
    );

    let tool_ids = state
        .history
        .iter()
        .filter_map(|message| match message {
            Message::Assistant(message) => message.assistant_response_message.tool_uses.as_deref(),
            _ => None,
        })
        .flatten()
        .map(|tool| tool.tool_use_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(tool_ids, ["read_history", "read_current"]);
}

#[test]
fn orphaned_tool_images_are_removed_without_removing_top_level_images() {
    let req: MessagesRequest = serde_json::from_value(json!({
        "model": "claude-sonnet-4-6", "max_tokens": 32,
        "messages": [{"role": "user", "content": [
            {"type": "text", "text": "Inspect the directly attached screenshots."},
            image("image/png", "dG9wLWJlZm9yZQ=="),
            tool_result("orphan_read", json!([
                image("image/jpeg", "b3JwaGFu")
            ])),
            image("image/webp", "dG9wLWFmdGVy")
        ]}]
    }))
    .unwrap();

    let state = convert_request_with_chunking(&req, 0)
        .unwrap()
        .conversation_state;
    let current = &state.current_message.user_input_message;

    assert!(current.user_input_message_context.tool_results.is_empty());
    let payloads = current
        .images
        .iter()
        .map(|image| image.source.bytes.as_str())
        .collect::<Vec<_>>();
    assert_eq!(payloads, ["dG9wLWJlZm9yZQ==", "dG9wLWFmdGVy"]);
    assert_eq!(
        current.content,
        "Inspect the directly attached screenshots."
    );
}

#[test]
fn consumed_tool_image_is_not_resent_and_historical_image_is_preserved() {
    let req: MessagesRequest = serde_json::from_value(json!({
        "model": "claude-sonnet-4-6", "max_tokens": 32,
        "messages": [
            {"role": "user", "content": "Read the first screenshot."},
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "read_consumed", "name": "Read", "input": {"file_path": "/tmp/first.png"}}
            ]},
            {"role": "user", "content": [tool_result("read_consumed", json!([
                image("image/png", "aGlzdG9yaWNhbA==")
            ]))]},
            {"role": "assistant", "content": "The first screenshot is already processed."},
            {"role": "user", "content": [
                image("image/jpeg", "ZGlyZWN0"),
                tool_result("read_consumed", json!([
                    image("image/png", "ZHVwbGljYXRl")
                ]))
            ]}
        ]
    })).unwrap();

    let state = convert_request_with_chunking(&req, 0)
        .unwrap()
        .conversation_state;
    let current = &state.current_message.user_input_message;
    assert!(current.user_input_message_context.tool_results.is_empty());
    assert_eq!(current.images.len(), 1);
    assert_eq!(current.images[0].source.bytes, "ZGlyZWN0");

    let historical = state
        .history
        .iter()
        .filter_map(|message| match message {
            Message::User(message)
                if message
                    .user_input_message
                    .user_input_message_context
                    .tool_results
                    .iter()
                    .any(|result| result.tool_use_id == "read_consumed") =>
            {
                Some(&message.user_input_message)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(historical.len(), 1);
    assert_eq!(historical[0].images.len(), 1);
    assert_eq!(historical[0].images[0].source.bytes, "aGlzdG9yaWNhbA==");
    let historical_results = &historical[0].user_input_message_context.tool_results;
    assert_eq!(historical_results.len(), 1);
    assert_eq!(historical_results[0].tool_use_id, "read_consumed");
    assert_eq!(
        result_text(&historical_results[0]),
        image_marker("read_consumed", 1)
    );
}

#[test]
fn repeated_current_tool_id_keeps_only_first_images_and_preserves_other_tool_order() {
    let req: MessagesRequest = serde_json::from_value(json!({
        "model": "claude-sonnet-4-6", "max_tokens": 32,
        "messages": [
            {"role": "user", "content": "Read the two screenshots."},
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "read_first", "name": "Read", "input": {"file_path": "/tmp/first.png"}},
                {"type": "tool_use", "id": "read_second", "name": "Read", "input": {"file_path": "/tmp/second.png"}}
            ]},
            {"role": "user", "content": [
                image("image/png", "dG9w"),
                tool_result("read_first", json!([
                    {"type": "text", "text": "Accepted first result"},
                    image("image/jpeg", "Zmlyc3Q=")
                ])),
                tool_result("read_first", json!([
                    {"type": "text", "text": "Rejected duplicate result"},
                    image("image/png", "ZHVwbGljYXRl")
                ])),
                tool_result("read_second", json!([
                    image("image/webp", "c2Vjb25k")
                ])),
                image("image/gif", "YWZ0ZXI=")
            ]}
        ]
    })).unwrap();

    let state = convert_request_with_chunking(&req, 0)
        .unwrap()
        .conversation_state;
    let current = &state.current_message.user_input_message;
    let payloads = current
        .images
        .iter()
        .map(|image| image.source.bytes.as_str())
        .collect::<Vec<_>>();
    assert_eq!(payloads, ["dG9w", "Zmlyc3Q=", "c2Vjb25k", "YWZ0ZXI="]);

    let results = &current.user_input_message_context.tool_results;
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].tool_use_id, "read_first");
    assert_eq!(results[1].tool_use_id, "read_second");
    assert_eq!(
        result_text(&results[0]),
        format!("Accepted first result\n{}", image_marker("read_first", 2))
    );
    assert_eq!(result_text(&results[1]), image_marker("read_second", 3));
}

#[test]
fn ordinary_text_tool_results_are_unchanged() {
    let content = json!([
        tool_result("string_result", json!("plain\ntext")),
        tool_result(
            "blocks_result",
            json!([
                {"type": "text", "text": "first"},
                {"type": "text", "text": "second"}
            ])
        )
    ]);

    let (text, images, results) = process_message_content(&content).unwrap();

    assert!(text.is_empty());
    assert!(images.is_empty());
    assert_eq!(results.len(), 2);
    assert_eq!(
        serde_json::to_value(&results[0]).unwrap(),
        serde_json::to_value(ToolResult::success("string_result", "plain\ntext")).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&results[1]).unwrap(),
        serde_json::to_value(ToolResult::success("blocks_result", "first\nsecond")).unwrap()
    );
}

#[test]
fn unsupported_or_malformed_images_do_not_claim_successful_attachment() {
    let content = json!([tool_result(
        "read_unsupported",
        json!([
            {"type": "text", "text": "Keep this diagnostic text."},
            image("image/svg+xml", "PHN2Zy8+"),
            image("image/png", ""),
            {"type": "image", "source": {"type": "url", "media_type": "image/png", "data": "bm90LWJhc2U2NC1zb3VyY2U="}},
            {"type": "image", "source": {"type": "base64", "media_type": "image/png"}},
            {"type": "image", "source": null},
            {"type": "image", "source": "data:image/png;base64,cHJpdmF0ZS1maXh0dXJl"}
        ])
    )]);

    let (_, images, results) = process_message_content(&content).unwrap();

    assert!(images.is_empty());
    assert_eq!(results.len(), 1);
    let text = result_text(&results[0]);
    assert!(text.contains("Keep this diagnostic text."));
    assert!(
        !text.contains("attached from tool_result"),
        "未发送图片时不能声称已经附图"
    );
    // 6 张无法转发的图片各留一行占位，让模型知道这里本该有图；载荷不得出现在文本里。
    assert_eq!(text.matches(&omitted_marker("read_unsupported")).count(), 6);
    assert!(!text.contains("cHJpdmF0ZS1maXh0dXJl"));
}

#[test]
fn image_only_tool_result_with_unsupported_image_is_not_empty() {
    let content = json!([tool_result(
        "read_svg",
        json!([image("image/svg+xml", "PHN2Zy8+")])
    )]);

    let (_, images, results) = process_message_content(&content).unwrap();

    assert!(images.is_empty());
    assert_eq!(results.len(), 1);
    assert_eq!(result_text(&results[0]), omitted_marker("read_svg"));
}

#[test]
fn valid_and_invalid_images_number_only_the_attached_ones() {
    let content = json!([tool_result(
        "read_partial",
        json!([
            image("image/png", "b25l"),
            image("image/svg+xml", "PHN2Zy8+"),
            image("image/jpeg", "dHdv")
        ])
    )]);

    let (_, images, results) = process_message_content(&content).unwrap();

    assert_eq!(images.len(), 2);
    assert_eq!(
        result_text(&results[0]),
        format!(
            "{}\n{}\n{}",
            image_marker("read_partial", 1),
            omitted_marker("read_partial"),
            image_marker("read_partial", 2)
        )
    );
}

#[test]
fn parallel_tool_results_get_message_wide_image_numbers() {
    // 并行读多张图时，每个工具结果各自从 1 编号会让模型无法把图对应到工具结果。
    let content = json!([
        tool_result("read_a", json!([image("image/png", "YQ==")])),
        tool_result("read_b", json!([image("image/png", "Yg==")])),
        tool_result("read_c", json!([image("image/png", "Yw==")]))
    ]);

    let (_, images, results) = process_message_content(&content).unwrap();

    assert_eq!(images.len(), 3);
    assert_eq!(result_text(&results[0]), image_marker("read_a", 1));
    assert_eq!(result_text(&results[1]), image_marker("read_b", 2));
    assert_eq!(result_text(&results[2]), image_marker("read_c", 3));
}

#[test]
fn orphaned_tool_image_does_not_shift_numbers_of_kept_images() {
    // 孤立结果的图片被丢弃后，保留下来的结果序号必须等于它在最终 images 里的位置。
    let req: MessagesRequest = serde_json::from_value(json!({
        "model": "claude-sonnet-4-6", "max_tokens": 32,
        "messages": [
            {"role": "user", "content": "Read the screenshot."},
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "read_ok", "name": "Read", "input": {"file_path": "/tmp/ok.png"}}
            ]},
            {"role": "user", "content": [
                tool_result("orphan_read", json!([image("image/png", "b3JwaGFu")])),
                tool_result("read_ok", json!([image("image/jpeg", "a2VwdA==")]))
            ]}
        ]
    }))
    .unwrap();

    let state = convert_request_with_chunking(&req, 0)
        .unwrap()
        .conversation_state;
    let current = &state.current_message.user_input_message;

    assert_eq!(current.images.len(), 1);
    assert_eq!(current.images[0].source.bytes, "a2VwdA==");
    let results = &current.user_input_message_context.tool_results;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].tool_use_id, "read_ok");
    assert_eq!(result_text(&results[0]), image_marker("read_ok", 1));
}

#[test]
fn merged_history_user_messages_continue_image_numbers() {
    // 连续的 user 消息合并成一条 Kiro 消息时，序号要接着前一条继续，而不是各自从 1 开始。
    let messages: Vec<crate::anthropic::types::Message> = serde_json::from_value(json!([
        {"role": "user", "content": [tool_result("read_a", json!([image("image/png", "YQ==")]))]},
        {"role": "user", "content": [
            image("image/png", "dG9w"),
            tool_result("read_b", json!([image("image/png", "Yg==")]))
        ]}
    ]))
    .unwrap();
    let refs = messages.iter().map(|m| (m, None)).collect::<Vec<_>>();

    let merged = super::super::history::merge_user_messages(&refs, "claude-sonnet-4-6")
        .unwrap()
        .user_input_message;

    assert_eq!(merged.images.len(), 3);
    let results = &merged.user_input_message_context.tool_results;
    assert_eq!(result_text(&results[0]), image_marker("read_a", 1));
    assert_eq!(result_text(&results[1]), image_marker("read_b", 3));
}
