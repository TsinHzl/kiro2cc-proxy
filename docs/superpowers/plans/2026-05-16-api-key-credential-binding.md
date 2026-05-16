> **注：** 本文档由 **claude-sonnet-4-6** 模型自动生成。

# API Key 凭据绑定 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 API Key 新增 `boundCredentialIds` 字段，使该 key 的所有请求只路由到绑定的凭据，绑定凭据全部不可用时直接报错，不故障转移。

**Architecture:** `ApiKey` 模型加字段 → `ApiKeyManager::create/update` 透传 → middleware 将字段复制到 `ApiKeyContext` → handler 提取后传给 `KiroProvider::call_api*` → `MultiTokenManager::acquire_context_filtered` 按 ID 白名单过滤凭据。

**Tech Stack:** Rust (axum, serde, parking_lot), TypeScript/React (admin-ui, TanStack Query)

---

### Task 1: ApiKey 模型新增 bound_credential_ids 字段

**Files:**
- Modify: `src/model/api_key.rs`

- [ ] **Step 1: 在 ApiKey 结构体中新增字段**

在 `activated_at` 字段之后添加：

```rust
/// 绑定的凭据 ID 列表，None 或空列表表示不限制（使用全局策略）
#[serde(default)]
#[serde(skip_serializing_if = "Option::is_none")]
pub bound_credential_ids: Option<Vec<u64>>,
```

- [ ] **Step 2: 修改 ApiKey::new() 签名，增加参数并初始化**

```rust
pub fn new(
    id: u32,
    name: String,
    expires_at: Option<DateTime<Utc>>,
    spending_limit: Option<f64>,
    duration_days: Option<f64>,
    bound_credential_ids: Option<Vec<u64>>,
) -> Self {
    Self {
        id,
        key: generate_api_key(),
        name,
        enabled: true,
        created_at: Utc::now(),
        expires_at,
        spending_limit,
        duration_days,
        activated_at: None,
        bound_credential_ids,
    }
}
```

- [ ] **Step 3: 修改 ApiKeyManager::create() 签名，透传新字段**

```rust
pub fn create(
    &self,
    name: String,
    expires_at: Option<DateTime<Utc>>,
    spending_limit: Option<f64>,
    duration_days: Option<f64>,
    bound_credential_ids: Option<Vec<u64>>,
) -> anyhow::Result<ApiKey> {
    let mut keys = self.keys.write();
    let next_id = keys.iter().map(|k| k.id).max().unwrap_or(0) + 1;
    let api_key = ApiKey::new(next_id, name, expires_at, spending_limit, duration_days, bound_credential_ids);
    keys.push(api_key.clone());
    drop(keys);
    self.save()?;
    Ok(api_key)
}
```

- [ ] **Step 4: 修改 ApiKeyManager::update()，支持更新 bound_credential_ids**

在 `update` 方法的参数列表末尾增加：

```rust
bound_credential_ids: Option<Option<Vec<u64>>>,
```

在方法体内，在 `duration_days` 处理之后添加：

```rust
if let Some(ids) = bound_credential_ids {
    api_key.bound_credential_ids = ids;
}
```

- [ ] **Step 5: 构建验证**

```bash
cd "/Users/MacBook/My Files/Code/sourcetree/kiro2cc-proxy"
cargo build 2>&1 | head -40
```

预期：编译错误，提示 `create` 调用处参数数量不匹配（Task 3 修复）。

- [ ] **Step 6: Commit**

```bash
git add src/model/api_key.rs
git commit -m "feat: add bound_credential_ids field to ApiKey model"
```

---

### Task 2: admin/types.rs 和 admin/api_keys.rs 透传新字段

**Files:**
- Modify: `src/admin/types.rs`
- Modify: `src/admin/api_keys.rs`

- [ ] **Step 1: CreateApiKeyRequest 新增字段**

在 `src/admin/types.rs` 的 `CreateApiKeyRequest` 结构体末尾添加：

```rust
/// 绑定的凭据 ID 列表
#[serde(default)]
pub bound_credential_ids: Option<Vec<u64>>,
```

- [ ] **Step 2: UpdateApiKeyRequest 新增字段**

在 `UpdateApiKeyRequest` 结构体末尾添加：

```rust
/// 绑定的凭据 ID 列表（null 表示清除绑定）
#[serde(default)]
pub bound_credential_ids: Option<Option<Vec<u64>>>,
```

- [ ] **Step 3: api_keys.rs create_api_key handler 透传字段**

在 `src/admin/api_keys.rs` 的 `create_api_key` handler 中，修改 `manager.create(...)` 调用：

```rust
match manager.create(
    payload.name,
    payload.expires_at,
    payload.spending_limit,
    payload.duration_days,
    payload.bound_credential_ids,
) {
```

- [ ] **Step 4: api_keys.rs update_api_key handler 透传字段**

在 `update_api_key` handler 中，修改 `manager.update(...)` 调用，末尾增加参数：

```rust
match manager.update(
    id,
    payload.name,
    payload.enabled,
    payload.expires_at,
    payload.spending_limit,
    payload.duration_days,
    payload.bound_credential_ids,
) {
```

- [ ] **Step 5: 构建验证**

```bash
cd "/Users/MacBook/My Files/Code/sourcetree/kiro2cc-proxy"
cargo build 2>&1 | head -40
```

预期：编译错误减少，仍有 middleware/handler 相关错误（后续 Task 修复）。

- [ ] **Step 6: Commit**

```bash
git add src/admin/types.rs src/admin/api_keys.rs
git commit -m "feat: add bound_credential_ids to admin API request types"
```

---

### Task 3: middleware 将 bound_credential_ids 透传到 ApiKeyContext

**Files:**
- Modify: `src/anthropic/middleware.rs`

- [ ] **Step 1: ApiKeyContext 新增字段**

在 `src/anthropic/middleware.rs` 的 `ApiKeyContext` 结构体中添加：

```rust
/// 绑定的凭据 ID 列表，None 或空列表表示不限制
pub bound_credential_ids: Option<Vec<u64>>,
```

- [ ] **Step 2: 主密钥认证路径初始化新字段**

找到主密钥匹配后插入 `ApiKeyContext` 的代码，改为：

```rust
request.extensions_mut().insert(ApiKeyContext {
    id: 0,
    spending_limit: None,
    bound_credential_ids: None,
});
```

- [ ] **Step 3: 子 API Key 认证路径透传字段**

找到 `ApiKeyAuthResult::Valid` 分支中查出 `api_key` 后插入 context 的代码。

首先修改 `ApiKeyAuthResult::Valid` 携带的数据，在 `src/model/api_key.rs` 的 `authenticate` 方法中：

```rust
ApiKeyAuthResult::Valid {
    id: api_key.id,
    name: api_key.name.clone(),
    spending_limit: api_key.spending_limit,
    bound_credential_ids: api_key.bound_credential_ids.clone(),
}
```

然后修改 `ApiKeyAuthResult` 枚举的 `Valid` 变体（同文件）：

```rust
pub enum ApiKeyAuthResult {
    Valid {
        id: u32,
        name: String,
        spending_limit: Option<f64>,
        bound_credential_ids: Option<Vec<u64>>,
    },
    Disabled,
    Expired,
    NotFound,
}
```

- [ ] **Step 4: middleware 中解构新字段并写入 context**

在 `auth_middleware` 的 `ApiKeyAuthResult::Valid { id, name, spending_limit, bound_credential_ids }` 分支中，修改插入 context 的代码：

```rust
request.extensions_mut().insert(ApiKeyContext {
    id,
    spending_limit,
    bound_credential_ids,
});
```

- [ ] **Step 5: 构建验证**

```bash
cd "/Users/MacBook/My Files/Code/sourcetree/kiro2cc-proxy"
cargo build 2>&1 | head -40
```

- [ ] **Step 6: Commit**

```bash
git add src/anthropic/middleware.rs src/model/api_key.rs
git commit -m "feat: propagate bound_credential_ids through auth middleware"
```

---

### Task 4: MultiTokenManager 新增 acquire_context_filtered

**Files:**
- Modify: `src/kiro/token_manager.rs`

- [ ] **Step 1: 修改 select_next_credential 接受可选 ID 白名单**

将 `select_next_credential` 签名改为：

```rust
fn select_next_credential(&self, model: Option<&str>, allowed_ids: &[u64]) -> Option<(u64, KiroCredentials)>
```

在过滤可用凭据的 `.filter(|e| { ... })` 闭包中，在 `if e.disabled { return false; }` 之后添加：

```rust
// 凭据 ID 白名单过滤（空列表表示不限制）
if !allowed_ids.is_empty() && !allowed_ids.contains(&e.id) {
    return false;
}
```

- [ ] **Step 2: 更新 select_next_credential 的所有调用点**

在 `token_manager.rs` 中搜索所有 `self.select_next_credential(` 调用，全部改为传入空切片：

```rust
self.select_next_credential(model, &[])
```

（共约 2-3 处，均在 `acquire_context` 内部）

- [ ] **Step 3: 新增 acquire_context_filtered 方法**

在 `acquire_context` 方法之后添加：

```rust
/// 带凭据 ID 白名单的调用上下文获取
///
/// 与 acquire_context 逻辑相同，但只在 allowed_ids 指定的凭据中选择。
/// 白名单内所有凭据均不可用时直接返回错误，不回退到全局池。
pub async fn acquire_context_filtered(
    &self,
    model: Option<&str>,
    allowed_ids: &[u64],
) -> anyhow::Result<CallContext> {
    if allowed_ids.is_empty() {
        return self.acquire_context(model).await;
    }

    let total = allowed_ids.len();
    let mut tried_count = 0;

    loop {
        if tried_count >= total {
            anyhow::bail!(
                "绑定的凭据均不可用（共 {} 个）",
                total
            );
        }

        let (id, credentials) = {
            let is_balanced = self.load_balancing_mode.lock().as_str() == "balanced";

            let current_hit = if is_balanced {
                None
            } else {
                let entries = self.entries.lock();
                let current_id = *self.current_id.lock();
                entries
                    .iter()
                    .find(|e| e.id == current_id && !e.disabled && allowed_ids.contains(&e.id))
                    .map(|e| (e.id, e.credentials.clone()))
            };

            if let Some(hit) = current_hit {
                hit
            } else {
                match self.select_next_credential(model, allowed_ids) {
                    Some((new_id, new_creds)) => {
                        let mut current_id = self.current_id.lock();
                        *current_id = new_id;
                        (new_id, new_creds)
                    }
                    None => {
                        anyhow::bail!("绑定的凭据均已禁用（共 {} 个）", total);
                    }
                }
            }
        };

        match self.try_ensure_token(id, &credentials).await {
            Ok(ctx) => return Ok(ctx),
            Err(e) => {
                tracing::warn!("绑定凭据 #{} Token 刷新失败，尝试下一个: {}", id, e);
                self.switch_to_next_by_priority();
                tried_count += 1;
            }
        }
    }
}
```

- [ ] **Step 4: 构建验证**

```bash
cd "/Users/MacBook/My Files/Code/sourcetree/kiro2cc-proxy"
cargo build 2>&1 | head -40
```

预期：token_manager 相关编译通过，仍有 provider/handler 错误。

- [ ] **Step 5: Commit**

```bash
git add src/kiro/token_manager.rs
git commit -m "feat: add acquire_context_filtered to MultiTokenManager"
```

---

### Task 5: KiroProvider 透传 bound_ids 到 acquire_context

**Files:**
- Modify: `src/kiro/provider.rs`

- [ ] **Step 1: call_api_with_retry 增加 bound_ids 参数**

找到 `async fn call_api_with_retry(&self, request_body: &str, is_stream: bool)` 签名，改为：

```rust
async fn call_api_with_retry(
    &self,
    request_body: &str,
    is_stream: bool,
    bound_ids: &[u64],
) -> anyhow::Result<reqwest::Response>
```

在方法体内，将 `self.token_manager.acquire_context(model.as_deref()).await` 改为：

```rust
let ctx = match self.token_manager.acquire_context_filtered(model.as_deref(), bound_ids).await {
```

- [ ] **Step 2: call_mcp_with_retry 增加 bound_ids 参数**

找到 `async fn call_mcp_with_retry(&self, request_body: &str)` 签名，改为：

```rust
async fn call_mcp_with_retry(&self, request_body: &str, bound_ids: &[u64]) -> anyhow::Result<reqwest::Response>
```

在方法体内，将 `self.token_manager.acquire_context(None).await` 改为：

```rust
self.token_manager.acquire_context_filtered(None, bound_ids).await
```

- [ ] **Step 3: 更新公开方法签名**

```rust
pub async fn call_api(&self, request_body: &str, bound_ids: &[u64]) -> anyhow::Result<reqwest::Response> {
    self.call_api_with_retry(request_body, false, bound_ids).await
}

pub async fn call_api_stream(&self, request_body: &str, bound_ids: &[u64]) -> anyhow::Result<reqwest::Response> {
    self.call_api_with_retry(request_body, true, bound_ids).await
}

pub async fn call_mcp(&self, request_body: &str, bound_ids: &[u64]) -> anyhow::Result<reqwest::Response> {
    self.call_mcp_with_retry(request_body, bound_ids).await
}
```

- [ ] **Step 4: 构建验证**

```bash
cd "/Users/MacBook/My Files/Code/sourcetree/kiro2cc-proxy"
cargo build 2>&1 | head -40
```

预期：provider 编译通过，handler 调用处报错（参数数量不匹配）。

- [ ] **Step 5: Commit**

```bash
git add src/kiro/provider.rs
git commit -m "feat: add bound_ids parameter to KiroProvider call methods"
```

---

### Task 6: handlers 提取 bound_ids 并传入 provider

**Files:**
- Modify: `src/anthropic/handlers.rs`

- [ ] **Step 1: 在主 handler 中提取 bound_ids**

在 `src/anthropic/handlers.rs` 中，找到每个顶层 handler（`post_messages`、`post_messages_cc`）中提取 `api_key_id` 的代码：

```rust
let api_key_id = identity.as_ref().map(|ext| ext.0.id);
```

在其下方添加：

```rust
let bound_ids: Vec<u64> = identity
    .as_ref()
    .and_then(|ext| ext.0.bound_credential_ids.clone())
    .unwrap_or_default();
```

- [ ] **Step 2: 将 bound_ids 传入 handle_stream_request**

找到 `handle_stream_request(...)` 的调用，增加参数 `bound_ids.clone()`，并修改函数签名：

```rust
async fn handle_stream_request(
    provider: std::sync::Arc<crate::kiro::provider::KiroProvider>,
    request_body: &str,
    model: &str,
    input_tokens: i32,
    thinking_enabled: bool,
    usage_tracker: Option<std::sync::Arc<crate::model::usage::UsageTracker>>,
    api_key_id: Option<u32>,
    prompt_cache_usage: crate::cache::PromptCacheUsage,
    bound_ids: Vec<u64>,
) -> Response
```

在函数体内，将 `provider.call_api_stream(request_body).await` 改为：

```rust
provider.call_api_stream(request_body, &bound_ids).await
```

- [ ] **Step 3: 将 bound_ids 传入 handle_non_stream_request**

同上，修改 `handle_non_stream_request` 签名增加 `bound_ids: Vec<u64>`，将 `provider.call_api(request_body).await` 改为：

```rust
provider.call_api(request_body, &bound_ids).await
```

- [ ] **Step 4: 将 bound_ids 传入 handle_stream_request_buffered**

同上，修改 `handle_stream_request_buffered` 签名增加 `bound_ids: Vec<u64>`，将 `provider.call_api_stream(request_body).await` 改为：

```rust
provider.call_api_stream(request_body, &bound_ids).await
```

- [ ] **Step 5: 修复 websearch handler 中的 call_mcp 调用**

在 `src/anthropic/handlers.rs` 中找到 websearch 相关的 `call_mcp_api` 函数，修改其签名增加 `bound_ids: &[u64]`，并将 `provider.call_mcp(...)` 改为 `provider.call_mcp(request_body, bound_ids)`。

在调用 `call_mcp_api` 的地方传入 `&bound_ids`。

- [ ] **Step 6: 完整构建验证**

```bash
cd "/Users/MacBook/My Files/Code/sourcetree/kiro2cc-proxy"
cargo build 2>&1
```

预期：编译全部通过，无错误。

- [ ] **Step 7: Commit**

```bash
git add src/anthropic/handlers.rs
git commit -m "feat: extract and propagate bound_ids in anthropic handlers"
```

---

### Task 7: Admin UI — 类型和 API 层

**Files:**
- Modify: `admin-ui/src/types/api.ts`
- Modify: `admin-ui/src/api/credentials.ts`

- [ ] **Step 1: ApiKeyItem 新增字段**

在 `admin-ui/src/types/api.ts` 的 `ApiKeyItem` 接口末尾添加：

```ts
boundCredentialIds?: number[];
```

- [ ] **Step 2: CreateApiKeyRequest 新增字段**

```ts
export interface CreateApiKeyRequest {
  name: string
  expiresAt?: string | null
  spendingLimit?: number | null
  durationDays?: number | null
  boundCredentialIds?: number[] | null
}
```

- [ ] **Step 3: UpdateApiKeyRequest 新增字段**

```ts
export interface UpdateApiKeyRequest {
  name?: string
  enabled?: boolean
  expiresAt?: string | null
  spendingLimit?: number | null
  durationDays?: number | null
  boundCredentialIds?: number[] | null
}
```

- [ ] **Step 4: Commit**

```bash
cd "/Users/MacBook/My Files/Code/sourcetree/kiro2cc-proxy"
git add admin-ui/src/types/api.ts admin-ui/src/api/credentials.ts
git commit -m "feat: add boundCredentialIds to admin UI types and API"
```

---

### Task 8: Admin UI — 创建/编辑对话框新增凭据多选

**Files:**
- Modify: `admin-ui/src/components/api-keys-panel.tsx`

- [ ] **Step 1: 导入 useCredentials hook**

在 `api-keys-panel.tsx` 顶部的 import 行中，将现有的 hook import 行改为：

```ts
import { useApiKeys, useCreateApiKey, useUpdateApiKey, useDeleteApiKey, useServerInfo, useAllUsage, useResetKeyUsage, useRpm, useCredentials } from '@/hooks/use-credentials'
```

- [ ] **Step 2: 在组件顶部调用 useCredentials**

在组件函数体内，在其他 hook 调用之后添加：

```ts
const { data: credentials } = useCredentials()
```

- [ ] **Step 3: 新增创建对话框的绑定凭据状态**

在 `newSpendingLimit` state 之后添加：

```ts
const [newBoundCredentialIds, setNewBoundCredentialIds] = useState<number[]>([])
```

- [ ] **Step 4: 在创建对话框表单中添加多选控件**

在创建对话框 `<div className="space-y-4">` 内，在最后一个表单项之后、`</div>` 之前添加：

```tsx
{credentials && credentials.credentials && credentials.credentials.length > 0 && (
  <div>
    <label className="text-sm font-medium">绑定凭据</label>
    <p className="text-xs text-muted-foreground mt-0.5">不选则使用全局策略</p>
    <div className="mt-2 space-y-1 max-h-40 overflow-y-auto border rounded-md p-2">
      {credentials.credentials.map((cred) => (
        <label key={cred.id} className="flex items-center gap-2 cursor-pointer hover:bg-muted/50 rounded px-1 py-0.5">
          <input
            type="checkbox"
            checked={newBoundCredentialIds.includes(cred.id)}
            onChange={(e) => {
              if (e.target.checked) {
                setNewBoundCredentialIds((prev) => [...prev, cred.id])
              } else {
                setNewBoundCredentialIds((prev) => prev.filter((id) => id !== cred.id))
              }
            }}
            className="h-3.5 w-3.5"
          />
          <span className="text-sm">
            #{cred.id} {cred.email ? `· ${cred.email}` : ''}
            {cred.disabled && <span className="text-xs text-muted-foreground ml-1">（已禁用）</span>}
          </span>
        </label>
      ))}
    </div>
  </div>
)}
```

- [ ] **Step 5: handleCreate 传入 boundCredentialIds**

修改 `handleCreate` 中的 `createKey(...)` 调用，在对象末尾添加：

```ts
boundCredentialIds: newBoundCredentialIds.length > 0 ? newBoundCredentialIds : null,
```

在 `onSuccess` 回调中重置状态：

```ts
setNewBoundCredentialIds([])
```

- [ ] **Step 6: 新增编辑对话框的绑定凭据状态**

在 `editDurationUnit` state 之后添加：

```ts
const [editBoundCredentialIds, setEditBoundCredentialIds] = useState<number[]>([])
```

- [ ] **Step 7: 编辑对话框打开时初始化绑定状态**

找到设置 `editingKey` 的地方（通常是点击编辑按钮的 onClick），在设置其他 edit state 的同时添加：

```ts
setEditBoundCredentialIds(key.boundCredentialIds ?? [])
```

- [ ] **Step 8: 在编辑对话框中添加相同的多选控件**

在编辑对话框 `<div className="space-y-4">` 内末尾添加（与创建对话框相同结构，但使用 `editBoundCredentialIds` 和 `setEditBoundCredentialIds`）：

```tsx
{credentials && credentials.credentials && credentials.credentials.length > 0 && (
  <div>
    <label className="text-sm font-medium">绑定凭据</label>
    <p className="text-xs text-muted-foreground mt-0.5">不选则使用全局策略</p>
    <div className="mt-2 space-y-1 max-h-40 overflow-y-auto border rounded-md p-2">
      {credentials.credentials.map((cred) => (
        <label key={cred.id} className="flex items-center gap-2 cursor-pointer hover:bg-muted/50 rounded px-1 py-0.5">
          <input
            type="checkbox"
            checked={editBoundCredentialIds.includes(cred.id)}
            onChange={(e) => {
              if (e.target.checked) {
                setEditBoundCredentialIds((prev) => [...prev, cred.id])
              } else {
                setEditBoundCredentialIds((prev) => prev.filter((id) => id !== cred.id))
              }
            }}
            className="h-3.5 w-3.5"
          />
          <span className="text-sm">
            #{cred.id} {cred.email ? `· ${cred.email}` : ''}
            {cred.disabled && <span className="text-xs text-muted-foreground ml-1">（已禁用）</span>}
          </span>
        </label>
      ))}
    </div>
  </div>
)}
```

- [ ] **Step 9: handleUpdate 传入 boundCredentialIds**

在 `handleUpdate` 的 `data` 对象中添加：

```ts
data.boundCredentialIds = editBoundCredentialIds.length > 0 ? editBoundCredentialIds : null
```

- [ ] **Step 10: key 列表行展示绑定凭据 badge**

在 key 列表行中，找到展示到期时间等信息的区域，添加绑定凭据展示：

```tsx
{apiKey.boundCredentialIds && apiKey.boundCredentialIds.length > 0 && (
  <span className="text-xs text-muted-foreground">
    绑定: {apiKey.boundCredentialIds.map(id => `#${id}`).join(', ')}
  </span>
)}
```

- [ ] **Step 11: 构建前端验证**

```bash
cd "/Users/MacBook/My Files/Code/sourcetree/kiro2cc-proxy/admin-ui"
npm run build 2>&1 | tail -20
```

预期：构建成功，无 TypeScript 错误。

- [ ] **Step 12: Commit**

```bash
cd "/Users/MacBook/My Files/Code/sourcetree/kiro2cc-proxy"
git add admin-ui/src/components/api-keys-panel.tsx
git commit -m "feat: add credential binding UI to API key create/edit dialogs"
```

---

### Task 9: 完整构建 + 验证

- [ ] **Step 1: 完整构建（Rust + 前端）**

```bash
cd "/Users/MacBook/My Files/Code/sourcetree/kiro2cc-proxy"
./build-mac.sh 2>&1 | tail -20
```

预期：`构建成功！二进制位置: ./target/release/kiro2cc-proxy`

- [ ] **Step 2: 启动服务验证**

```bash
./run-local-service-mac.sh
```

打开 `http://127.0.0.1:5678/admin`，验证：
1. 创建 API Key 对话框中出现"绑定凭据"多选列表
2. 选择凭据后创建，key 列表行显示绑定信息
3. 编辑已绑定的 key，绑定状态正确回显
4. 清除绑定（取消所有勾选）后保存，key 恢复全局策略

- [ ] **Step 3: 最终 Commit**

```bash
git add -A
git commit -m "feat: complete API key credential binding feature"
```
