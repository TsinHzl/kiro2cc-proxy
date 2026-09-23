#[cfg(test)]
mod tests {
    use crate::openai::chat_request::DEFAULT_MAX_TOKENS;
    use crate::openai::responses_request::tools::{
        DEFAULT_TOOL_NAMESPACE, FREEFORM_ADAPTATION_NOTE, MAX_NAMESPACE_DEPTH, custom_tool_schema,
    };
    use crate::openai::responses_request::{ConvertedResponsesRequest, convert};
    use serde_json::{Value, json};

    fn convert_ok(body: Value) -> ConvertedResponsesRequest {
        convert(&body).expect("转换应成功")
    }

    #[test]
    fn instructions_become_system_and_string_input_becomes_user_message() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "instructions": "You are Codex.",
            "input": "list files",
        }));
        assert_eq!(r.client_model, "gpt-5-codex");
        assert!(!r.stream);
        assert_eq!(r.anthropic_body["model"], "gpt-5.6-terra");
        assert_eq!(r.anthropic_body["max_tokens"], DEFAULT_MAX_TOKENS);
        assert_eq!(
            r.anthropic_body["system"],
            json!([{"type": "text", "text": "You are Codex."}])
        );
        assert_eq!(
            r.anthropic_body["messages"],
            json!([{"role": "user", "content": [{"type": "text", "text": "list files"}]}])
        );
        assert!(r.anthropic_body.get("tools").is_none());
        assert!(r.anthropic_body.get("thinking").is_none());
    }

    #[test]
    fn freeform_tool_description_gets_adaptation_note() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": "hi",
            "tools": [{
                "type": "custom", "name": "shell",
                "description": "Send raw text, not JSON.",
            }],
        }));
        let tool = &r.anthropic_body["tools"][0];
        let desc = tool["description"].as_str().unwrap();
        assert!(
            desc.starts_with("Send raw text, not JSON."),
            "原描述须保留在前：{desc}"
        );
        assert!(
            desc.ends_with(FREEFORM_ADAPTATION_NOTE),
            "须追加适配说明：{desc}"
        );
        assert_eq!(tool["input_schema"]["additionalProperties"], false);
    }

    #[test]
    fn max_output_tokens_maps_to_max_tokens() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": "hi",
            "max_output_tokens": 8192,
        }));
        assert_eq!(r.anthropic_body["max_tokens"], 8192);
    }

    #[test]
    fn rejects_missing_model_and_input() {
        assert!(convert(&json!({"input": "hi"})).is_err());
        assert!(convert(&json!({"model": "  ", "input": "hi"})).is_err());
        assert!(convert(&json!({"model": "gpt-5-codex"})).is_err());
        assert!(convert(&json!({"model": "gpt-5-codex", "input": []})).is_err());
        // 只有 reasoning item：没有任何可转换内容
        assert!(
            convert(&json!({
                "model": "gpt-5-codex",
                "input": [{"type": "reasoning", "summary": []}],
            }))
            .is_err()
        );
    }

    #[test]
    fn rejects_stateful_request_with_previous_response_id() {
        let err = convert(&json!({
            "model": "gpt-5-codex",
            "input": "hi",
            "previous_response_id": "resp_abc123",
        }))
        .expect_err("有状态请求应被拒绝");
        assert!(err.contains("previous_response_id"));
    }

    #[test]
    fn empty_or_null_previous_response_id_is_accepted() {
        assert!(
            convert(&json!({"model": "gpt-5-codex", "input": "hi", "previous_response_id": null}))
                .is_ok()
        );
        assert!(
            convert(&json!({"model": "gpt-5-codex", "input": "hi", "previous_response_id": ""}))
                .is_ok()
        );
    }

    #[test]
    fn message_items_carry_input_and_output_text() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": [
                {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "第一问"}]},
                {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "第一答"}]},
                {"role": "user", "content": "第二问"},
            ],
        }));
        assert_eq!(
            r.anthropic_body["messages"],
            json!([
                {"role": "user", "content": [{"type": "text", "text": "第一问"}]},
                {"role": "assistant", "content": [{"type": "text", "text": "第一答"}]},
                {"role": "user", "content": [{"type": "text", "text": "第二问"}]},
            ])
        );
    }

    #[test]
    fn developer_message_item_goes_to_system() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "instructions": "base",
            "input": [
                {"type": "message", "role": "developer", "content": [{"type": "input_text", "text": "extra"}]},
                {"type": "message", "role": "user", "content": "go"},
            ],
        }));
        assert_eq!(
            r.anthropic_body["system"],
            json!([
                {"type": "text", "text": "base"},
                {"type": "text", "text": "extra"},
            ])
        );
    }

    #[test]
    fn function_call_and_output_form_tool_roundtrip() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": [
                {"type": "message", "role": "user", "content": "读一下 README"},
                {
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "read_file",
                    "arguments": "{\"path\":\"README.md\"}",
                },
                {"type": "function_call_output", "call_id": "call_1", "output": "# Title"},
            ],
        }));
        assert_eq!(
            r.anthropic_body["messages"],
            json!([
                {"role": "user", "content": [{"type": "text", "text": "读一下 README"}]},
                {"role": "assistant", "content": [{
                    "type": "tool_use",
                    "id": "call_1",
                    "name": "read_file",
                    "input": {"path": "README.md"},
                }]},
                {"role": "user", "content": [{
                    "type": "tool_result",
                    "tool_use_id": "call_1",
                    "content": "# Title",
                }]},
            ])
        );
    }

    #[test]
    fn parallel_function_calls_merge_into_one_assistant_message() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": [
                {"type": "message", "role": "user", "content": "go"},
                {"type": "function_call", "call_id": "c1", "name": "a", "arguments": "{}"},
                {"type": "function_call", "call_id": "c2", "name": "b", "arguments": "{}"},
                {"type": "function_call_output", "call_id": "c1", "output": "ra"},
                {"type": "function_call_output", "call_id": "c2", "output": "rb"},
            ],
        }));
        let messages = r.anthropic_body["messages"].as_array().unwrap();
        // user / assistant(2 个 tool_use) / user(2 个 tool_result)
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[1]["content"].as_array().unwrap().len(), 2);
        assert_eq!(messages[2]["content"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn custom_tool_call_wraps_free_text_input() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": [
                {"type": "message", "role": "user", "content": "跑个命令"},
                {"type": "custom_tool_call", "call_id": "call_x", "name": "shell", "input": "ls -la"},
                {"type": "custom_tool_call_output", "call_id": "call_x", "output": "total 0"},
            ],
        }));
        let messages = r.anthropic_body["messages"].as_array().unwrap();
        assert_eq!(
            messages[1]["content"][0],
            json!({
                "type": "tool_use",
                "id": "call_x",
                "name": "shell",
                "input": {"input": "ls -la"},
            })
        );
        assert_eq!(
            messages[2]["content"][0],
            json!({
                "type": "tool_result",
                "tool_use_id": "call_x",
                "content": "total 0",
            })
        );
    }

    #[test]
    fn tool_items_missing_call_id_or_name_are_skipped() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": [
                {"type": "message", "role": "user", "content": "go"},
                {"type": "function_call", "name": "no_id", "arguments": "{}"},
                {"type": "function_call", "call_id": "c1", "arguments": "{}"},
                {"type": "function_call_output", "output": "orphan"},
            ],
        }));
        assert_eq!(
            r.anthropic_body["messages"],
            json!([{"role": "user", "content": [{"type": "text", "text": "go"}]}])
        );
    }

    #[test]
    fn invalid_arguments_json_degrades_to_empty_object() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": [
                {"type": "message", "role": "user", "content": "go"},
                {"type": "function_call", "call_id": "c1", "name": "t", "arguments": "{not json"},
            ],
        }));
        assert_eq!(
            r.anthropic_body["messages"][1]["content"][0]["input"],
            json!({})
        );
    }

    #[test]
    fn flat_function_tool_and_custom_tool_are_converted() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": "hi",
            "tools": [
                {
                    "type": "function",
                    "name": "read_file",
                    "description": "读文件",
                    "parameters": {"type": "object", "properties": {"path": {"type": "string"}}},
                    "strict": true,
                },
                {"type": "custom", "name": "shell", "description": "跑命令"},
                {"type": "web_search"},
            ],
        }));
        assert_eq!(
            r.anthropic_body["tools"],
            json!([
                {
                    "name": "read_file",
                    "description": "读文件",
                    "input_schema": {"type": "object", "properties": {"path": {"type": "string"}}},
                },
                {
                    "name": "shell",
                    // custom 工具的描述末尾追加适配说明，原文保持在前
                    "description": format!("跑命令{FREEFORM_ADAPTATION_NOTE}"),
                    "input_schema": custom_tool_schema(),
                },
            ])
        );
    }

    #[test]
    fn additional_tools_item_expands_namespaces_into_flat_tools() {
        // Codex CLI 0.148 的真实形态：顶层 tools 为 null，工具挂在 developer item 上，
        // 且按 namespace 分组
        let r = convert_ok(json!({
            "model": "gpt-5.6-terra",
            "input": [
                {"type": "message", "role": "user", "content": "读一下 demo.txt"},
                {
                    "type": "additional_tools",
                    "role": "developer",
                    "tools": [
                        {
                            "type": "namespace",
                            "name": "functions",
                            "description": "",
                            "tools": [
                                {"type": "custom", "name": "exec", "description": "跑 JS",
                                 "format": {"type": "grammar", "syntax": "lark", "definition": "x"}},
                                {"type": "function", "name": "wait", "description": "等",
                                 "parameters": {"type": "object", "properties": {}}, "strict": true},
                            ],
                        },
                        {
                            "type": "namespace",
                            "name": "collaboration",
                            "tools": [
                                {"type": "function", "name": "list_agents", "description": "列出",
                                 "parameters": {"type": "object", "properties": {}}},
                            ],
                        },
                    ],
                },
            ],
            "tools": null,
        }));

        let names: Vec<&str> = r.anthropic_body["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        // namespace 名不拼进工具名：模型回调时给的就是叶子名。
        // collaboration 的 list_agents 被整体跳过——非默认命名空间的子工具客户端调不动
        assert_eq!(names, vec!["exec", "wait"]);
        assert_eq!(
            r.anthropic_body["tools"][0]["input_schema"],
            custom_tool_schema()
        );
        // exec 是 custom，响应侧要还原成 custom_tool_call
        assert_eq!(r.custom_tools, ["exec".to_string()].into_iter().collect());
        // additional_tools item 本身不产生任何消息
        assert_eq!(r.anthropic_body["messages"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn top_level_tool_wins_over_same_name_in_additional_tools() {
        let r = convert_ok(json!({
            "model": "gpt-5.6-terra",
            "input": [
                {"type": "message", "role": "user", "content": "go"},
                {
                    "type": "additional_tools",
                    "tools": [{"type": "custom", "name": "exec", "description": "来自 item"}],
                },
            ],
            "tools": [{
                "type": "function",
                "name": "exec",
                "description": "来自顶层",
                "parameters": {"type": "object", "properties": {}},
            }],
        }));
        let tools = r.anthropic_body["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["description"], "来自顶层");
        // 胜出的是 function 形态，不该被登记为 custom
        assert!(r.custom_tools.is_empty());
    }

    #[test]
    fn namespace_beyond_depth_limit_is_skipped() {
        // 逐层包裹出超过 MAX_NAMESPACE_DEPTH 的嵌套。每层都必须是默认命名空间，
        // 否则会被白名单在最外层直接跳过，深度守卫根本走不到
        let mut tool = json!({
            "type": "function",
            "name": "deep",
            "parameters": {"type": "object", "properties": {}},
        });
        for _ in 0..=MAX_NAMESPACE_DEPTH {
            tool = json!({"type": "namespace", "name": DEFAULT_TOOL_NAMESPACE, "tools": [tool]});
        }
        let r = convert_ok(json!({
            "model": "gpt-5.6-terra",
            "input": "hi",
            "tools": [tool],
        }));
        assert!(r.anthropic_body.get("tools").is_none());
    }

    #[test]
    fn unsupported_tools_only_yields_no_tools_field() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": "hi",
            "tools": [{"type": "web_search"}],
        }));
        assert!(r.anthropic_body.get("tools").is_none());
    }

    #[test]
    fn reasoning_effort_maps_to_thinking() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": "hi",
            "reasoning": {"effort": "high", "summary": "auto"},
        }));
        assert_eq!(
            r.anthropic_body["thinking"],
            json!({"type": "enabled", "budget_tokens": 24576})
        );

        let minimal = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": "hi",
            "reasoning": {"effort": "minimal"},
        }));
        assert!(minimal.anthropic_body.get("thinking").is_none());
    }

    #[test]
    fn encrypted_content_include_is_ignored() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": "hi",
            "include": ["reasoning.encrypted_content"],
            "store": false,
            "parallel_tool_calls": false,
        }));
        // include / store / parallel_tool_calls 都不该出现在下游请求体里
        let obj = r.anthropic_body.as_object().unwrap();
        assert!(!obj.contains_key("include"));
        assert!(!obj.contains_key("store"));
        assert!(!obj.contains_key("parallel_tool_calls"));
        assert_eq!(obj.len(), 4); // model / max_tokens / messages / stream
    }

    #[test]
    fn tool_choice_other_than_auto_still_converts() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": "hi",
            "tool_choice": {"type": "function", "name": "read_file"},
        }));
        // tool_choice 下游无读取点，只 WARN 留痕，不写入请求体
        assert!(r.anthropic_body.get("tool_choice").is_none());
    }

    #[test]
    fn input_image_data_url_becomes_image_block() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": [{
                "type": "message",
                "role": "user",
                "content": [
                    {"type": "input_text", "text": "看图"},
                    {"type": "input_image", "image_url": "data:image/png;base64,AAAA"},
                ],
            }],
        }));
        assert_eq!(
            r.anthropic_body["messages"][0]["content"][1],
            json!({
                "type": "image",
                "source": {"type": "base64", "media_type": "image/png", "data": "AAAA"},
            })
        );
    }

    #[test]
    fn stream_flag_is_propagated() {
        let r = convert_ok(json!({"model": "gpt-5-codex", "input": "hi", "stream": true}));
        assert!(r.stream);
        assert_eq!(r.anthropic_body["stream"], true);
    }

    #[test]
    fn user_field_maps_to_metadata_user_id() {
        // user 非空 → 写入 metadata.user_id（下游 converter 据此派生稳定 conversationId）
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "user": "8bb5523b-ec7c-4540-a9ca-beb6d79f1552",
            "input": "hi",
        }));
        assert_eq!(
            r.anthropic_body["metadata"]["user_id"],
            "8bb5523b-ec7c-4540-a9ca-beb6d79f1552"
        );

        // user 缺失 / 空串 / 纯空白 → 不产生 metadata
        for user in [None, Some(""), Some("   ")] {
            let mut body = json!({
                "model": "gpt-5-codex",
                "input": "hi",
            });
            if let Some(u) = user {
                body["user"] = json!(u);
            }
            let r = convert_ok(body);
            assert!(
                r.anthropic_body.get("metadata").is_none(),
                "user={user:?} 不应产生 metadata"
            );
        }
    }

    #[test]
    fn non_uuid_user_keeps_stable_conversation_id_across_calls() {
        // 非 UUID user（如 user-123）无法走 metadata 解析路径，落到 fallback 派生；
        // fallback 依赖首条消息，同一请求体连续两次转换必须得到相同 conversationId
        let body = json!({
            "model": "gpt-5-codex",
            "user": "user-123",
            "input": "hi",
        });
        let r1 = convert_ok(body.clone());
        let r2 = convert_ok(body);
        assert_eq!(
            r1.anthropic_body["metadata"]["user_id"],
            json!("user-123"),
            "非 UUID user 也应原样透传为 metadata.user_id"
        );
        let anthropic1: crate::anthropic::types::MessagesRequest =
            serde_json::from_value(r1.anthropic_body.clone()).expect("anthropic body 应可反序列化");
        let anthropic2: crate::anthropic::types::MessagesRequest =
            serde_json::from_value(r2.anthropic_body).expect("anthropic body 应可反序列化");
        let kiro1 = crate::anthropic::convert_request(&anthropic1).expect("转换 Kiro 请求应成功");
        let kiro2 = crate::anthropic::convert_request(&anthropic2).expect("转换 Kiro 请求应成功");
        assert_eq!(
            kiro1.conversation_state.conversation_id, kiro2.conversation_state.conversation_id,
            "非 UUID user 同一请求体连续两次转换必须派生相同 conversationId"
        );
    }

    #[test]
    fn unknown_item_types_and_parts_are_skipped() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": [
                {"type": "brand_new_item", "foo": 1},
                {"type": "message", "role": "user", "content": [
                    {"type": "input_audio", "data": "x"},
                    {"type": "input_text", "text": "still here"},
                ]},
            ],
        }));
        assert_eq!(
            r.anthropic_body["messages"],
            json!([{"role": "user", "content": [{"type": "text", "text": "still here"}]}])
        );
    }
}
