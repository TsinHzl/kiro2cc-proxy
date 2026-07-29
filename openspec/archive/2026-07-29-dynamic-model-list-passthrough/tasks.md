# 任务清单：dynamic-model-list-passthrough

## 状态：ARCHIVED

## 任务

- [x] 任务 1：`src/model/config.rs` 新增 `model_cache_ttl_secs: u64`（serde alias `modelCacheTtlSecs`，默认 3600），纳入 `apply_env_overrides()`
      - spec.md 引用：配置增量 / modelCacheTtlSecs 配置项
      - 验证：cargo check + 配置反序列化默认值单测
- [x] 任务 2：`src/anthropic/converter.rs` `map_model()` 增加开放透传兜底（非空 ID 剥离 `-thinking` 后透传；空 ID 仍 `None`；gpt 未命中变体也走透传）
      - spec.md 引用：未知模型开放透传（3 个场景）
      - **注意**：现有测试 `test_map_model_unsupported`（converter.rs:1876）断言 `map_model("gpt-4").is_none()`，本任务将使其变为 `Some("gpt-4")`，必须同步改写该断言为透传预期（否则任务 7 全量 test 无法通过）
      - 验证：新增单测——未知 ID 透传、`-thinking` 剥离、空串拒绝、内置规则不受影响；改写 `test_map_model_unsupported`
- [x] 任务 3：抽取 `guess_owned_by()` 为共享函数，`admin/service.rs` 改为引用；新增 `AvailableModelInfo -> Model` 映射函数
      - 验证：cargo check + 映射函数单测（owned_by 推断、max_tokens 取值）
- [x] 任务 4：`AppState` 新增模型缓存字段 `Arc<RwLock<Option<CachedModels>>>` 及初始化
      - 影响评估：AppState 为 Clone，新增 Arc 字段不破坏克隆语义；不影响现有 middleware
      - 验证：cargo check
- [x] 任务 5：实现 `fetch_models_dynamic(&AppState) -> Vec<Model>`（缓存命中 / 过期刷新 / 失败续用旧缓存 / 无缓存回退静态表 / 无 provider 回退静态表）
      - spec.md 引用：/v1/models 动态来源（前 4 个场景）
      - 影响评估：调用 `token_manager.list_available_models()`，其内部经 `acquire_context()` 会触发 balanced 模式账号轮询——仅列表端点低频调用（TTL 默认 1h），可接受，但需在实现中说明
      - 验证：单测覆盖 5 条分支——①缓存命中不打上游 ②过期刷新成功写缓存 ③过期刷新失败续用旧缓存 ④无缓存刷新失败回退静态表 ⑤无 provider 直接回退静态表
- [x] 任务 6：`get_models` / `get_model` / 首页 `models_count` 改为走 `fetch_models_dynamic`，路由传入 `State<AppState>`
      - spec.md 引用：GET /v1/models/{id} 与列表来源一致
      - 验证：cargo check + `get_model` 命中/未命中(404) 单测
- [x] 任务 7：`cargo fmt` + `cargo clippy` clean + 全量 `cargo test` 通过
      - 验证：CI 三项全绿（fmt 已格式化；本次新增代码无 clippy 告警，残留告警均为既有测试代码；332 项 test 全过）

## 验收标准

- [ ] `GET /v1/models` 在上游可用时返回实时模型（含 rate 信息可选），TTL 内不重复打上游
- [ ] 上游不可用时 `/v1/models` 仍返回 200（旧缓存或静态表），不报错
- [ ] 未在内置规则中的非空模型 ID 经 `POST /v1/messages` 能透传到上游（不再代理侧 400）
- [ ] 空模型 ID 仍被拒绝
- [ ] 现有所有可调模型名行为不变（内置规则优先级不变）
- [ ] `modelCacheTtlSecs` 默认 3600，可经 env 覆盖
- [ ] cargo fmt / clippy / test 全部通过
