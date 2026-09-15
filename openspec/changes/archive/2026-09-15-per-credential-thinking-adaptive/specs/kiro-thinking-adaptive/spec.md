# Spec: 账号级 thinking adaptive 注入（kiro-thinking-adaptive）

## Purpose

提供账号粒度的 Kiro thinking 调度开关：管理员为某账号开启后，客户端请求携带 `thinking: {"type": "adaptive"}` 且路由到该账号时，代理向 Kiro 上游 `additionalModelRequestFields` 注入 `thinking: {"type": "adaptive"}`，恢复该账号的 thinking 能力；其余一切情况（开关关闭、客户端未请求 adaptive、模型不支持）与 v3.3.0 以来"不发 thinking 字段"的现状逐字节一致。

## ADDED Requirements

### Requirement: 账号级 thinking adaptive 开关持久化

系统 SHALL 在 `KiroCredentials` 上持久化账号级开关 `thinkingAdaptive`（bool，serde default=false），经 Admin API `PUT /credentials/{id}` 的 `thinkingAdaptive` 字段（`Option<bool>`，未提供时不更新）变更，并随账号快照（`CredentialEntrySnapshot`）透出给前端。

#### Scenario: 旧配置文件向后兼容

- **GIVEN** credentials.json 由旧版本生成，某账号对象不含 `thinkingAdaptive` 字段
- **WHEN** 服务加载该 credentials.json
- **THEN** 该账号的 `thinking_adaptive` 反序列化为 `false`
- **AND** credentials.json 中不因加载而出现序列化噪音（bool 字段照常输出，与 `disabled` 字段行为一致）

#### Scenario: Admin API 部分更新开关

- **GIVEN** 存在账号 #N，当前 `thinkingAdaptive = false`
- **WHEN** Admin 调用 `PUT /credentials/N`，body 携带 `{"thinkingAdaptive": true}`
- **THEN** 账号 #N 的 `thinking_adaptive` 更新为 `true` 并持久化
- **AND** 同请求未提供的其他字段（refresh_token、proxy_url 等）不被修改
- **AND** 快照接口返回该账号 `thinkingAdaptive: true`

#### Scenario: 未提供该字段时不更新

- **WHEN** Admin 调用 `PUT /credentials/N`，body 不含 `thinkingAdaptive`
- **THEN** 账号 #N 的 `thinking_adaptive` 保持原值不变

### Requirement: 仅在开关开启且客户端请求 adaptive 时注入 thinking 字段

Provider 层 SHALL 在确定账号（`CallContext`）后，当且仅当同时满足以下全部条件时，向发往 Kiro 上游的 request body 的 `additionalModelRequestFields` 对象注入 `"thinking": {"type": "adaptive"}`：

1. 该账号的 `thinkingAdaptive` 开关为 `true`
2. 本次请求的客户端 `thinking` 配置为 `{"type": "adaptive"}`（converter 透传的标记为 true）
3. 目标模型不属于 GPT 系（`gpt-*`）且不属于 4.5 代际（`*-4.5` / `*4.5` 结尾）

任一条件不满足时，request body 与 v3.3.0 现状逐字节一致。

#### Scenario: 开关开启 + 客户端 adaptive → 注入

- **GIVEN** 账号 A 的 `thinkingAdaptive = true`，客户端请求携带 `thinking: {"type": "adaptive"}`，模型为 claude-sonnet-4.6
- **WHEN** 请求路由到账号 A 并发往 Kiro 上游
- **THEN** request body 的 `additionalModelRequestFields` 含 `"thinking": {"type": "adaptive"}`
- **AND** 其余字段（output_config、max_tokens）保持既有构建逻辑不变

#### Scenario: 开关关闭 → 不注入（现状不变）

- **GIVEN** 账号 B 的 `thinkingAdaptive = false`，客户端请求携带 `thinking: {"type": "adaptive"}`
- **WHEN** 请求路由到账号 B
- **THEN** request body 不含 `additionalModelRequestFields.thinking`
- **AND** body 与 v3.3.0 行为逐字节一致

#### Scenario: 客户端未请求 adaptive → 不注入

- **GIVEN** 账号 A 的 `thinkingAdaptive = true`
- **WHEN** 客户端请求不携带 thinking，或携带 `thinking: {"type": "enabled"}`
- **THEN** request body 不含 `additionalModelRequestFields.thinking`

#### Scenario: GPT 系与 4.5 代际模型 → 不注入

- **GIVEN** 账号 A 的 `thinkingAdaptive = true`，客户端请求携带 `thinking: {"type": "adaptive"}`
- **WHEN** 目标模型为 `gpt-5.6-*` 系列或 4.5 代际（`claude-sonnet-4-5` 等）
- **THEN** request body 不含 thinking 注入（这些模型整体跳过 additionalModelRequestFields，注入会 400 REQUEST_BODY_INVALID）

#### Scenario: 故障转移后按新账号重新判定

- **GIVEN** 账号 A（开关开）与账号 B（开关关）均可用，客户端请求携带 adaptive
- **WHEN** 首选账号 A 请求失败，故障转移到账号 B 重试
- **THEN** 发往账号 B 的 request body 不含 thinking 注入（按账号 B 的开关值判定）
- **AND** 每次重试都以当次实际选中的账号状态为准

## 接口说明

本变更为 Admin 管理面行为，不影响 `/v1/messages` 与 `/cc/v1/messages` 的对外协议：两端点对客户端的请求/响应格式零变化，差异仅体现在代理 → Kiro 上游的 request body（且仅当账号开关开启）。
