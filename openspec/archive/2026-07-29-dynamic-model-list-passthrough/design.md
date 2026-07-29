# 设计：dynamic-model-list-passthrough

## 上下文

`/v1/models` 与 `map_model()` 是两条独立链路：前者管"展示什么"，后者管"能调什么"。本次要让两者都随上游动态化，且保持对上游不可用的健壮回退。底层 `list_available_models()` / `KiroProvider::token_manager()` / `AppState.kiro_provider` 已就绪。

## 目标 / 非目标

**目标**
- `/v1/models` 反映账号真实可用模型，带缓存、失败回退
- 未知模型开放透传，"看到即可用"
- 复用现有 admin 端映射逻辑，不重复实现

**非目标**
- 不做按凭据缓存 / 按分组聚合 / 凭据路由优先（上游 commit 的重度部分）
- 不改请求转换、流式、帧解析核心链路
- 不引入新 crate

## 决策

### 决策 1：缓存放在 AppState，用 `Arc<RwLock<Option<CachedModels>>>`
- `CachedModels { models: Vec<Model>, fetched_at: Instant }`。
- 读多写少，`RwLock` 合适；TTL 判断用 `Instant::elapsed()`。
- 放 `AppState` 而非全局 static（符合 config 约束"禁止全局 static mut"，状态经 `Arc<T>` 传递）。
- 备选：放 `KiroProvider` 内部。否决——`get_models` 直接从 `State` 取更直观，且 provider 可能为 `None`（未配置上游）时也要能回退静态表。

### 决策 2：动态获取封装为独立 async 函数 `fetch_models_dynamic(&AppState) -> Vec<Model>`
- 内含"缓存命中→过期刷新→失败回退"全部逻辑，`get_models` / `get_model` / 统计三处共用，避免来源分叉。
- `kiro_provider` 为 `None` 时直接返回 `build_model_list()`（无上游可查）。

### 决策 3：开放透传在 `map_model` 末尾兜底分支实现
- 现有 `else { None }` 改为：`let m = model.trim(); if m.is_empty() { None } else { Some(strip_thinking_suffix(m)) }`。
- `-thinking` 后缀剥离：thinking 由 `override_thinking_from_model_name` / additionalModelRequestFields 单独控制，透传的后端 ID 不应带 `-thinking`，与内置规则对 `-thinking` 的处理保持一致。
- 内置关键词分支全部保持在前，优先级不变；仅"全都没命中"时才透传。
- `gpt` 分支内部当前对无 sol/terra/luna/5.6 的情况返回 `None`——改为也走透传兜底（保持一致性），避免 `gpt-` 开头的新变体被拒。

### 决策 4：`guess_owned_by` 从 `admin/service.rs` 提升为共享函数
- 移到 handlers（或公共位置）供两处引用，保持 `owned_by` 命名一致；admin 侧改为引用，避免两份实现漂移。

## 风险 / 权衡

- **透传扩大上游请求面**：仅字符串透传，无效 ID 上游快速拒绝；日志记录便于观测异常调用。
- **缓存与真实性权衡**：TTL 默认 1h，牺牲部分实时性换取稳定与低上游压力；失败续用旧缓存进一步稳态。
- **写锁短暂阻塞**：刷新时持写锁，但仅列表端点路径经过，不影响 `/v1/messages` 转换链路。
