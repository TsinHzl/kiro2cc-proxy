#[cfg(test)]
mod tests {
    use crate::openai::chat_request::DEFAULT_MAX_TOKENS;
    use crate::openai::responses_request::tools::{
        DEFAULT_TOOL_NAMESPACE, FREEFORM_ADAPTATION_NOTE, MAX_NAMESPACE_DEPTH, custom_tool_schema,
        flatten_tool_name,
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
        // 缺 call_id 的 function_call 被跳过；没有配对调用的输出降级为 user 文本，不丢上下文
        assert_eq!(
            r.anthropic_body["messages"],
            json!([{"role": "user", "content": [
                {"type": "text", "text": "go"},
                {"type": "text", "text": "<tool_output>orphan</tool_output>"},
            ]}])
        );
    }

    // ---------- 孤立 function_call_output（issue #66：Codex Desktop 0.167 的界面状态注入） ----------

    #[test]
    fn orphan_function_call_output_becomes_user_text_in_history() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": [
                {"type": "message", "role": "user", "content": "跑 ls"},
                {
                    "type": "function_call_output",
                    "id": "fco_1",
                    "name": "codex_apps_open_page",
                    "namespace": "agent",
                    "output": "{\"page_id\":null}",
                },
                {"type": "function_call", "call_id": "call_1", "name": "shell", "arguments": "{}"},
                {"type": "function_call_output", "call_id": "call_1", "output": "total 0"},
                {
                    "type": "function_call_output",
                    "id": "fco_2",
                    "name": "browser_tabs",
                    "namespace": "agent",
                    "output": "tabs",
                },
            ],
        }));
        assert_eq!(
            r.anthropic_body["messages"],
            json!([
                {"role": "user", "content": [
                    {"type": "text", "text": "跑 ls"},
                    {"type": "text", "text": "<codex_apps_open_page>{\"page_id\":null}</codex_apps_open_page>"},
                ]},
                {"role": "assistant", "content": [{
                    "type": "tool_use", "id": "call_1", "name": "shell", "input": {},
                }]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "call_1", "content": "total 0"},
                    {"type": "text", "text": "<browser_tabs>tabs</browser_tabs>"},
                ]},
            ])
        );
    }

    #[test]
    fn orphan_output_with_unknown_call_id_is_text_not_tool_result() {
        // 有 call_id 但前文没有对应调用（历史被截断等），同样不能发成 tool_result
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": [
                {"type": "message", "role": "user", "content": "go"},
                {"type": "function_call_output", "call_id": "call_gone", "output": "late"},
            ],
        }));
        let text = r.anthropic_body["messages"].to_string();
        assert!(!text.contains("tool_result"), "{text}");
        assert!(text.contains("<tool_output>late</tool_output>"), "{text}");
    }

    #[test]
    fn orphan_output_with_empty_content_is_dropped() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": [
                {"type": "message", "role": "user", "content": "go"},
                {"type": "function_call_output", "id": "fco_1", "name": "x", "output": ""},
            ],
        }));
        assert_eq!(
            r.anthropic_body["messages"],
            json!([{"role": "user", "content": [{"type": "text", "text": "go"}]}])
        );
    }

    #[test]
    fn orphan_output_label_is_sanitized() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": [
                {"type": "message", "role": "user", "content": "go"},
                {"type": "function_call_output", "name": "a</b> <c>", "output": "x"},
            ],
        }));
        assert!(
            r.anthropic_body["messages"]
                .to_string()
                .contains("<abc>x</abc>")
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
        // functions 命名空间的子工具保持裸名；其余 namespace 的子工具展开成 `ns__叶子名`，
        // 并登记映射供响应侧还原 namespace 字段
        assert_eq!(names, vec!["exec", "wait", "collaboration__list_agents"]);
        assert_eq!(
            r.namespaced_tools,
            [(
                "collaboration__list_agents".to_string(),
                ("collaboration".to_string(), "list_agents".to_string())
            )]
            .into_iter()
            .collect()
        );
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
    fn mcp_namespace_tools_are_flattened_and_recorded() {
        // Codex 0.163：顶层 tools 里每个 MCP server 是一个 namespace 容器，叶子名是裸名
        let r = convert_ok(json!({
            "model": "gpt-5.6-terra",
            "input": "hi",
            "tools": [
                {"type": "function", "name": "list_mcp_resources",
                 "parameters": {"type": "object", "properties": {}}},
                {"type": "namespace", "name": "mcp__dbhub", "tools": [
                    {"type": "function", "name": "execute_sql", "description": "跑 SQL",
                     "parameters": {"type": "object", "properties": {"sql": {"type": "string"}}}},
                    {"type": "function", "name": "search_objects",
                     "parameters": {"type": "object", "properties": {}}},
                ]},
                {"type": "namespace", "name": "mcp__yunxiao", "tools": [
                    {"type": "function", "name": "execute_sql",
                     "parameters": {"type": "object", "properties": {}}},
                ]},
            ],
        }));
        let names: Vec<&str> = r.anthropic_body["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        // 不同 server 的同名叶子工具靠前缀区分，不会被去重吞掉
        assert_eq!(
            names,
            vec![
                "list_mcp_resources",
                "mcp__dbhub__execute_sql",
                "mcp__dbhub__search_objects",
                "mcp__yunxiao__execute_sql",
            ]
        );
        assert_eq!(
            r.namespaced_tools["mcp__yunxiao__execute_sql"],
            ("mcp__yunxiao".to_string(), "execute_sql".to_string())
        );
        // 顶层裸函数不在映射里
        assert!(!r.namespaced_tools.contains_key("list_mcp_resources"));
        // 描述与 schema 原样保留
        assert_eq!(r.anthropic_body["tools"][1]["description"], "跑 SQL");
    }

    #[test]
    fn flatten_tool_name_is_valid_deterministic_and_collision_free() {
        // 正常情况只是拼接
        assert_eq!(flatten_tool_name("mcp__a", "b"), "mcp__a__b");

        // 超长：截断到 64 字符并带哈希后缀；同输入同输出，不同输入不同输出
        let long_leaf = "x".repeat(80);
        let a = flatten_tool_name("mcp__codex_apps__github", &long_leaf);
        let b = flatten_tool_name("mcp__codex_apps__github", &format!("{long_leaf}y"));
        assert_eq!(a.len(), 64);
        assert_eq!(a, flatten_tool_name("mcp__codex_apps__github", &long_leaf));
        assert_ne!(a, b);

        // 非法字符被替换，且不会因替换后相同而撞名
        let dot = flatten_tool_name("mcp__a", "get.file");
        let slash = flatten_tool_name("mcp__a", "get/file");
        assert!(
            dot.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        );
        assert_ne!(dot, slash);

        // 非 ASCII 不会让按字节截断 panic
        let cjk = flatten_tool_name("mcp__a", &"查".repeat(40));
        assert!(cjk.len() <= 64);
        assert!(cjk.is_ascii());
    }

    #[test]
    fn history_function_call_with_namespace_uses_the_same_flat_name_as_declaration() {
        let r = convert_ok(json!({
            "model": "gpt-5.6-terra",
            "input": [
                {"type": "message", "role": "user", "content": "查一下"},
                {"type": "function_call", "call_id": "call_1", "name": "execute_sql",
                 "namespace": "mcp__dbhub", "arguments": "{\"sql\":\"select 1\"}"},
                {"type": "function_call_output", "call_id": "call_1", "output": "ok"},
                {"type": "function_call", "call_id": "call_2", "name": "wait",
                 "namespace": "functions", "arguments": "{}"},
                {"type": "function_call_output", "call_id": "call_2", "output": "done"},
            ],
            "tools": [{"type": "namespace", "name": "mcp__dbhub", "tools": [
                {"type": "function", "name": "execute_sql",
                 "parameters": {"type": "object", "properties": {}}},
            ]}],
        }));
        let declared = r.anthropic_body["tools"][0]["name"].as_str().unwrap();
        let used: Vec<&str> = r.anthropic_body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|m| m["content"].as_array())
            .flatten()
            .filter(|b| b["type"] == "tool_use")
            .map(|b| b["name"].as_str().unwrap())
            .collect();
        // 回放的调用与声明同名，默认命名空间的调用保持裸名
        assert_eq!(declared, "mcp__dbhub__execute_sql");
        assert_eq!(used, vec!["mcp__dbhub__execute_sql", "wait"]);
    }

    #[test]
    fn non_function_tool_inside_named_namespace_is_skipped() {
        let r = convert_ok(json!({
            "model": "gpt-5.6-terra",
            "input": "hi",
            "tools": [{"type": "namespace", "name": "mcp__x", "tools": [
                {"type": "custom", "name": "freeform", "description": "d"},
            ]}],
        }));
        assert!(r.anthropic_body.get("tools").is_none());
        assert!(r.namespaced_tools.is_empty());
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

    // ---------- function_call_output 内嵌图片（Codex view_image / 截图类工具） ----------

    fn tool_output_request(output: Value) -> Value {
        json!({
            "model": "gpt-5-codex",
            "input": [
                {"type": "message", "role": "user", "content": "看一下截图"},
                {"type": "function_call", "call_id": "call_img", "name": "view_image", "arguments": "{}"},
                {"type": "function_call_output", "call_id": "call_img", "output": output},
            ],
        })
    }

    fn tool_result_content(r: &ConvertedResponsesRequest) -> Value {
        r.anthropic_body["messages"][2]["content"][0]["content"].clone()
    }

    #[test]
    fn function_call_output_image_only_becomes_image_block() {
        let r = convert_ok(tool_output_request(json!([
            {"type": "input_image", "image_url": "data:image/png;base64,QUJD"},
        ])));
        assert_eq!(
            tool_result_content(&r),
            json!([{
                "type": "image",
                "source": {"type": "base64", "media_type": "image/png", "data": "QUJD"},
            }])
        );
    }

    #[test]
    fn function_call_output_mixed_text_and_image_keeps_order() {
        let r = convert_ok(tool_output_request(json!([
            {"type": "input_text", "text": "before"},
            {"type": "input_image", "image_url": "data:image/jpeg;base64,REVG"},
            "after",
        ])));
        assert_eq!(
            tool_result_content(&r),
            json!([
                {"type": "text", "text": "before"},
                {"type": "image", "source": {"type": "base64", "media_type": "image/jpeg", "data": "REVG"}},
                {"type": "text", "text": "after"},
            ])
        );
    }

    #[test]
    fn function_call_output_remote_image_url_degrades_to_text_placeholder() {
        let r = convert_ok(tool_output_request(json!([
            {"type": "input_image", "image_url": "https://example.com/a.png"},
        ])));
        assert_eq!(
            tool_result_content(&r),
            json!([{"type": "text", "text": "[image: https://example.com/a.png]"}])
        );
    }

    #[test]
    fn function_call_output_unconvertible_image_leaves_placeholder_not_empty_content() {
        // 缺 image_url / image_url 为空串：不能被静默丢弃成空 content 数组
        let r = convert_ok(tool_output_request(json!([
            {"type": "input_image"},
            {"type": "input_image", "image_url": ""},
        ])));
        let placeholder =
            json!({"type": "text", "text": "[image omitted: missing or empty image_url]"});
        assert_eq!(tool_result_content(&r), json!([placeholder, placeholder]));
    }

    #[test]
    fn function_call_output_text_only_array_stays_flattened_string() {
        let r = convert_ok(tool_output_request(json!([
            {"type": "input_text", "text": "a"},
            {"type": "output_text", "text": "b"},
            {"output": "c"},
        ])));
        assert_eq!(tool_result_content(&r), json!("a\nb\nc"));
    }

    #[test]
    fn function_call_output_image_reaches_kiro_images() {
        let r = convert_ok(tool_output_request(json!([
            {"type": "input_image", "image_url": "data:image/png;base64,QUJD"},
        ])));
        let anthropic: crate::anthropic::types::MessagesRequest =
            serde_json::from_value(r.anthropic_body).expect("anthropic body 应可反序列化");
        let kiro = crate::anthropic::convert_request(&anthropic).expect("转换 Kiro 请求应成功");
        let current = &kiro.conversation_state.current_message.user_input_message;
        assert_eq!(current.images.len(), 1, "工具图片应进入 Kiro images");
        assert_eq!(current.images[0].source.bytes, "QUJD");
        let results = &current.user_input_message_context.tool_results;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tool_use_id, "call_img");
    }
}
