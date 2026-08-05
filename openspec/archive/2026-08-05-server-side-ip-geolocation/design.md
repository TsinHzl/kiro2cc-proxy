## 上下文

`admin-ui` 三个详情页与 `user-ui` 的 `usage-log-page.tsx` 都通过 `useIpGeo(ips)` hook 展示 IP 归属地，但当前实现（两侧 `use-ip-geo.ts` 字节级相同）在浏览器内直接调用 `http://ip-api.com/batch` + `https://api.ipify.org`。已通过反编译 `ip2region` crate（0.1.0）源码确认：`Searcher::new<P: AsRef<Path>>(file: P) -> Result<Self, Error>` 只接受一个文件系统路径参数（内部 `std::fs::read` 无条件把整份文件读入内存），不存在 `CachePolicy` 这个类型，也没有从 `&[u8]` 内存缓冲区构造的公开 API；官方数据文件 `data/ip2region_v4.xdb`（~10.6MB，Apache-2.0）可从 GitHub 直接下载，字段格式为官方 README 记载的 `Country|Province|City|ISP|iso-alpha2-code`（5 段）。

本仓库存在三个彼此独立、无共享字段的状态结构体：`AppState`（`src/anthropic/middleware.rs`，服务 `/v1`、`/cc/v1`）、`AdminState`（`src/admin/middleware.rs`，服务 `/api/admin`，字段含 `admin_api_key`/`service`/`api_key_manager`/`usage_tracker`/`rpm_tracker`/`throttle_log_store`/`failure_log_store`/`log_capture`/`config_path`）、`UserState`（`src/user/middleware.rs`，服务 `/api/user`，字段仅 `api_key_manager`/`usage_tracker`）。admin-ui 的三个详情页通过 `AdminState` 侧的鉴权访问，user-ui 的用量页通过 `UserState` 侧的 `x-api-key` 鉴权访问——两者都不经过 `AppState`。

## 目标 / 非目标

**目标：**
- 归属地解析全部移到服务端完成，前端 `GeoInfo` 类型与 `useIpGeo` 调用签名保持不变
- 查询延迟做到纳秒级（内存查表），不引入网络往返
- 不修改现有日志持久化结构

**非目标：**
- 不支持 IPv6 归属地
- 不做数据文件的自动更新/热加载
- 不改变三个详情页组件的 JSX 渲染逻辑

## 决策

1. **数据文件嵌入方式：`include_bytes!` + 启动时落盘，而非 `rust-embed`**
   项目已用 `rust-embed` 嵌入 `admin_ui`/`user_ui` 前端资源，但那些资源是"按需读取字节流直接响应 HTTP"，不需要落地为真实文件。`ip2region` 的 `Searcher::new` 要求真实文件路径，`rust-embed` 拿到的也只是内存字节，并不能省掉"写出到磁盘"这一步。因此直接用标准库 `include_bytes!` 嵌入更简单，不必再让 `rust-embed` 参与（避免为了复用一个宏而多包一层抽象）。`Searcher::new` 本身没有可选的缓存策略参数——它总是把整份文件读进内存，所以这里不存在"选择 FullMemory 还是 VectorIndex"这类决策，落盘后直接单参数调用即可。

2. **落盘位置：复用 `config_path` 同级目录，而非新建独立目录**
   项目里 `throttle_log.json`、`failure_log.json`、`api_keys.json`、`api_key_usage.json` 都写在 `Path::new(&config_path).parent()` 目录下（`src/main.rs`）。归属地 xdb 文件按同一约定写为 `<config_dir>/ip2region_v4.xdb`，复用已验证过的"该目录可写"假设，不引入新的路径配置项。

3. **归属地解析时机：查询时惰性解析，不落库**
   `client_ip` 已经持久化在 `usage.rs`/`throttle_log.rs`/`failure_log.rs` 里。归属地查询是纯内存操作（~120ns），没有必要在写入日志时同步解析并新增字段——那样要处理历史日志的 schema 迁移/兼容问题。改为在 admin API 返回时按需查表，历史日志自动获得归属地展示，零迁移成本。

4. **API 形态：两个批量 GET 端点，各自挂载在消费方所在的状态结构体上，而非塞进 `AppState`**
   对齐前端现状（`useIpGeo` 已经是按 `pageIps` 批量查询、内存级去重缓存）。由于 admin-ui 走 `AdminState` 鉴权、user-ui 走 `UserState` 鉴权，且这两个结构体互不相通，新增：
   - `GET /api/admin/geo/batch?ips=a,b,c` — 挂在 `src/admin/router.rs` 现有 `protected` 路由组下，复用既有 admin 鉴权中间件
   - `GET /api/user/geo/batch?ips=a,b,c` — 挂在 `src/user/router.rs` 现有 `protected`（`user_auth_middleware`）路由组下，复用既有用户鉴权中间件

   两个 handler 内部持有各自状态结构体里的 `Arc<GeoResolver>`，但两个 `Arc` 在 `main.rs` 中 clone 自**同一个** `GeoResolver` 实例（同一份内存索引，避免重复加载 xdb）。`AppState` 不新增任何字段——它服务的 `/v1`、`/cc/v1` 端点与"查看日志详情"这个场景无关，塞进去只会让状态结构体承担无关职责。

5. **私有/环回地址短路**
   `GeoResolver::resolve` 对 `127.0.0.1`、`::1`、`10.0.0.0/8`、`172.16.0.0/12`、`192.168.0.0/16` 等直接返回 `None`，不进入 `Searcher::search`（这类地址本身查不出有效归属地，也避免 ip2region 对非法输入报错）。

6. **彻底移除 `getPublicIp`/ipify.org，不作为阶段 3 可选项**
   现有 `use-ip-geo.ts` 对 localhost 访问会额外请求 `api.ipify.org` 猜测管理员的公网 IP 并展示其归属地。这个分支若保留，会与"浏览器 Network 面板中不再出现对 `ip-api.com`/`api.ipify.org` 的请求"这条验收标准直接冲突，且该猜测本身依赖的也是会被 Mixed Content 拦截的外部请求，价值有限。因此本次决策为整段移除，localhost/环回地址与其他私有地址一样统一返回 `None`、前端展示为不带归属地的原始 IP。

## 风险 / 权衡

- **新增外部 crate 违反 `project.md` 约定**：已通过 AskUserQuestion 向用户确认选择本方案，视为本次变更的显式豁免；`project.md` 将同步补充例外说明，而非静默违反。
- **二进制体积 +10.6MB**：相较于换取的"零网络依赖、零隐私外泄、纳秒级查询"，判断为可接受的权衡。
- **数据时效性**：静态快照会随时间产生偏差，不在本次范围内建立更新机制，作为已知技术债记录在 proposal.md 风险章节。
