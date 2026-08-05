# 任务清单：server-side-ip-geolocation

## 状态：ARCHIVED

## 任务

- [x] 下载官方 `ip2region_v4.xdb`（`https://raw.githubusercontent.com/lionsoul2014/ip2region/master/data/ip2region_v4.xdb`），提交到仓库 `assets/ip2region_v4.xdb`；`Cargo.toml` 新增 `ip2region` 依赖
- [x] 新增 `src/model/geo.rs`：`GeoResolver` 结构体，内部持有 `ip2region::Searcher`；`new()` 负责判断 `<config_dir>/ip2region_v4.xdb` 是否已存在，不存在则用 `include_bytes!` 内嵌字节写出，再调用 `Searcher::new(path)`（单参数，无缓存策略可选）初始化；提供 `resolve(&self, ip: &str) -> Option<GeoInfo>`，对私有/环回地址短路返回 `None`，其余调用 `Searcher::search` 并按 `|` 分段解析出 `{country, region_name, city}`
- [x] `src/admin/middleware.rs`：`AdminState` 新增 `geo_resolver: Option<Arc<GeoResolver>>` 字段 + `with_geo_resolver` builder 方法（对齐既有 `with_throttle_log_store` 等风格）
- [x] `src/user/middleware.rs`：`UserState` 新增 `geo_resolver: Arc<GeoResolver>` 字段（该结构体无 builder，走字面量构造）
- [x] `src/main.rs`：启动流程中构造**一个** `Arc<GeoResolver>`（复用现有 `config_path` 解析出的数据目录），分别 clone 注入 `admin_state`（经 `with_geo_resolver`）与 `user_state`（构造字面量新增该字段）
- [x] `src/admin/`（`types.rs`/`service.rs`/`router.rs` 视现有分层新增对应函数）：新增 `GET /api/admin/geo/batch?ips=a,b,c` handler，解析 `ips` 查询参数、去重、逐个调用 `GeoResolver::resolve`，组装 `HashMap<String, Option<GeoInfo>>` JSON 响应；挂载到 `router.rs` 现有 `protected` 路由组
- [x] `src/user/`（`handlers.rs`/`router.rs`）：新增 `GET /api/user/geo/batch?ips=a,b,c` handler，逻辑与上一条一致；挂载到 `router.rs` 现有 `protected`（`user_auth_middleware`）路由组
- [x] `admin-ui/src/hooks/use-ip-geo.ts`：`fetchGeoForIps` 改为请求本项目 `/api/admin/geo/batch`；**整段移除** `getPublicIp`/`api.ipify.org` 相关代码（不保留），localhost 统一按后端返回的 `null` 展示为无归属地；对外 `GeoInfo` 类型与 `useIpGeo(ips)` 签名保持不变
- [x] `user-ui/src/hooks/use-ip-geo.ts`：`fetchGeoForIps` 改为通过其现有 `axios` 实例请求 `geo/batch`（`baseURL: '/api/user'`，自动带 `x-api-key`）；同样**整段移除** `getPublicIp`/`api.ipify.org` 相关代码，对外签名保持不变
- [x] `openspec/project.md`「开发约定」章节补充「例外：为 IP 归属地功能引入 `ip2region` 本地库，理由见 `openspec/archive/.../server-side-ip-geolocation`」
- [x] `cargo fmt` + `cargo clippy` clean；新增单元测试覆盖 `GeoResolver::resolve` 的三种场景（公网 IP / 私有地址 / 非法格式）

## 验收标准

- [x] `cargo build --release` 成功，二进制体积增量在预期范围内（约 +10.6MB）
- [x] 启动服务后，`GET /api/admin/geo/batch?ips=222.35.11.141`（带合法 admin 鉴权）返回非空的 `country`/`regionName`/`city`（curl 实测：`{"country":"中国","regionName":"北京","city":"北京市"}`）
- [x] `GET /api/user/geo/batch?ips=222.35.11.141`（带合法用户 `x-api-key`）返回同样非空的结果
- [x] `GET /api/admin/geo/batch?ips=127.0.0.1` 与 `GET /api/user/geo/batch?ips=127.0.0.1` 均返回该 IP 对应 `null`
- [x] 未带鉴权访问以上两个端点均返回 401/403
- [x] 三个 admin-ui 详情页（daily/credential/api-key-detail-page.tsx）与 user-ui 的 `usage-log-page.tsx` 在浏览器中打开时正确调用本地 `/api/{admin,user}/geo/batch`，浏览器 Network 面板中确认不再出现对 `ip-api.com`/`api.ipify.org` 的请求（Playwright 实测确认）。**已知限制**：受限于本地环境仅有 `127.0.0.1` 私有地址日志，未能在浏览器中实际观察公网 IP 的"国家·城市"渲染效果（后端已通过 curl 单独验证正确）；且城市字段缺失颗粒度时前端会显示悬空的"国家·"分隔符（范围外前端组件未做条件渲染，已记录为已知问题）
- [x] `cargo clippy` 无警告（本次改动文件零警告；仓库中 `token_manager.rs`/`token.rs` 的 63 条历史遗留警告与本次改动无关，不在本次范围内）
