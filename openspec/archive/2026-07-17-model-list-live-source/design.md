# 设计文档：model-list-live-source

## 上下文

`add-model-rate-multiplier` 已验证 `ListAvailableModels` 可实时调用并优雅降级。用户本次抓包给出了完整响应体结构，确认单条模型条目含 `modelId`/`modelName`/`rateMultiplier`/`tokenLimits.{maxInputTokens,maxOutputTokens}` 等字段，足以完全替代本地静态表作为模型行数据源，无需再做 `map_model()` 归一化 join。

## 目标 / 非目标

**目标：**
- 支持模型页的模型行数据源从"本地静态表 + 归一化查费率" 改为"直接使用实时接口返回的模型行"
- 上游失败时保持与当前一致的静态列表兜底，不引入新的失败态

**非目标：**
- 不追求 `owned_by`/`display_name` 的人类友好包装（如实映射接口字段，不额外美化）
- 不改变 `/v1/models` 或客户端请求路由逻辑（`map_model()` 在 `converter.rs` 内部仍保留，仅收回 admin 模块对它的可见性依赖）

## 决策

1. **不再需要归一化 join**：此前 `map_model(&model.id)` 的作用是把本地静态 id 转成 Kiro 真实 `modelId` 去查费率表；现在模型行本身就是接口返回的原始 `modelId`，join 步骤整体消失，`AdminService` 只需一次映射（成功路径）或一次映射（失败路径），二选一。
2. **保持 `AdminModelItem` 对外 JSON 形状不变**：继续 `#[serde(flatten)] model: Model` + `rate_multiplier`。成功路径下人工构造 `Model { created: 0, object: "model".into(), model_type: "chat".into(), ... }`——`created`/`object`/`type` 三个字段接口未提供对应真实值，用固定占位符延续现有契约，避免前端改动。

2.5. **映射逻辑拆成纯函数，脱离网络依赖可测**：`AdminService::list_admin_models()` 内部只做"调用 + 分支"，实际映射交给两个纯函数：
   - `fn live_model_to_admin_item(info: &AvailableModelInfo) -> AdminModelItem`（成功路径，单条映射）
   - `fn fallback_model_to_admin_item(model: Model) -> AdminModelItem`（失败路径，单条映射，`rate_multiplier: None`）
   两者均不涉及 `async`/网络调用，可直接用 fake `AvailableModelInfo`/`Model` 构造实例单测，复用 `add-model-rate-multiplier` 中 `build_admin_model_items` 已验证的"纯函数可测"模式。
2.6. **反序列化容错 + 真实抓包回归测试**：`TokenLimits` 派生 `Default`，`AvailableModelInfo` 的 `model_name`/`token_limits` 均加 `#[serde(default)]`——上游若缺失/新增/重排字段，反序列化仍成功（缺失字段取默认值），不会因单条模型的字段异常导致整个 `models[]` 解析失败进而静默触发全局回退。同时新增一条基于用户提供的真实抓包 JSON 字面量的反序列化单测，直接验证 `AvailableModelsResponse` 对当前真实响应结构的字段映射，作为后续上游协议漂移的回归防线。
3. **`owned_by` 用前缀启发式**：`guess_owned_by(model_id: &str) -> &'static str`，覆盖当前已知前缀（`claude`/`gpt`/`deepseek`/`minimax`/`glm`/`qwen`/`auto`），未知前缀返回 `"unknown"`。这是纯展示层的最佳猜测，不影响费率/路由等核心逻辑。
4. **失败判定与降级复用现有边界**：`MultiTokenManager::list_available_models()` 返回 `Err` 时（无可用账号/网络错误/非 2xx/反序列化失败）触发降级；`Ok` 即使 `models` 为空数组也直接展示空结果（如实反映接口返回，不额外判空降级——避免把"接口返回空列表"和"接口调用失败"两种不同语义混为一谈）。
5. **收回 `converter` 模块可见性**：确认 `map_model` 无其他外部调用方后，将 `src/anthropic/mod.rs` 的 `mod converter;` 可见性改回私有，消除 `add-model-rate-multiplier` 遗留的、本次改动后已不再需要的可见性放宽。

## 风险 / 权衡

- `owned_by` 启发式会在 Kiro 上新厂商前缀时显示 `unknown`，这是已接受的展示降级，不阻塞功能
- 新增字段的 `#[serde(default)]` 容错只解决"字段缺失/新增"级别的协议漂移，无法覆盖"字段类型不兼容"（如 `maxOutputTokens` 从数字变成字符串）——这类情况仍会导致该条模型反序列化失败；因 `serde_json` 默认按元素级失败而非整体失败传播到 `Vec<AvailableModelInfo>` 的上一层（`Vec` 反序列化本身仍是整体失败的），此风险与"上游协议为私有、无文档"的既有风险同源，不单独引入新的处理机制
- 移除 `map_model` 依赖及收回模块可见性属于范围内的清理性改动，回归风险低（`cargo check` 可立即验证是否有遗漏调用方）
- **模型集合从客户端别名变为 Kiro 原生家族**（约 32 条 → 约 16 条，不含 `-thinking` 独立条目和历史日期式别名）：已向用户说明并经其明确选择接受，非本设计遗漏
