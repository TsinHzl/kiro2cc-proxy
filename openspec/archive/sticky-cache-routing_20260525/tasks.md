> **注：** 本文档由 **claude-sonnet-4-6** 模型自动生成。

# 任务清单：sticky-cache-routing

## 状态：DONE

## 任务

- [x] T1：在 `token_manager.rs` 中添加 `StickyCacheEntry` 结构体和 `STICKY_CACHE_TTL` 常量
- [x] T2：在 `MultiTokenManager` struct 中添加 `sticky_cache` 字段，并在 `new()` 中初始化
- [x] T3：在 `MultiTokenManager` 中实现 `acquire_context_sticky` 方法（含懒惰 GC）
- [x] T4：在 `provider.rs` 中添加 `extract_continuation_id_from_request` 辅助函数
- [x] T5：将 `call_api_with_retry` 改用 `acquire_context_sticky`
- [x] T6：将 `call_mcp_with_retry` 改用 `acquire_context_sticky`
- [x] T7：为 `acquire_context_sticky` 添加单元测试（命中/过期/驱逐/缺失 ID 四个场景）
- [x] T8：`cargo check` + `cargo clippy` 验证

## 验收标准

- [x] 同一 `agentContinuationId` 的连续请求路由到同一凭据
- [x] TTL 过期后自动重新选择凭据
- [x] 缓存命中的凭据不健康（disabled 或 Unhealthy）时驱逐并重选
- [x] `continuation_id = None` 时完全退化为原有 `acquire_context_filtered` 行为
- [x] `cargo check` 通过，`cargo clippy` 无新 warning
