## 新增需求

### 需求：批量 IP 归属地查询接口

Admin API 与 User API 各提供一个受鉴权保护的端点，输入一批 IP，返回每个 IP 的归属地信息（国家/省份/城市），分别供 admin-ui 日志详情页与 user-ui 用量日志页展示。两个端点共享同一份服务端 ip2region 离线索引，行为完全一致，仅鉴权方式和挂载路径不同。

#### 场景：查询公网 IP 的归属地（Admin）

- **WHEN** 管理员已鉴权，请求 `GET /api/admin/geo/batch?ips=222.35.11.141`
- **THEN** 响应包含 `"222.35.11.141"` 对应的对象，字段为 `country`（国家）、`regionName`（省份）、`city`（城市），值来自本地 ip2region 库的离线查询结果

#### 场景：查询公网 IP 的归属地（User）

- **WHEN** 用户已通过 `x-api-key` 鉴权，请求 `GET /api/user/geo/batch?ips=222.35.11.141`
- **THEN** 响应格式与结果与 Admin 端点一致

#### 场景：查询私有/环回地址

- **WHEN** 请求的 `ips` 参数中包含 `127.0.0.1`、`10.x.x.x`、`172.16.x.x`、`192.168.x.x` 或 `::1`
- **THEN** 该 IP 对应的值为 `null`，不进入 ip2region 查询（这类地址本身没有有效归属地）

#### 场景：查询格式非法的 IP

- **WHEN** `ips` 参数中的某一项不是合法的 IPv4/IPv6 字符串
- **THEN** 该项对应的值为 `null`，不影响批次中其他合法 IP 的正常返回（不整体报错）

#### 场景：批量查询包含重复 IP

- **WHEN** `ips` 参数中同一个 IP 出现多次
- **THEN** 响应中该 IP 只出现一次，对应一次查询结果（去重）

#### 场景：未鉴权访问

- **WHEN** 请求缺少有效的 Admin 鉴权凭据（访问 `/api/admin/geo/batch`）或有效的 `x-api-key`（访问 `/api/user/geo/batch`）
- **THEN** 分别返回现有 admin / user 鉴权中间件统一的 401/403 响应，不执行归属地查询

## 修改的行为

### 需求：`useIpGeo` 前端 hook 的数据来源

`admin-ui` 与 `user-ui` 中 `useIpGeo(ips)` 的对外类型（`GeoInfo { country, regionName, city, displayIp? }`）与调用方式保持不变，但内部实现分别改为请求各自后端新增的端点（`/api/admin/geo/batch` 或 `/api/user/geo/batch`），不再请求 `http://ip-api.com` 与 `https://api.ipify.org`。

#### 场景：管理员在 HTTPS 部署下查看日志详情

- **WHEN** admin-ui 部署在 HTTPS 域名下，管理员打开任意一个日志详情页（daily/credential/api-key）
- **THEN** IP 归属地正常显示（此前因 Mixed Content 被浏览器拦截而始终为空的问题被消除）

#### 场景：普通用户在 HTTPS 部署下查看自己的用量日志

- **WHEN** user-ui 部署在 HTTPS 域名下，用户打开用量日志页（`usage-log-page.tsx`）
- **THEN** IP 归属地正常显示，问题根因与效果与 Admin 侧一致
