# 变更提案：dynamic-model-list-passthrough

## 背景

客户端可见的 `GET /v1/models` 目前返回 `build_model_list()`（`src/anthropic/handlers.rs:187`）——一张 333 行的手写静态表。上游 Kiro 每次上新模型都要手动改代码补条目，列表内容与账号真实可用模型脱节，也不含官方费率倍率。

与此同时，实际"能调哪些模型"由 `map_model()`（`src/anthropic/converter.rs:421`）的关键词匹配决定：只有命中 `sonnet`/`opus`/`gpt`/`glm` 等内置规则的模型才会被映射到 Kiro 模型 ID，未命中直接返回 `None` → `UnsupportedModel` 报错。因此即便上游上了新模型，客户端也无法调用，除非改代码新增映射规则。

底层能力已具备但未接通到 `/v1/models`：
- `MultiTokenManager::list_available_models()`（`token_manager.rs:1768`）已能拉取上游 `ListAvailableModels`
- Admin 端 `list_admin_models()`（`admin/service.rs:249`）已用它做实时展示（失败回退静态表）
- `KiroProvider::token_manager()`（`provider.rs:132`）可取到 manager，且 `AppState.kiro_provider` 已挂在 `/v1` 路由 state 上

缺口在于：`get_models()` 签名不接收 `State`，只能返回写死的表；`map_model()` 对未知模型一律拒绝。

## 目标范围

**在范围内：**
- `GET /v1/models` 改为动态来源：优先取上游 `ListAvailableModels` 实时响应，带 TTL 缓存，失败回退 `build_model_list()` 静态表
- 新增配置项 `modelCacheTtlSecs`（默认 3600 秒），支持 env override
- `map_model()` 增加开放透传：未命中内置关键词规则的非空合法模型 ID，原样透传给 Kiro（不再报 `UnsupportedModel`），可用性交由上游判断
- `GET /v1/models/{id}` 与首页 `models_count` 统计随动态列表联动（复用同一来源）
- 抽取 `guess_owned_by()` 等映射逻辑为 handlers/admin 共享，消除重复

**不在范围内：**
- 不改 `POST /v1/messages` 请求转换、流式状态机、二进制帧解析核心链路
- 不引入按凭据缓存模型 + 按客户端 Key 分组聚合 + 凭据路由优先（参考上游 commit 的重度部分，留待后续）
- 不实现 `customModels` 自定义别名（本仓库无此功能，本次不新增）
- 不改多账号故障转移 / 负载均衡逻辑
- **不给 `/v1/models` 的 `Model` 结构新增 `rate_multiplier` 字段**：费率倍率目前仅在 Admin 专用 `AdminModelItem` 暴露；本次动态化仅让列表内容随上游更新，`Model` 结构（`types.rs:45`）保持不变，官方费率倍率仍不通过 `/v1/models` 对客户端暴露（背景所述该缺口本轮不解决）

## 技术方案

1. **动态列表 + TTL 缓存**：`AppState` 新增 `Arc<RwLock<Option<CachedModels>>>` 缓存字段（带刷新时间戳）。`get_models` 接收 `State<AppState>`，TTL 内命中返回缓存；过期则调 `list_available_models()` 刷新。刷新失败时：有旧缓存续用旧缓存，无缓存回退静态表，保证端点永不失败。
2. **上游→Anthropic 映射**：把 `AvailableModelInfo` 映射为 `Model`，复用现有 `guess_owned_by()` 推断 `owned_by`，`max_tokens` 取 `token_limits.max_output_tokens`。
3. **开放透传**：`map_model()` 末尾兜底分支由 `None` 改为——对 trim 后非空的模型 ID，剥离可选 `-thinking` 后缀后原样返回 `Some(id)`；仍保留对空字符串的拒绝。内置关键词规则优先级不变（命中规则的仍走规范化映射）。
4. **配置**：`src/model/config.rs` 加 `model_cache_ttl_secs: u64`（serde 别名 `modelCacheTtlSecs`，默认 3600），纳入 `apply_env_overrides()`。

## 预期影响

- **行为变化**：`/v1/models` 内容从固定表变为账号真实可用模型；此前被拒的未知模型 ID 现在会被透传（可能从代理侧 400 变为上游侧成功或上游侧拒绝）。
- **性能**：新增一次上游调用，但有 TTL 缓存（默认 1 小时）摊薄；列表端点从 O(1) 内存构造变为缓存命中 O(1) / 未命中一次网络调用。
- **兼容性**：静态表保留为回退，上游不可用时行为与现状一致；现有能调的模型名全部继续可用（内置规则不变）。
- **安全**：开放透传扩大了可发往上游的模型 ID 集合，但仅透传字符串，最终鉴权/可用性由上游把关，代理侧不放宽认证。

## 风险

| 风险 | 应对 |
|---|---|
| 上游 `ListAvailableModels` 抖动导致列表频繁变化 | TTL 缓存（默认 1h）+ 失败续用旧缓存，避免抖动直接透传到客户端 |
| 开放透传让无效模型 ID 打到上游产生无意义请求 | 仅透传非空字符串；无效 ID 上游会快速拒绝，不影响正常模型；日志记录透传的未知模型便于观测 |
| 缓存字段并发读写 | 用 `RwLock`，读多写少；刷新时短暂持写锁，不阻塞请求转换链路 |
| `get_model{id}` / 统计与列表来源不一致 | 三者统一走同一动态来源函数，避免分叉 |
| `list_available_models()` 经 `acquire_context()` 触发 balanced 账号轮询，扰动本应由消息请求驱动的轮转状态 | TTL 默认 1h，列表端点调用频率极低，对轮转影响可忽略；不改 `select_next_credential` 逻辑，仅复用现有取 context 路径 |
