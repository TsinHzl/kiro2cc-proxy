# 任务清单：add-gpt-cache-session-identity

## 状态：ARCHIVED

## 任务
- [x] 任务 1：converter.rs `derive_fallback_conversation_id` — 移除 system+tools 皆空时返回 None 的短路，裸请求改用首条消息派生稳定 conversationId；改写文档注释（issue #27 防御性设计已不再必要：seed 已含首条消息，折叠需同时满足相同 system+相同 tools+相同首条消息，实测收益 37.9% 远大于收窄后的风险）（验证：改写 `test_derive_fallback_none_without_system_and_tools` 为新行为断言 + 新增裸请求连续两轮稳定性测试 + 保留 `test_derive_fallback_distinguishes_different_sessions` 通过）
- [x] 任务 2：converter.rs `extract_session_id` — 新增「字符串本身是合法 UUID」直通分支（`is_valid_uuid(user_id)` → Some），使 OpenAI `user` 为纯 UUID 时走 metadata 路径（验证：新增纯 UUID 直通单测 + 既有 extract_session_id 测试群全通过）
- [x] 任务 3：openai/chat_request.rs — `body.user` 非空时写入 `anthropic["metadata"]["user_id"]`（验证：新增 user 透传单测：带 user 转换后 metadata 存在、不带 user 不存在；补一条非 UUID user（如 `user-123`）同一请求体连续两次转换得到相同 conversationId 的断言，锚定「fallback 兜底不退化」端到端链路）
- [x] 任务 4：openai/responses_request.rs — 同任务 3（验证：新增 user 透传单测 + 非 UUID user 连续两次转换相同 conversationId 断言）
- [x] 任务 5：cargo fmt + cargo clippy clean + cargo test 全量回归

## 验收标准
- [x] 裸请求（无 metadata/system/tools）相同首条消息连续两轮派生相同 conversationId（单测断言）；不同首条消息派生不同 ID（issue #27 回归测试保持通过）
- [x] OpenAI chat/completions 与 responses 请求带纯 UUID `user` 字段时，转换后 Anthropic body 含 metadata.user_id 且值等于该 UUID（单测断言）；`user` 非空但非 UUID 时同样写入 metadata.user_id（由 extract_session_id 直通/解析或 fallback 兜底，不退化为随机 UUID）
- [x] cargo check / cargo test / cargo clippy 全部通过
