# 任务清单：fix-retry-permit-release-and-token-dedup

## 状态：ARCHIVED

## 任务
- [x] 任务1：`src/kiro/provider.rs` — `call_api_with_retry` 5 处退避 sleep 前插入 `drop(_cred_permit)`，并将 `wait_for_rpm_gate` 调用调整到 `_cred_permit` 获取之前；验证：`cargo check` 编译通过，无 moved-value 错误
- [x] 任务2：`src/kiro/provider.rs` — `call_mcp_with_retry` 4 处退避 sleep 前插入 `drop(_cred_permit)`，并将 `wait_for_rpm_gate` 调用调整到 `_cred_permit` 获取之前；验证：`cargo check` 编译通过
- [x] 任务3：`src/token.rs` — 新增 `count_message_tokens` 辅助函数（`count_all_tokens_local`/`count_prefix_tokens` 同步改为调用该函数）+ `count_all_tokens_with_prefix` 函数（含空 `messages` 边界处理）；验证：新增单元测试断言与原 `count_all_tokens_local`/`count_prefix_tokens` 组合结果一致，含空 messages 场景
- [x] 任务4：`src/anthropic/handlers.rs` — `post_messages`、`post_messages_cc` 两处改为调用 `count_all_tokens_with_prefix`；验证：`cargo test` 全绿，`cargo clippy` 无新增警告

## 验收标准
- [x] `cargo build --release` 编译通过
- [x] `cargo test` 全部通过（含新增等价性单测）：572 passed; 0 failed
- [x] `cargo clippy` 无新增 warning：改动前后均为 71 warnings（预置警告，非本次引入）
- [x] provider.rs 全部 9 处退避 sleep 调用点均在 sleep 前完成 `_cred_permit` drop
- [x] provider.rs 两处 `wait_for_rpm_gate` 调用均已移至 `_cred_permit` 获取之前
- [x] handlers.rs 两处调用点的 `input_tokens` 计算结果与改动前完全一致（含空 messages 边界）：新增单测覆盖等价性
