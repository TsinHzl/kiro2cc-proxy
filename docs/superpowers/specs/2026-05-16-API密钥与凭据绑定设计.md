> **注：** 本文档由 **claude-sonnet-4-6** 模型自动生成。

# API Key 凭据绑定功能设计

## 概述

为 API Key 管理新增凭据绑定能力：创建或编辑 key 时，可选择将该 key 绑定到一个或多个 Kiro 凭据。绑定后，该 key 的所有请求只能使用绑定的凭据，不会路由到其他凭据。未绑定的 key 行为保持不变，继续使用全局负载均衡策略。

## 需求

- 创建/编辑 API Key 时，可选择绑定 0 个或多个凭据
- 绑定后，该 key 的请求严格限定在绑定的凭据池内（不故障转移到其他凭据）
- 绑定多个凭据时，凭据间的选择策略继承全局负载均衡模式（priority / balanced）
- 绑定的所有凭据均不可用时，直接返回错误
- 未绑定凭据的 key 行为不变

## 方案选择

采用方案 A：在 `ApiKey` 模型上加 `bound_credential_ids` 字段，`MultiTokenManager` 新增带过滤的 `acquire_context_filtered` 方法。改动最小，逻辑集中，完全向后兼容。

## 数据模型变更

### `src/model/api_key.rs`

`ApiKey` 结构体新增字段：

```rust
/// 绑定的凭据 ID 列表，None 或空列表表示不限制（使用全局策略）
#[serde(default)]
#[serde(skip_serializing_if = "Option::is_none")]
pub bound_credential_ids: Option<Vec<u64>>,
```

`ApiKey::new()` 签名增加 `bound_credential_ids: Option<Vec<u64>>` 参数。

`ApiKeyManager::create()` 和 `update()` 透传该字段。

### `src/admin/types.rs`

`CreateApiKeyRequest` 和 `UpdateApiKeyRequest` 同步增加：

```rust
#[serde(default)]
pub bound_credential_ids: Option<Vec<u64>>,
```

## token_manager 过滤逻辑

### `src/kiro/token_manager.rs`

新增方法：

```rust
pub async fn acquire_context_filtered(
    &self,
    model: Option<&str>,
    allowed_ids: &[u64],
) -> anyhow::Result<CallContext>
```

- 逻辑与 `acquire_context` 完全一致
- 在 `select_next_credential` 的可用凭据过滤步骤中额外加：
  ```rust
  if !allowed_ids.is_empty() && !allowed_ids.contains(&e.id) {
      return false;
  }
  ```
- 绑定的所有凭据均不可用时返回错误：`"绑定的凭据均不可用（共 N 个）"`
- 原有 `acquire_context` 不动

## handler 层调用路径

### `src/common/auth.rs`

`ApiKeyInfo` 结构体增加：

```rust
pub bound_credential_ids: Option<Vec<u64>>,
```

middleware 认证通过后从 `ApiKey` 复制该字段。

### `src/kiro/provider.rs`

`send_message`（及 stream/non-stream/websearch 三个调用点）增加 `bound_ids: &[u64]` 参数：

```rust
if bound_ids.is_empty() {
    self.token_manager.acquire_context(model).await
} else {
    self.token_manager.acquire_context_filtered(model, bound_ids).await
}
```

### `src/anthropic/handlers.rs`

在调用 `provider.send_message()` 前提取绑定列表：

```rust
let bound_ids = identity
    .as_ref()
    .and_then(|ext| ext.0.bound_credential_ids.as_deref())
    .unwrap_or(&[]);
```

## Admin UI 变更

### `admin-ui/src/types/api.ts`

`ApiKey` 类型增加：

```ts
boundCredentialIds?: number[];
```

### `admin-ui/src/api/credentials.ts`

`createApiKey` / `updateApiKey` 请求体增加 `boundCredentialIds?: number[]`。

### `admin-ui/src/components/api-keys-panel.tsx`

创建/编辑 key 对话框新增多选控件：

- 标签：**绑定凭据**
- 控件：多选 checkbox 列表，列出所有凭据（显示凭据备注名 + ID）
- 不选 = 不绑定（使用全局策略）
- key 列表行以 badge 展示已绑定的凭据
- 复用已有 `useCredentials` hook 获取凭据列表

## 错误处理

| 场景 | 行为 |
|------|------|
| 绑定的凭据全部不可用 | 返回 503，不故障转移 |
| 绑定的凭据 ID 不存在 | 视为不可用，同上 |
| 未绑定凭据的 key | 走原有全局策略，不受影响 |

## 文件变更清单

| 文件 | 变更类型 |
|------|---------|
| `src/model/api_key.rs` | 新增字段 + 修改 new/create/update 签名 |
| `src/admin/types.rs` | 新增请求字段 |
| `src/admin/api_keys.rs` | 透传新字段 |
| `src/common/auth.rs` | ApiKeyInfo 新增字段 |
| `src/kiro/token_manager.rs` | 新增 acquire_context_filtered 方法 |
| `src/kiro/provider.rs` | send_message 增加 bound_ids 参数 |
| `src/anthropic/handlers.rs` | 提取 bound_ids 并传入 provider |
| `admin-ui/src/types/api.ts` | 新增类型字段 |
| `admin-ui/src/api/credentials.ts` | 新增请求字段 |
| `admin-ui/src/components/api-keys-panel.tsx` | 新增多选凭据控件 |
