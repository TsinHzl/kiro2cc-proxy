## 新增需求

### 需求：模型费率倍率展示

`GET /api/admin/models` 响应的每个模型条目须包含官方实时费率倍率（`rate_multiplier`），来源于 Kiro `ListAvailableModels` 接口的实时调用，不使用缓存。

#### 场景：上游调用成功且模型可映射

- **WHEN** 至少存在一个可用账号，且 Kiro `ListAvailableModels` 调用成功返回，且某内部模型 id 经 `map_model()` 归一化后的目标 id 出现在本次返回的模型目录中
- **THEN** 该模型条目的 `rate_multiplier` 字段为对应的数值（如 `1.3`）

#### 场景：无可用账号或上游调用失败

- **WHEN** 当前没有可用账号，或调用 Kiro `ListAvailableModels` 失败（网络错误、非 2xx 响应）
- **THEN** `GET /api/admin/models` 仍返回 `200`，模型列表完整（与不含费率信息时一致），所有条目的 `rate_multiplier` 均为 `null`

#### 场景：归一化后未命中当前账号的模型目录

- **WHEN** 上游调用成功，但某内部模型 id 经 `map_model()` 归一化后的目标 id 不在本次返回的模型目录中
- **THEN** 该模型条目的 `rate_multiplier` 为 `null`
