#[cfg(test)]
mod tests {
    use crate::openai::chat_response::{
        ChatStreamConverter, convert_non_stream, map_finish_reason,
    };
    use serde_json::{Value, json};

    fn anthropic_text_response() -> Value {
        json!({
            "id": "msg_abc",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "你好"}],
            "model": "gpt-5.6-terra",
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {
                "input_tokens": 12,
                "output_tokens": 3,
                "cache_creation_input_tokens": 0,
                "cache_read_input_tokens": 0,
            },
        })
    }

    #[test]
    fn converts_plain_text_response() {
        let out = convert_non_stream(&anthropic_text_response(), "gpt-5-codex");
        assert!(
            out["id"].as_str().unwrap().starts_with("chatcmpl-"),
            "id 前缀应为 chatcmpl-"
        );
        assert_eq!(out["object"], "chat.completion");
        assert!(out["created"].as_i64().unwrap() > 0);
        // 必须回写客户端原始模型名
        assert_eq!(out["model"], "gpt-5-codex");
        assert_eq!(out["choices"][0]["index"], 0);
        assert_eq!(out["choices"][0]["message"]["role"], "assistant");
        assert_eq!(out["choices"][0]["message"]["content"], "你好");
        assert_eq!(out["choices"][0]["finish_reason"], "stop");
    }

    #[test]
    fn usage_matches_upstream() {
        let out = convert_non_stream(&anthropic_text_response(), "gpt-5-codex");
        assert_eq!(out["usage"]["prompt_tokens"], 12);
        assert_eq!(out["usage"]["completion_tokens"], 3);
        assert_eq!(out["usage"]["total_tokens"], 15);
    }

    #[test]
    fn missing_usage_yields_zeros() {
        let out = convert_non_stream(&json!({"content": []}), "gpt-5-codex");
        assert_eq!(out["usage"]["prompt_tokens"], 0);
        assert_eq!(out["usage"]["completion_tokens"], 0);
        assert_eq!(out["usage"]["total_tokens"], 0);
    }

    #[test]
    fn converts_tool_use_response() {
        let anthropic = json!({
            "content": [{
                "type": "tool_use",
                "id": "toolu_1",
                "name": "get_weather",
                "input": {"city": "SH"},
            }],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 5, "output_tokens": 7},
        });
        let out = convert_non_stream(&anthropic, "gpt-5-codex");
        let msg = &out["choices"][0]["message"];
        assert_eq!(msg["content"], Value::Null);
        assert_eq!(out["choices"][0]["finish_reason"], "tool_calls");
        assert_eq!(msg["tool_calls"][0]["id"], "toolu_1");
        assert_eq!(msg["tool_calls"][0]["type"], "function");
        assert_eq!(msg["tool_calls"][0]["index"], 0);
        assert_eq!(msg["tool_calls"][0]["function"]["name"], "get_weather");
        // arguments 必须是 JSON 字符串而不是对象
        let args = msg["tool_calls"][0]["function"]["arguments"]
            .as_str()
            .expect("arguments 应为字符串");
        assert_eq!(
            serde_json::from_str::<Value>(args).unwrap(),
            json!({"city": "SH"})
        );
    }

    #[test]
    fn text_plus_tool_use_keeps_both() {
        let anthropic = json!({
            "content": [
                {"type": "text", "text": "让我查一下"},
                {"type": "tool_use", "id": "t1", "name": "f", "input": {}},
            ],
            "stop_reason": "tool_use",
        });
        let out = convert_non_stream(&anthropic, "m");
        assert_eq!(out["choices"][0]["message"]["content"], "让我查一下");
        assert_eq!(
            out["choices"][0]["message"]["tool_calls"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn multiple_tool_uses_get_distinct_indexes() {
        let anthropic = json!({
            "content": [
                {"type": "tool_use", "id": "a", "name": "f", "input": {}},
                {"type": "tool_use", "id": "b", "name": "g", "input": {}},
            ],
            "stop_reason": "tool_use",
        });
        let out = convert_non_stream(&anthropic, "m");
        let calls = out["choices"][0]["message"]["tool_calls"]
            .as_array()
            .unwrap();
        assert_eq!(calls[0]["index"], 0);
        assert_eq!(calls[1]["index"], 1);
    }

    #[test]
    fn prompt_tokens_include_cached_input() {
        // Anthropic 的 input_tokens 已扣掉缓存，OpenAI 的 prompt_tokens 要求输入总量
        let out = convert_non_stream(
            &json!({
                "content": [{"type": "text", "text": "hi"}],
                "stop_reason": "end_turn",
                "usage": {
                    "input_tokens": 100, "output_tokens": 7,
                    "cache_creation_input_tokens": 300,
                    "cache_read_input_tokens": 600,
                },
            }),
            "m",
        );
        assert_eq!(out["usage"]["prompt_tokens"], 1000);
        assert_eq!(out["usage"]["total_tokens"], 1007);
        // cached_tokens 只计命中，不含 creation
        assert_eq!(out["usage"]["prompt_tokens_details"]["cached_tokens"], 600);
    }

    #[test]
    fn malformed_tool_use_is_skipped_without_shifting_indexes() {
        // 缺 id / 缺 name 的块无法构造合法 tool_calls 项，跳过后剩下的 index 仍须连续
        let anthropic = json!({
            "content": [
                {"type": "tool_use", "name": "no_id", "input": {}},
                {"type": "tool_use", "id": "ok1", "name": "f", "input": {}},
                {"type": "tool_use", "id": "no_name", "input": {}},
                {"type": "tool_use", "id": "ok2", "name": "g", "input": {}},
            ],
            "stop_reason": "tool_use",
        });
        let out = convert_non_stream(&anthropic, "m");
        let calls = out["choices"][0]["message"]["tool_calls"]
            .as_array()
            .unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0]["id"], "ok1");
        assert_eq!(calls[0]["index"], 0);
        assert_eq!(calls[1]["id"], "ok2");
        assert_eq!(calls[1]["index"], 1);
    }

    #[test]
    fn thinking_goes_to_reasoning_content_without_signature() {
        let anthropic = json!({
            "content": [
                {"type": "thinking", "thinking": "先看天气", "signature": "FAKESIGNATURE".repeat(10)},
                {"type": "text", "text": "晴"},
            ],
            "stop_reason": "end_turn",
        });
        let out = convert_non_stream(&anthropic, "m");
        let msg = &out["choices"][0]["message"];
        assert_eq!(msg["reasoning_content"], "先看天气");
        assert_eq!(msg["content"], "晴");
        // 伪造签名不得出现在任何字段中
        assert!(!out.to_string().contains("FAKESIGNATURE"));
    }

    #[test]
    fn finish_reason_covers_all_upstream_stop_reasons() {
        assert_eq!(map_finish_reason(Some("end_turn")), "stop");
        assert_eq!(map_finish_reason(Some("tool_use")), "tool_calls");
        assert_eq!(map_finish_reason(Some("max_tokens")), "length");
        assert_eq!(
            map_finish_reason(Some("model_context_window_exceeded")),
            "length"
        );
        assert_eq!(map_finish_reason(Some("stop_sequence")), "stop");
        // 未枚举值回退
        assert_eq!(map_finish_reason(Some("brand_new_reason")), "stop");
        assert_eq!(map_finish_reason(None), "stop");
    }

    #[test]
    fn unknown_content_block_is_skipped() {
        let anthropic = json!({
            "content": [
                {"type": "redacted_thinking", "data": "xxx"},
                {"type": "text", "text": "ok"},
            ],
            "stop_reason": "end_turn",
        });
        let out = convert_non_stream(&anthropic, "m");
        assert_eq!(out["choices"][0]["message"]["content"], "ok");
    }

    #[test]
    fn empty_content_yields_empty_string_not_null() {
        // 无 tool_calls 时 content 为空串（null 会让部分 SDK 报错）
        let out = convert_non_stream(&json!({"content": [], "stop_reason": "end_turn"}), "m");
        assert_eq!(out["choices"][0]["message"]["content"], "");
        assert!(out["choices"][0]["message"].get("tool_calls").is_none());
    }

    // === 流式 ===

    /// 把帧文本还原为 JSON（`[DONE]` 保持原样返回 `None`）
    fn parse_frame(frame: &str) -> Option<Value> {
        let payload = frame
            .strip_prefix("data: ")
            .and_then(|s| s.strip_suffix("\n\n"))
            .expect("帧格式应为 'data: <payload>\\n\\n'");
        if payload == "[DONE]" {
            return None;
        }
        Some(serde_json::from_str(payload).expect("帧内容应为合法 JSON"))
    }

    fn text_delta(index: i64, text: &str) -> Value {
        json!({
            "type": "content_block_delta",
            "index": index,
            "delta": {"type": "text_delta", "text": text},
        })
    }

    /// 驱动一串上游事件，返回所有下发帧
    fn run_stream(events: &[(&str, Value)], include_usage: bool) -> Vec<String> {
        let mut conv = ChatStreamConverter::new("gpt-5-codex", include_usage);
        let mut frames = Vec::new();
        for (name, data) in events {
            frames.extend(conv.on_event(name, data));
        }
        frames.extend(conv.finish());
        frames
    }

    #[test]
    fn plain_text_stream_has_role_first_and_done_last() {
        let frames = run_stream(
            &[
                ("message_start", json!({"type": "message_start"})),
                (
                    "content_block_start",
                    json!({"index": 0, "content_block": {"type": "text", "text": ""}}),
                ),
                ("content_block_delta", text_delta(0, "你")),
                ("content_block_delta", text_delta(0, "好")),
                ("content_block_stop", json!({"index": 0})),
                (
                    "message_delta",
                    json!({"delta": {"stop_reason": "end_turn"}, "usage": {"input_tokens": 4, "output_tokens": 2}}),
                ),
                ("message_stop", json!({})),
            ],
            false,
        );

        // 首帧：role
        let first = parse_frame(&frames[0]).unwrap();
        assert_eq!(first["object"], "chat.completion.chunk");
        assert_eq!(first["model"], "gpt-5-codex");
        assert_eq!(first["choices"][0]["delta"]["role"], "assistant");

        // 文本增量
        assert_eq!(
            parse_frame(&frames[1]).unwrap()["choices"][0]["delta"]["content"],
            "你"
        );
        assert_eq!(
            parse_frame(&frames[2]).unwrap()["choices"][0]["delta"]["content"],
            "好"
        );

        // 倒数第二帧：finish_reason 非 null 且 delta 为空对象
        let finish = parse_frame(&frames[frames.len() - 2]).unwrap();
        assert_eq!(finish["choices"][0]["finish_reason"], "stop");
        assert_eq!(finish["choices"][0]["delta"], json!({}));

        // 末帧：[DONE]
        assert_eq!(frames.last().unwrap(), "data: [DONE]\n\n");
        // 未开启 include_usage 时不得出现 usage 帧
        assert!(frames.iter().all(|f| !f.contains("\"usage\"")));
        // 所有 chunk 共用同一个 id
        let id = first["id"].as_str().unwrap().to_string();
        for f in &frames {
            if let Some(v) = parse_frame(f) {
                assert_eq!(v["id"], id);
            }
        }
    }

    #[test]
    fn include_usage_appends_usage_frame_before_done() {
        let frames = run_stream(
            &[
                ("message_start", json!({})),
                ("content_block_delta", text_delta(0, "x")),
                (
                    "message_delta",
                    json!({"delta": {"stop_reason": "end_turn"}, "usage": {"input_tokens": 10, "output_tokens": 5}}),
                ),
                ("message_stop", json!({})),
            ],
            true,
        );

        let usage_frame = parse_frame(&frames[frames.len() - 2]).unwrap();
        assert_eq!(usage_frame["choices"], json!([]));
        assert_eq!(usage_frame["usage"]["prompt_tokens"], 10);
        assert_eq!(usage_frame["usage"]["completion_tokens"], 5);
        assert_eq!(usage_frame["usage"]["total_tokens"], 15);
        assert_eq!(frames.last().unwrap(), "data: [DONE]\n\n");
    }

    #[test]
    fn tool_call_stream_emits_id_then_argument_deltas() {
        let frames = run_stream(
            &[
                ("message_start", json!({})),
                (
                    "content_block_start",
                    json!({"index": 1, "content_block": {
                        "type": "tool_use", "id": "toolu_1", "name": "get_weather", "input": {},
                    }}),
                ),
                (
                    "content_block_delta",
                    json!({"index": 1, "delta": {"type": "input_json_delta", "partial_json": "{\"ci"}}),
                ),
                (
                    "content_block_delta",
                    json!({"index": 1, "delta": {"type": "input_json_delta", "partial_json": "ty\":\"SH\"}"}}),
                ),
                ("content_block_stop", json!({"index": 1})),
                (
                    "message_delta",
                    json!({"delta": {"stop_reason": "tool_use"}}),
                ),
                ("message_stop", json!({})),
            ],
            false,
        );

        // frames[0] = role 帧，frames[1] = 工具首帧
        let head = parse_frame(&frames[1]).unwrap();
        let call = &head["choices"][0]["delta"]["tool_calls"][0];
        assert_eq!(call["index"], 0);
        assert_eq!(call["id"], "toolu_1");
        assert_eq!(call["type"], "function");
        assert_eq!(call["function"]["name"], "get_weather");

        // 后续帧只带 index 与 arguments 增量，不重复 id / name
        let arg1 = parse_frame(&frames[2]).unwrap();
        let c1 = &arg1["choices"][0]["delta"]["tool_calls"][0];
        assert_eq!(c1["index"], 0);
        assert_eq!(c1["function"]["arguments"], "{\"ci");
        assert!(c1.get("id").is_none());
        assert!(c1["function"].get("name").is_none());

        let finish = parse_frame(&frames[frames.len() - 2]).unwrap();
        assert_eq!(finish["choices"][0]["finish_reason"], "tool_calls");
    }

    #[test]
    fn concurrent_tool_calls_get_stable_distinct_indexes() {
        let frames = run_stream(
            &[
                ("message_start", json!({})),
                (
                    "content_block_start",
                    json!({"index": 1, "content_block": {"type": "tool_use", "id": "a", "name": "f"}}),
                ),
                (
                    "content_block_start",
                    json!({"index": 2, "content_block": {"type": "tool_use", "id": "b", "name": "g"}}),
                ),
                (
                    "content_block_delta",
                    json!({"index": 2, "delta": {"type": "input_json_delta", "partial_json": "{}"}}),
                ),
                (
                    "content_block_delta",
                    json!({"index": 1, "delta": {"type": "input_json_delta", "partial_json": "{}"}}),
                ),
                (
                    "message_delta",
                    json!({"delta": {"stop_reason": "tool_use"}}),
                ),
                ("message_stop", json!({})),
            ],
            false,
        );

        let idx_of = |frame: &str| -> i64 {
            parse_frame(frame).unwrap()["choices"][0]["delta"]["tool_calls"][0]["index"]
                .as_i64()
                .unwrap()
        };
        // 两个块拿到不同 index
        assert_eq!(idx_of(&frames[1]), 0);
        assert_eq!(idx_of(&frames[2]), 1);
        // 参数增量按块归属回到各自 index（顺序交错也不串号）
        assert_eq!(idx_of(&frames[3]), 1);
        assert_eq!(idx_of(&frames[4]), 0);
    }

    #[test]
    fn thinking_delta_goes_to_reasoning_content() {
        let frames = run_stream(
            &[
                ("message_start", json!({})),
                (
                    "content_block_delta",
                    json!({"index": 0, "delta": {"type": "thinking_delta", "thinking": "推理中"}}),
                ),
                ("content_block_delta", text_delta(1, "答案")),
                ("message_stop", json!({})),
            ],
            false,
        );

        let reasoning = parse_frame(&frames[1]).unwrap();
        assert_eq!(
            reasoning["choices"][0]["delta"]["reasoning_content"],
            "推理中"
        );
        // 推理内容不得混入 content
        assert!(reasoning["choices"][0]["delta"].get("content").is_none());
        assert_eq!(
            parse_frame(&frames[2]).unwrap()["choices"][0]["delta"]["content"],
            "答案"
        );
    }

    #[test]
    fn signature_delta_never_reaches_client() {
        let fake_signature = "A".repeat(120);
        let frames = run_stream(
            &[
                ("message_start", json!({})),
                (
                    "content_block_delta",
                    json!({"index": 0, "delta": {"type": "thinking_delta", "thinking": "想"}}),
                ),
                (
                    "content_block_delta",
                    json!({"index": 0, "delta": {"type": "signature_delta", "signature": fake_signature}}),
                ),
                ("content_block_stop", json!({"index": 0})),
                ("message_stop", json!({})),
            ],
            false,
        );

        let all = frames.concat();
        assert!(
            !all.contains(&"A".repeat(120)),
            "伪造签名串不得出现在任何下发帧中"
        );
        assert!(!all.contains("signature"));
        // 签名帧不产生任何输出：role + reasoning + finish + DONE 共 4 帧
        assert_eq!(frames.len(), 4);
    }

    #[test]
    fn error_after_done_emits_no_second_done() {
        let mut conv = ChatStreamConverter::new("gpt-5.6-terra", false);
        let mut frames = conv.on_event("message_start", &json!({}));
        frames.extend(conv.finish());
        // message_stop 之后上游又异常下发 error，不得再产出第二个 [DONE]
        let extra = conv.on_event(
            "error",
            &json!({"error": {"type": "overloaded_error", "message": "迟到的错误"}}),
        );
        assert!(extra.is_empty(), "收尾后不应再下发任何帧");
        assert_eq!(
            frames.iter().filter(|f| f.contains("[DONE]")).count(),
            1,
            "整条流只应有一个 [DONE]"
        );
    }

    #[test]
    fn max_tokens_stop_reason_maps_to_length() {
        let frames = run_stream(
            &[
                ("message_start", json!({})),
                ("content_block_delta", text_delta(0, "x")),
                (
                    "message_delta",
                    json!({"delta": {"stop_reason": "model_context_window_exceeded"}}),
                ),
                ("message_stop", json!({})),
            ],
            false,
        );
        let finish = parse_frame(&frames[frames.len() - 2]).unwrap();
        assert_eq!(finish["choices"][0]["finish_reason"], "length");
    }

    #[test]
    fn finish_is_idempotent_after_message_stop() {
        let mut conv = ChatStreamConverter::new("m", true);
        let mut frames = conv.on_event("message_start", &json!({}));
        frames.extend(conv.on_event("message_stop", &json!({})));
        let extra = conv.finish();
        // message_stop 已收尾，重复 finish 不得再产生帧（否则客户端会看到两个 [DONE]）
        assert!(extra.is_empty());
        assert_eq!(frames.iter().filter(|f| f.contains("[DONE]")).count(), 1);
    }

    #[test]
    fn truncated_stream_still_gets_finish_and_done() {
        // 上游只发了 message_start 就断开
        let frames = run_stream(&[("message_start", json!({}))], false);
        assert_eq!(frames.len(), 3);
        assert_eq!(
            parse_frame(&frames[0]).unwrap()["choices"][0]["delta"]["role"],
            "assistant"
        );
        assert_eq!(
            parse_frame(&frames[1]).unwrap()["choices"][0]["finish_reason"],
            "stop"
        );
        assert_eq!(frames[2], "data: [DONE]\n\n");
    }

    #[test]
    fn upstream_error_event_emits_error_frame_then_done() {
        let mut conv = ChatStreamConverter::new("m", false);
        let mut frames = conv.on_event("message_start", &json!({}));
        frames.extend(conv.on_event(
            "error",
            &json!({"type": "error", "error": {"type": "overloaded_error", "message": "上游过载"}}),
        ));
        // 错误帧之后不得再补发正常收尾帧
        frames.extend(conv.finish());

        let err = parse_frame(&frames[1]).unwrap();
        assert_eq!(err["error"]["message"], "上游过载");
        // overloaded_error 映射为 OpenAI 的 server_error / overloaded（见 openai::error）
        assert_eq!(err["error"]["type"], "server_error");
        assert_eq!(err["error"]["code"], "overloaded");
        assert_eq!(frames.last().unwrap(), "data: [DONE]\n\n");
        assert_eq!(frames.iter().filter(|f| f.contains("[DONE]")).count(), 1);
    }

    #[test]
    fn unknown_event_and_unknown_delta_are_skipped() {
        let frames = run_stream(
            &[
                ("message_start", json!({})),
                ("brand_new_event", json!({"foo": 1})),
                (
                    "content_block_delta",
                    json!({"index": 0, "delta": {"type": "brand_new_delta", "x": 1}}),
                ),
                ("message_stop", json!({})),
            ],
            false,
        );
        // 只剩 role + finish + DONE
        assert_eq!(frames.len(), 3);
    }
}
