//! responses_response 测试（原内联测试整体迁移）
#![cfg(test)]

use serde_json::{Value, json};
use std::collections::HashSet;

use super::nonstream::*;
use super::stream::*;

#[cfg(test)]
mod tests {
    use super::*;

    /// 绝大多数用例没有 custom 工具声明；本地定义遮蔽 glob 导入的同名函数，
    /// 需要 custom 语义的用例显式调 [`super::convert_non_stream`]
    fn convert_non_stream(anthropic: &Value, client_model: &str) -> Value {
        super::convert_non_stream(anthropic, client_model, &HashSet::new())
    }

    fn custom_names(names: &[&str]) -> HashSet<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    /// 把一帧 SSE 文本拆成 (事件名, data JSON)
    fn parse_frame(frame: &str) -> (String, Value) {
        let mut lines = frame.trim_end().lines();
        let name = lines
            .next()
            .and_then(|l| l.strip_prefix("event: "))
            .expect("缺少 event 行")
            .to_string();
        let data = lines
            .next()
            .and_then(|l| l.strip_prefix("data: "))
            .expect("缺少 data 行");
        (
            name,
            serde_json::from_str(data).expect("data 不是合法 JSON"),
        )
    }

    fn parse_all(frames: &[String]) -> Vec<(String, Value)> {
        frames.iter().map(|f| parse_frame(f)).collect()
    }

    fn event_names(frames: &[String]) -> Vec<String> {
        parse_all(frames).into_iter().map(|(n, _)| n).collect()
    }

    /// 依次喂入事件并收集所有下发帧，末尾自动 finish
    fn run_stream(events: &[(&str, Value)]) -> Vec<String> {
        run_stream_with_custom(events, HashSet::new())
    }

    fn run_stream_with_custom(events: &[(&str, Value)], custom: HashSet<String>) -> Vec<String> {
        let mut conv = ResponsesStreamConverter::new("gpt-5.6-terra", custom);
        let mut frames = Vec::new();
        for (name, data) in events {
            frames.extend(conv.on_event(name, data));
        }
        frames.extend(conv.finish());
        frames
    }

    /// 一轮完整的工具调用块事件（参数分两段发，覆盖增量拼装）
    fn tool_block_events(name: &'static str) -> Vec<(&'static str, Value)> {
        vec![
            (
                "content_block_start",
                json!({"index": 0, "content_block": {"type": "tool_use", "id": "toolu_1", "name": name}}),
            ),
            (
                "content_block_delta",
                json!({"index": 0, "delta": {"type": "input_json_delta", "partial_json": "{\"input\":\"ls "}}),
            ),
            (
                "content_block_delta",
                json!({"index": 0, "delta": {"type": "input_json_delta", "partial_json": "-la\"}"}}),
            ),
            ("content_block_stop", json!({"index": 0})),
            ("message_stop", json!({})),
        ]
    }

    fn text_block_events(text: &str) -> Vec<(&'static str, Value)> {
        vec![
            ("message_start", json!({"message": {"id": "msg_up"}})),
            (
                "content_block_start",
                json!({"index": 0, "content_block": {"type": "text", "text": ""}}),
            ),
            (
                "content_block_delta",
                json!({"index": 0, "delta": {"type": "text_delta", "text": text}}),
            ),
            ("content_block_stop", json!({"index": 0})),
            (
                "message_delta",
                json!({"delta": {"stop_reason": "end_turn"}, "usage": {"input_tokens": 5, "output_tokens": 2}}),
            ),
            ("message_stop", json!({})),
        ]
    }

    #[test]
    fn plain_text_stream_emits_full_event_sequence() {
        let frames = run_stream(&text_block_events("你好"));
        assert_eq!(
            event_names(&frames),
            vec![
                "response.created",
                "response.in_progress",
                "response.output_item.added",
                "response.content_part.added",
                "response.output_text.delta",
                "response.output_text.done",
                "response.content_part.done",
                "response.output_item.done",
                "response.completed",
            ]
        );

        let parsed = parse_all(&frames);
        // sequence_number 从 0 起严格递增
        for (i, (_, data)) in parsed.iter().enumerate() {
            assert_eq!(data["sequence_number"], json!(i as i64));
        }

        let (_, done) = &parsed[5];
        assert_eq!(done["text"], json!("你好"));
        assert_eq!(done["content_index"], json!(0));

        let (_, completed) = parsed.last().unwrap();
        let response = &completed["response"];
        assert_eq!(response["status"], json!("completed"));
        assert_eq!(response["model"], json!("gpt-5.6-terra"));
        assert_eq!(response["usage"]["input_tokens"], json!(5));
        assert_eq!(response["usage"]["total_tokens"], json!(7));
        assert_eq!(response["output"][0]["content"][0]["text"], json!("你好"));
    }

    #[test]
    fn created_snapshot_has_no_output_or_usage_yet() {
        let frames = run_stream(&text_block_events("hi"));
        let (name, created) = parse_frame(&frames[0]);
        assert_eq!(name, "response.created");
        assert_eq!(created["response"]["status"], json!("in_progress"));
        assert_eq!(created["response"]["output"], json!([]));
        assert_eq!(created["response"]["usage"], Value::Null);
    }

    #[test]
    fn empty_text_block_produces_no_output_item() {
        // 上游在工具调用前会先发一个空 text 块，不能因此给客户端塞空 message item
        let frames = run_stream(&[
            (
                "content_block_start",
                json!({"index": 0, "content_block": {"type": "text", "text": ""}}),
            ),
            ("content_block_stop", json!({"index": 0})),
            (
                "content_block_start",
                json!({"index": 1, "content_block": {"type": "tool_use", "id": "toolu_1", "name": "ls"}}),
            ),
            (
                "content_block_delta",
                json!({"index": 1, "delta": {"type": "input_json_delta", "partial_json": "{}"}}),
            ),
            ("content_block_stop", json!({"index": 1})),
            ("message_stop", json!({})),
        ]);

        assert_eq!(
            event_names(&frames),
            vec![
                "response.created",
                "response.in_progress",
                "response.output_item.added",
                "response.function_call_arguments.delta",
                "response.function_call_arguments.done",
                "response.output_item.done",
                "response.completed",
            ]
        );
        // 工具 item 占 output_index 0（空文本块没有占用编号）
        let parsed = parse_all(&frames);
        assert_eq!(parsed[2].1["output_index"], json!(0));
        let completed = &parsed.last().unwrap().1["response"];
        assert_eq!(completed["output"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn block_start_with_initial_text_keeps_it() {
        let frames = run_stream(&[
            (
                "content_block_start",
                json!({"index": 0, "content_block": {"type": "text", "text": "开头"}}),
            ),
            (
                "content_block_delta",
                json!({"index": 0, "delta": {"type": "text_delta", "text": "结尾"}}),
            ),
            ("content_block_stop", json!({"index": 0})),
            ("message_stop", json!({})),
        ]);

        let parsed = parse_all(&frames);
        let done = parsed
            .iter()
            .find(|(n, _)| n == "response.output_text.done")
            .expect("缺少 output_text.done");
        assert_eq!(done.1["text"], json!("开头结尾"));
    }

    #[test]
    fn tool_call_stream_emits_arguments_events() {
        let frames = run_stream(&[
            ("message_start", json!({"message": {"id": "msg_up"}})),
            (
                "content_block_start",
                json!({"index": 0, "content_block": {"type": "tool_use", "id": "toolu_1", "name": "read_file"}}),
            ),
            (
                "content_block_delta",
                json!({"index": 0, "delta": {"type": "input_json_delta", "partial_json": "{\"path\""}}),
            ),
            (
                "content_block_delta",
                json!({"index": 0, "delta": {"type": "input_json_delta", "partial_json": ":\"a.rs\"}"}}),
            ),
            ("content_block_stop", json!({"index": 0})),
            (
                "message_delta",
                json!({"delta": {"stop_reason": "tool_use"}}),
            ),
            ("message_stop", json!({})),
        ]);

        assert_eq!(
            event_names(&frames),
            vec![
                "response.created",
                "response.in_progress",
                "response.output_item.added",
                "response.function_call_arguments.delta",
                "response.function_call_arguments.delta",
                "response.function_call_arguments.done",
                "response.output_item.done",
                "response.completed",
            ]
        );

        let parsed = parse_all(&frames);
        let added = &parsed[2].1["item"];
        assert_eq!(added["type"], json!("function_call"));
        assert_eq!(added["call_id"], json!("toolu_1"));
        assert_eq!(added["name"], json!("read_file"));
        assert_eq!(added["status"], json!("in_progress"));

        assert_eq!(parsed[5].1["arguments"], json!("{\"path\":\"a.rs\"}"));
        let done_item = &parsed[6].1["item"];
        assert_eq!(done_item["arguments"], json!("{\"path\":\"a.rs\"}"));
        assert_eq!(done_item["status"], json!("completed"));
        // 工具调用不产生 message item
        let completed = &parsed.last().unwrap().1["response"];
        assert_eq!(completed["output"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn tool_call_without_arguments_yields_empty_json_object() {
        let frames = run_stream(&[
            (
                "content_block_start",
                json!({"index": 0, "content_block": {"type": "tool_use", "id": "toolu_1", "name": "now"}}),
            ),
            ("content_block_stop", json!({"index": 0})),
            ("message_stop", json!({})),
        ]);
        let parsed = parse_all(&frames);
        let done = parsed
            .iter()
            .find(|(n, _)| n == "response.function_call_arguments.done")
            .expect("缺少 arguments.done");
        assert_eq!(done.1["arguments"], json!("{}"));
    }

    #[test]
    fn reasoning_stream_uses_distinct_output_index_and_hides_signature() {
        let frames = run_stream(&[
            ("message_start", json!({"message": {"id": "msg_up"}})),
            (
                "content_block_start",
                json!({"index": 0, "content_block": {"type": "thinking", "thinking": ""}}),
            ),
            (
                "content_block_delta",
                json!({"index": 0, "delta": {"type": "thinking_delta", "thinking": "先看文件"}}),
            ),
            (
                "content_block_delta",
                json!({"index": 0, "delta": {"type": "signature_delta", "signature": "A".repeat(120)}}),
            ),
            ("content_block_stop", json!({"index": 0})),
            (
                "content_block_start",
                json!({"index": 1, "content_block": {"type": "text", "text": ""}}),
            ),
            (
                "content_block_delta",
                json!({"index": 1, "delta": {"type": "text_delta", "text": "结论"}}),
            ),
            ("content_block_stop", json!({"index": 1})),
            ("message_stop", json!({})),
        ]);

        assert_eq!(
            event_names(&frames),
            vec![
                "response.created",
                "response.in_progress",
                "response.output_item.added",
                "response.reasoning_summary_part.added",
                "response.reasoning_summary_text.delta",
                "response.reasoning_summary_text.done",
                "response.reasoning_summary_part.done",
                "response.output_item.done",
                "response.output_item.added",
                "response.content_part.added",
                "response.output_text.delta",
                "response.output_text.done",
                "response.content_part.done",
                "response.output_item.done",
                "response.completed",
            ]
        );

        let parsed = parse_all(&frames);
        // reasoning 与文本必须占不同的 output_index
        assert_eq!(parsed[2].1["output_index"], json!(0));
        assert_eq!(parsed[8].1["output_index"], json!(1));
        assert_eq!(parsed[4].1["summary_index"], json!(0));

        // 伪造签名不得出现在任何下发帧中
        let joined = frames.concat();
        assert!(!joined.contains(&"A".repeat(100)));
        assert!(!joined.contains("signature"));
    }

    #[test]
    fn sequence_numbers_strictly_increase_across_mixed_blocks() {
        let frames = run_stream(&[
            ("message_start", json!({"message": {"id": "msg_up"}})),
            (
                "content_block_start",
                json!({"index": 0, "content_block": {"type": "thinking", "thinking": ""}}),
            ),
            (
                "content_block_delta",
                json!({"index": 0, "delta": {"type": "thinking_delta", "thinking": "t"}}),
            ),
            ("content_block_stop", json!({"index": 0})),
            (
                "content_block_start",
                json!({"index": 1, "content_block": {"type": "text", "text": ""}}),
            ),
            (
                "content_block_delta",
                json!({"index": 1, "delta": {"type": "text_delta", "text": "a"}}),
            ),
            ("content_block_stop", json!({"index": 1})),
            (
                "content_block_start",
                json!({"index": 2, "content_block": {"type": "tool_use", "id": "toolu_1", "name": "ls"}}),
            ),
            (
                "content_block_delta",
                json!({"index": 2, "delta": {"type": "input_json_delta", "partial_json": "{}"}}),
            ),
            ("content_block_stop", json!({"index": 2})),
            ("message_stop", json!({})),
        ]);

        let parsed = parse_all(&frames);
        let seqs: Vec<i64> = parsed
            .iter()
            .map(|(_, d)| d["sequence_number"].as_i64().expect("缺少 sequence_number"))
            .collect();
        assert_eq!(seqs, (0..seqs.len() as i64).collect::<Vec<_>>());
        // 三个块占三个不同的 output_index
        let completed = &parsed.last().unwrap().1["response"];
        assert_eq!(completed["output"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn truncated_stream_ends_with_response_incomplete() {
        for reason in ["max_tokens", "model_context_window_exceeded"] {
            let frames = run_stream(&[
                (
                    "content_block_start",
                    json!({"index": 0, "content_block": {"type": "text", "text": ""}}),
                ),
                (
                    "content_block_delta",
                    json!({"index": 0, "delta": {"type": "text_delta", "text": "半截"}}),
                ),
                ("content_block_stop", json!({"index": 0})),
                (
                    "message_delta",
                    json!({"delta": {"stop_reason": reason}, "usage": {"input_tokens": 1, "output_tokens": 1}}),
                ),
                ("message_stop", json!({})),
            ]);

            let names = event_names(&frames);
            assert_eq!(names.last().unwrap(), "response.incomplete", "{}", reason);
            assert!(
                !names.iter().any(|n| n == "response.completed"),
                "{}",
                reason
            );

            let (_, last) = parse_frame(frames.last().unwrap());
            assert_eq!(last["response"]["status"], json!("incomplete"));
            assert_eq!(
                last["response"]["incomplete_details"]["reason"],
                json!("max_output_tokens")
            );
        }
    }

    #[test]
    fn finish_is_idempotent() {
        let mut conv = ResponsesStreamConverter::new("gpt-5.6-terra", HashSet::new());
        conv.on_event("message_start", &json!({"message": {"id": "msg_up"}}));
        let first = conv.finish();
        assert_eq!(event_names(&first), vec!["response.completed"]);
        assert!(conv.finish().is_empty());
    }

    #[test]
    fn interrupted_stream_closes_open_items() {
        // 上游断开：只有 start + delta，没有 content_block_stop / message_stop
        let mut conv = ResponsesStreamConverter::new("gpt-5.6-terra", HashSet::new());
        let mut frames = conv.on_event(
            "content_block_start",
            &json!({"index": 0, "content_block": {"type": "text", "text": ""}}),
        );
        frames.extend(conv.on_event(
            "content_block_delta",
            &json!({"index": 0, "delta": {"type": "text_delta", "text": "半"}}),
        ));
        frames.extend(conv.finish());

        assert_eq!(
            event_names(&frames),
            vec![
                "response.created",
                "response.in_progress",
                "response.output_item.added",
                "response.content_part.added",
                "response.output_text.delta",
                "response.output_text.done",
                "response.content_part.done",
                "response.output_item.done",
                "response.completed",
            ]
        );
    }

    #[test]
    fn empty_stream_still_yields_created_and_completed() {
        let frames = run_stream(&[("message_stop", json!({}))]);
        assert_eq!(
            event_names(&frames),
            vec![
                "response.created",
                "response.in_progress",
                "response.completed",
            ]
        );
        let (_, last) = parse_frame(frames.last().unwrap());
        assert_eq!(last["response"]["output"], json!([]));
    }

    #[test]
    fn delta_without_block_start_is_lazily_opened() {
        let frames = run_stream(&[
            (
                "content_block_delta",
                json!({"index": 3, "delta": {"type": "text_delta", "text": "裸增量"}}),
            ),
            ("message_stop", json!({})),
        ]);
        assert_eq!(
            event_names(&frames),
            vec![
                "response.created",
                "response.in_progress",
                "response.output_item.added",
                "response.content_part.added",
                "response.output_text.delta",
                "response.output_text.done",
                "response.content_part.done",
                "response.output_item.done",
                "response.completed",
            ]
        );
    }

    #[test]
    fn orphan_arguments_delta_and_unknown_events_are_skipped() {
        let frames = run_stream(&[
            // 没有 tool_use 块就来的参数增量：拿不到 call_id / name，只能丢弃
            (
                "content_block_delta",
                json!({"index": 0, "delta": {"type": "input_json_delta", "partial_json": "{}"}}),
            ),
            (
                "content_block_delta",
                json!({"index": 0, "delta": {"type": "citations_delta", "citation": {}}}),
            ),
            (
                "content_block_start",
                json!({"index": 1, "content_block": {"type": "redacted_thinking", "data": "x"}}),
            ),
            ("content_block_stop", json!({"index": 9})),
            ("unknown_event", json!({})),
            ("message_stop", json!({})),
        ]);
        assert_eq!(
            event_names(&frames),
            vec![
                "response.created",
                "response.in_progress",
                "response.completed",
            ]
        );
    }

    #[test]
    fn upstream_error_event_terminates_without_completed() {
        let mut conv = ResponsesStreamConverter::new("gpt-5.6-terra", HashSet::new());
        let mut frames = conv.on_event("message_start", &json!({"message": {"id": "msg_up"}}));
        frames.extend(conv.on_event(
            "error",
            &json!({"error": {"type": "overloaded_error", "message": "上游繁忙"}}),
        ));
        // 错误已终止流，后续 finish 不再补 completed
        frames.extend(conv.finish());

        let names = event_names(&frames);
        assert_eq!(
            names,
            vec!["response.created", "response.in_progress", "error"]
        );
        let (_, err) = parse_frame(frames.last().unwrap());
        assert_eq!(err["message"], json!("上游繁忙"));
    }

    #[test]
    fn upstream_error_closes_open_items_before_terminating() {
        let mut conv = ResponsesStreamConverter::new("gpt-5.6-terra", HashSet::new());
        let mut frames = conv.on_event("message_start", &json!({"message": {"id": "msg_up"}}));
        // 文本块已打开但未收到 content_block_stop 就报错
        frames.extend(conv.on_event(
            "content_block_start",
            &json!({"index": 0, "content_block": {"type": "text", "text": "半截"}}),
        ));
        frames.extend(conv.on_event(
            "error",
            &json!({"error": {"type": "overloaded_error", "message": "上游繁忙"}}),
        ));
        frames.extend(conv.finish());

        let names = event_names(&frames);
        // added 必须有配对的 done，且 error 是最后一个事件
        assert_eq!(
            names
                .iter()
                .filter(|n| *n == "response.output_item.added")
                .count(),
            names
                .iter()
                .filter(|n| *n == "response.output_item.done")
                .count(),
        );
        assert_eq!(names.last().unwrap(), "error");
        assert!(!names.iter().any(|n| n == "response.completed"));
        assert!(!names.iter().any(|n| n == "response.incomplete"));

        // sequence_number 仍严格递增无跳号
        let seqs: Vec<i64> = parse_all(&frames)
            .iter()
            .map(|(_, d)| d["sequence_number"].as_i64().expect("缺少 sequence_number"))
            .collect();
        assert_eq!(seqs, (0..seqs.len() as i64).collect::<Vec<_>>());
    }

    #[test]
    fn error_after_finish_emits_nothing() {
        let mut conv = ResponsesStreamConverter::new("gpt-5.6-terra", HashSet::new());
        let _ = conv.on_event("message_start", &json!({"message": {"id": "msg_up"}}));
        let _ = conv.finish();
        let extra = conv.on_event(
            "error",
            &json!({"error": {"type": "overloaded_error", "message": "迟到的错误"}}),
        );
        assert!(extra.is_empty(), "finish 之后不应再下发任何帧");
    }

    #[test]
    fn plain_text_response_has_single_message_item() {
        let out = convert_non_stream(
            &json!({
                "id": "msg_up",
                "type": "message",
                "role": "assistant",
                "content": [{"type": "text", "text": "你好"}],
                "stop_reason": "end_turn",
                "usage": {"input_tokens": 12, "output_tokens": 3},
            }),
            "gpt-5-codex",
        );

        assert!(out["id"].as_str().unwrap().starts_with("resp_"));
        assert_eq!(out["object"], "response");
        assert!(out["created_at"].as_i64().unwrap() > 0);
        assert_eq!(out["status"], "completed");
        assert_eq!(out["model"], "gpt-5-codex");
        assert!(out.get("incomplete_details").is_none());

        let output = out["output"].as_array().unwrap();
        assert_eq!(output.len(), 1);
        assert_eq!(output[0]["type"], "message");
        assert!(output[0]["id"].as_str().unwrap().starts_with("msg_"));
        assert_eq!(output[0]["status"], "completed");
        assert_eq!(output[0]["role"], "assistant");
        assert_eq!(
            output[0]["content"],
            json!([{"type": "output_text", "text": "你好", "annotations": []}])
        );

        assert_eq!(
            out["usage"],
            json!({
                "input_tokens": 12, "output_tokens": 3, "total_tokens": 15,
                "input_tokens_details": {"cached_tokens": 0},
            })
        );
    }

    #[test]
    fn multiple_text_blocks_are_concatenated() {
        let out = convert_non_stream(
            &json!({
                "content": [
                    {"type": "text", "text": "前半"},
                    {"type": "text", "text": "后半"},
                ],
                "stop_reason": "end_turn",
            }),
            "gpt-5-codex",
        );
        assert_eq!(out["output"][0]["content"][0]["text"], "前半后半");
    }

    #[test]
    fn tool_use_becomes_function_call_item_without_message() {
        let out = convert_non_stream(
            &json!({
                "content": [{
                    "type": "tool_use",
                    "id": "toolu_1",
                    "name": "shell",
                    "input": {"cmd": "ls"},
                }],
                "stop_reason": "tool_use",
                "usage": {"input_tokens": 5, "output_tokens": 7},
            }),
            "gpt-5-codex",
        );

        let output = out["output"].as_array().unwrap();
        assert_eq!(output.len(), 1);
        assert_eq!(output[0]["type"], "function_call");
        assert!(output[0]["id"].as_str().unwrap().starts_with("fc_"));
        assert_eq!(output[0]["call_id"], "toolu_1");
        assert_eq!(output[0]["name"], "shell");
        assert_eq!(output[0]["arguments"], r#"{"cmd":"ls"}"#);
        assert_eq!(output[0]["status"], "completed");
        // 工具调用轮次仍是 completed —— 截断才用 incomplete
        assert_eq!(out["status"], "completed");
    }

    #[test]
    fn text_and_tool_use_keep_message_before_function_call() {
        let out = convert_non_stream(
            &json!({
                "content": [
                    {"type": "text", "text": "我来跑一下"},
                    {"type": "tool_use", "id": "toolu_1", "name": "shell", "input": {}},
                ],
                "stop_reason": "tool_use",
            }),
            "gpt-5-codex",
        );
        let output = out["output"].as_array().unwrap();
        assert_eq!(output.len(), 2);
        assert_eq!(output[0]["type"], "message");
        assert_eq!(output[1]["type"], "function_call");
    }

    #[test]
    fn parallel_tool_uses_keep_distinct_call_ids() {
        let out = convert_non_stream(
            &json!({
                "content": [
                    {"type": "tool_use", "id": "toolu_1", "name": "a", "input": {}},
                    {"type": "tool_use", "id": "toolu_2", "name": "b", "input": {}},
                ],
                "stop_reason": "tool_use",
            }),
            "gpt-5-codex",
        );
        let output = out["output"].as_array().unwrap();
        assert_eq!(output.len(), 2);
        assert_eq!(output[0]["call_id"], "toolu_1");
        assert_eq!(output[1]["call_id"], "toolu_2");
        assert_ne!(output[0]["id"], output[1]["id"]);
    }

    #[test]
    fn thinking_becomes_reasoning_item_ahead_of_message() {
        let out = convert_non_stream(
            &json!({
                "content": [
                    {"type": "thinking", "thinking": "先看目录", "signature": "A".repeat(120)},
                    {"type": "text", "text": "结论"},
                ],
                "stop_reason": "end_turn",
            }),
            "gpt-5-codex",
        );

        let output = out["output"].as_array().unwrap();
        assert_eq!(output.len(), 2);
        assert_eq!(output[0]["type"], "reasoning");
        assert!(output[0]["id"].as_str().unwrap().starts_with("rs_"));
        assert_eq!(
            output[0]["summary"],
            json!([{"type": "summary_text", "text": "先看目录"}])
        );
        assert_eq!(output[1]["type"], "message");

        // 伪造签名不得出现在任何位置
        let serialized = out.to_string();
        assert!(!serialized.contains(&"A".repeat(100)));
        assert!(!serialized.contains("signature"));
    }

    #[test]
    fn max_tokens_stop_reason_yields_incomplete_status() {
        for reason in ["max_tokens", "model_context_window_exceeded"] {
            let out = convert_non_stream(
                &json!({
                    "content": [{"type": "text", "text": "半句"}],
                    "stop_reason": reason,
                }),
                "gpt-5-codex",
            );
            assert_eq!(out["status"], "incomplete", "stop_reason={reason}");
            assert_eq!(
                out["incomplete_details"],
                json!({"reason": "max_output_tokens"}),
                "stop_reason={reason}"
            );
            // 已产出的内容仍要保留
            assert_eq!(out["output"][0]["content"][0]["text"], "半句");
        }
    }

    #[test]
    fn input_tokens_include_cached_input() {
        // 与 chat 侧同一口径：input_tokens 须为输入总量，命中部分另列 cached_tokens
        let out = convert_non_stream(
            &json!({
                "stop_reason": "end_turn",
                "usage": {
                    "input_tokens": 100, "output_tokens": 7,
                    "cache_creation_input_tokens": 300,
                    "cache_read_input_tokens": 600,
                },
            }),
            "gpt-5-codex",
        );
        assert_eq!(
            out["usage"],
            json!({
                "input_tokens": 1000, "output_tokens": 7, "total_tokens": 1007,
                "input_tokens_details": {"cached_tokens": 600},
            })
        );
    }

    #[test]
    fn missing_usage_and_content_still_yields_valid_response() {
        let out = convert_non_stream(&json!({"stop_reason": "end_turn"}), "gpt-5-codex");
        assert_eq!(out["status"], "completed");
        assert_eq!(out["output"], json!([]));
        assert_eq!(
            out["usage"],
            json!({
                "input_tokens": 0, "output_tokens": 0, "total_tokens": 0,
                "input_tokens_details": {"cached_tokens": 0},
            })
        );
    }

    #[test]
    fn unknown_block_type_is_skipped() {
        let out = convert_non_stream(
            &json!({
                "content": [
                    {"type": "brand_new_block", "foo": 1},
                    {"type": "text", "text": "ok"},
                ],
                "stop_reason": "end_turn",
            }),
            "gpt-5-codex",
        );
        let output = out["output"].as_array().unwrap();
        assert_eq!(output.len(), 1);
        assert_eq!(output[0]["content"][0]["text"], "ok");
    }

    #[test]
    fn tool_use_missing_id_or_name_is_skipped() {
        let out = convert_non_stream(
            &json!({
                "content": [
                    {"type": "tool_use", "name": "no_id", "input": {}},
                    {"type": "tool_use", "id": "toolu_1", "input": {}},
                ],
                "stop_reason": "tool_use",
            }),
            "gpt-5-codex",
        );
        assert_eq!(out["output"], json!([]));
    }

    #[test]
    fn custom_tool_call_is_restored_with_free_text_input() {
        let out = super::convert_non_stream(
            &json!({
                "content": [
                    {"type": "tool_use", "id": "toolu_1", "name": "exec",
                     "input": {"input": "tools.read_file({path: 'demo.txt'})"}},
                    {"type": "tool_use", "id": "toolu_2", "name": "wait", "input": {"ms": 100}},
                ],
                "stop_reason": "tool_use",
            }),
            "gpt-5.6-terra",
            &custom_names(&["exec"]),
        );
        let output = out["output"].as_array().unwrap();
        assert_eq!(output[0]["type"], "custom_tool_call");
        assert_eq!(output[0]["call_id"], "toolu_1");
        assert_eq!(output[0]["name"], "exec");
        // 降级 schema 的包装被剥掉，客户端拿到原始自由文本
        assert_eq!(output[0]["input"], "tools.read_file({path: 'demo.txt'})");
        assert!(output[0].get("arguments").is_none());
        assert!(output[0]["id"].as_str().unwrap().starts_with("ctc_"));
        // 未声明为 custom 的工具仍走 function_call
        assert_eq!(output[1]["type"], "function_call");
        assert_eq!(output[1]["arguments"], "{\"ms\":100}");
    }

    #[test]
    fn custom_tool_input_falls_back_to_raw_json_when_schema_ignored() {
        let out = super::convert_non_stream(
            &json!({
                "content": [{"type": "tool_use", "id": "toolu_1", "name": "exec",
                             "input": {"cmd": "ls", "cwd": "/tmp"}}],
                "stop_reason": "tool_use",
            }),
            "gpt-5.6-terra",
            &custom_names(&["exec"]),
        );
        // 模型没照降级 schema 作答时不能给空串，整段 JSON 原样交给客户端
        assert_eq!(
            out["output"][0]["input"],
            "{\"cmd\":\"ls\",\"cwd\":\"/tmp\"}"
        );
    }

    #[test]
    fn custom_tool_stream_emits_custom_tool_call_input_events() {
        let frames = run_stream_with_custom(&tool_block_events("exec"), custom_names(&["exec"]));
        assert_eq!(
            event_names(&frames),
            vec![
                "response.created",
                "response.in_progress",
                "response.output_item.added",
                // 参数增量是包装 JSON 的碎片，攒到收尾才下发解包后的原文
                "response.custom_tool_call_input.delta",
                "response.custom_tool_call_input.done",
                "response.output_item.done",
                "response.completed",
            ]
        );

        let parsed = parse_all(&frames);
        assert_eq!(parsed[2].1["item"]["type"], json!("custom_tool_call"));
        assert_eq!(parsed[2].1["item"]["input"], json!(""));
        assert_eq!(parsed[3].1["delta"], json!("ls -la"));
        assert_eq!(parsed[4].1["input"], json!("ls -la"));
        assert_eq!(parsed[5].1["item"]["type"], json!("custom_tool_call"));
        assert_eq!(parsed[5].1["item"]["call_id"], json!("toolu_1"));
        assert_eq!(parsed[5].1["item"]["input"], json!("ls -la"));
        assert_eq!(parsed[5].1["item"]["status"], json!("completed"));
        // 快照里也是 custom_tool_call
        assert_eq!(
            parsed[6].1["response"]["output"][0]["type"],
            json!("custom_tool_call")
        );
        for (i, (_, data)) in parsed.iter().enumerate() {
            assert_eq!(data["sequence_number"], json!(i as i64));
        }
    }

    #[test]
    fn non_custom_tool_stream_keeps_function_call_events() {
        // 同一组事件在未声明 custom 时必须仍走 function_call 事件族（零回归）
        let frames = run_stream(&tool_block_events("exec"));
        assert_eq!(
            event_names(&frames),
            vec![
                "response.created",
                "response.in_progress",
                "response.output_item.added",
                "response.function_call_arguments.delta",
                "response.function_call_arguments.delta",
                "response.function_call_arguments.done",
                "response.output_item.done",
                "response.completed",
            ]
        );
        let parsed = parse_all(&frames);
        assert_eq!(parsed[5].1["arguments"], json!("{\"input\":\"ls -la\"}"));
    }

    #[test]
    fn interrupted_custom_tool_stream_yields_partial_json_as_input() {
        let mut conv = ResponsesStreamConverter::new("gpt-5.6-terra", custom_names(&["exec"]));
        conv.on_event(
            "content_block_start",
            &json!({"index": 0, "content_block": {"type": "tool_use", "id": "toolu_1", "name": "exec"}}),
        );
        conv.on_event(
            "content_block_delta",
            &json!({"index": 0, "delta": {"type": "input_json_delta", "partial_json": "{\"input\":\"ls"}}),
        );
        let frames = conv.finish();
        let parsed = parse_all(&frames);
        // 半截 JSON 解不出 input 字段，原样下发而不是丢空串
        let done = parsed
            .iter()
            .find(|(n, _)| n == "response.custom_tool_call_input.done")
            .expect("应有 input.done 事件");
        assert_eq!(done.1["input"], json!("{\"input\":\"ls"));
    }
}
