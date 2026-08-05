# 变更提案：server-side-ip-geolocation

## 背景

`admin-ui` 的三个日志详情页（`daily-detail-page.tsx`、`credential-detail-page.tsx`、`api-key-detail-page.tsx`）已经实现了 IP 归属地展示的前端逻辑：`use-ip-geo.ts` 中的 `useIpGeo` hook 会在浏览器里直接调用外部接口 `http://ip-api.com/batch` 解析归属地，并用 `https://api.ipify.org` 处理 localhost 场景。

`user-ui/src/hooks/use-ip-geo.ts` 与上述文件**字节级完全相同**，被 `user-ui/src/components/usage-log-page.tsx` 引用，存在与 admin-ui 完全一致的问题（已通过 `diff` 确认）。因此本次改动范围同时覆盖 admin-ui 与 user-ui 两侧的重复实现，而不仅是 admin-ui。

这个纯前端方案存在结构性缺陷：

1. **HTTP-only 导致静默失效**：`ip-api.com` 免费版仅支持 `http://`。若 admin-ui 部署在 HTTPS 域名下（生产环境常态），浏览器的混合内容（Mixed Content）策略会直接拦截该请求，`fetch` 进入 `catch` 分支，归属地永远解析不出来——这与用户反馈的截图现象（IP 后面没有国家·城市文本）完全一致。
2. **隐私外泄**：管理员浏览器会把日志里访问者的 IP 批量发送给第三方 `ip-api.com`。
3. **限频风险**：免费版限 45 次/分钟，日志分页一多容易触发限流。
4. **无持久缓存**：仅内存 `Map`，每次刷新页面都要重新请求。

## 目标范围

**在范围内：**
- 后端集成 `ip2region`（Rust binding，本地离线库）：新增 Cargo 依赖 + vendor 一份官方 `ip2region_v4.xdb` 数据文件（~10.6MB，从 [lionsoul2014/ip2region](https://github.com/lionsoul2014/ip2region) `data/ip2region_v4.xdb` 获取，Apache-2.0 许可）
- 新增两个轻量端点，共享同一个 `Arc<GeoResolver>` 实例：
  - `GET /api/admin/geo/batch`（挂载于 `AdminState`，走既有 admin 鉴权）
  - `GET /api/user/geo/batch`（挂载于 `UserState`，走既有 `x-api-key` 用户鉴权）
- 改造 `admin-ui/src/hooks/use-ip-geo.ts` **与** `user-ui/src/hooks/use-ip-geo.ts`（两者当前字节级相同）：内部实现分别改为调用各自后端新增的端点，移除对 `ip-api.com` / `api.ipify.org` 的外部依赖；对外的 `GeoInfo` 类型和 `useIpGeo(ips)` 调用签名保持不变，四个消费组件（admin-ui 三个详情页 + user-ui `usage-log-page.tsx`）**不需要改动**
- 更新 `openspec/project.md`「不引入新外部 crate」的开发约定，注明本次例外及理由

**不在范围内：**
- IPv6 归属地（当前日志观察到的 IP 均为 IPv4，暂不引入体积更大的 `ip2region_v6.xdb`）
- 修改日志持久化结构（`client_ip` 存储方式不变，归属地是查询时惰性解析，不落库、不做历史回填）
- 数据文件的自动更新机制（本次仅内嵌一份静态快照）
- 合并 `AppState`/`AdminState`/`UserState` 三个状态结构体或做任何与本次无关的架构调整（三者维持独立，仅各自新增一个指向同一 `GeoResolver` 的字段）

## 技术方案

- **数据文件嵌入**：`ip2region_v4.xdb` 提交到仓库（如 `assets/ip2region_v4.xdb`），通过 `include_bytes!` 编译期嵌入二进制。已通过反编译 crate 源码（`Searcher::new<P: AsRef<Path>>(file: P) -> Result<Self, Error>`，内部无条件 `std::fs::read(file)` 整个文件读入内存）确认：该 crate **只有一个文件路径参数**，不存在 `CachePolicy` 这个类型，也不支持从内存字节直接构造。因此启动时需将内嵌字节写出到运行时数据目录（与现有 `throttle_log.json`/`api_keys.json` 同级目录，取 `config_path` 的父目录），文件已存在则跳过写入；随后以该路径调用 `Searcher::new` 初始化（该调用本身就是把整份 xdb 读入内存，无需也无法额外指定缓存策略），之后每次查询是纯内存操作（官方基准 ~120ns/次）。
- **状态结构体集成**：`GeoResolver` 封装 `Searcher` 并提供 `resolve(ip: &str) -> Option<GeoInfo>`，对私有/环回地址（127.0.0.1、10.0.0.0/8、172.16.0.0/12、192.168.0.0/16、::1 等）直接短路返回 `None`，不查库。本仓库中 `AppState`（`src/anthropic/middleware.rs`）、`AdminState`（`src/admin/middleware.rs`）、`UserState`（`src/user/middleware.rs`）是三个彼此独立的状态结构体，分别服务于 `/v1`|`/cc/v1`、`/api/admin`、`/api/user` 三组路由；实际消费方是 admin-ui（走 `AdminState`）与 user-ui（走 `UserState`），因此**不**改动 `AppState`：
  - `AdminState` 新增 `geo_resolver: Option<Arc<GeoResolver>>` 字段 + `with_geo_resolver` builder 方法（沿用其现有的 `with_throttle_log_store` 等 Optional-builder 风格）
  - `UserState` 新增 `geo_resolver: Arc<GeoResolver>` 字段（该结构体当前无 builder，走既有的字面量构造风格，在 `src/main.rs` 构造 `user_state` 时直接赋值）
  - `main.rs` 中只构造**一个** `Arc<GeoResolver>` 实例，分别 clone 注入两处
- **返回值解析**：`Searcher::search()` 返回形如 `"中国|广东省|深圳市|电信|CN"` 的管道分隔字符串（顺序：国家|省份|城市|ISP|国家代码），按 `|` split 后取前 3 段映射为 `{country, region_name, city}`；ISP 与国家代码本次不透出（未被前端使用）。
- **新增端点（两个，共享同一 `GeoResolver`）**：
  - `GET /api/admin/geo/batch?ips=ip1,ip2,...`，风格对齐现有 `src/admin/router.rs` 中的路由注册方式，落在受鉴权保护的 `protected` 路由组下
  - `GET /api/user/geo/batch?ips=ip1,ip2,...`，对齐 `src/user/router.rs` 中 `protected`（`user_auth_middleware`）路由组的注册方式
  - 两者返回格式一致：`{ "ip1": {"country":"中国","regionName":"广东省","city":"深圳市"}, "ip2": null, ... }`
- **前端改造**：`admin-ui/src/hooks/use-ip-geo.ts` 的 `fetchGeoForIps` 改为 `fetch('/api/admin/geo/batch?ips=' + ...)`；`user-ui/src/hooks/use-ip-geo.ts` 改为通过其现有 `axios` 实例（`baseURL: '/api/user'`，已自动带上 `x-api-key`）请求 `geo/batch?ips=...`。两侧 `getPublicIp`/`api.ipify.org` 调用**整段移除**（不保留、不作为阶段 3 可选项）——若保留即会与"浏览器 Network 面板中不再出现对 `ip-api.com`/`api.ipify.org` 的请求"这条验收标准直接冲突。localhost/环回地址不再尝试解析出管理员的公网 IP，统一按后端 `resolve()` 对私有/环回地址返回 `None` 处理，前端展示为原始 IP、不带国家/城市后缀（与 spec.md「查询私有/环回地址」场景一致）。

## 预期影响

- 二进制体积增加约 10.6MB（xdb 数据文件内嵌）
- 首次启动多一次数据目录写入（写出 xdb 文件），此后无额外磁盘 I/O
- 归属地查询延迟从"浏览器发起外部请求（几十到几百毫秒，且可能被 Mixed Content 拦截）"降为"服务端内存查表（纳秒级）"
- 访问者 IP 不再离开服务器，消除隐私外泄面；不再依赖 `ip-api.com`/`ipify.org` 的公网可达性
- 四个现有消费组件（admin-ui 的 daily/credential/api-key-detail-page.tsx + user-ui 的 usage-log-page.tsx）渲染逻辑不变，用户可见的展示效果与当前"预期效果"一致，只是从"经常失效"变为"稳定生效"
- **行为变化（次要）**：因移除 `getPublicIp`/ipify.org，管理员/用户从 localhost 访问时不再显示"本机公网 IP 的归属地"这一猜测性展示，改为与其他私有地址一致的"无归属地"；这是当前隐藏 bug（该猜测本身也依赖 Mixed Content 会失效的外部请求）变为显式、一致行为，视为可接受的次要变化

## 风险

- **违反现有开发约定**：`openspec/project.md` 明确写着"不引入新外部 crate"，本次需要用户显式豁免（已在阶段 0 技术方案讨论中通过 AskUserQuestion 确认选择"方案 A: ip2region 本地库"），并同步更新 `project.md` 记录例外
- **数据时效性**：ip2region 数据是静态快照，运营商/地址段变化后归属地会有偏差；本次不做自动更新机制，记录为已知限制
- **Docker 部署可写性**：写出 xdb 文件需要运行时数据目录可写；当前 `throttle_log.json` 等文件已依赖同一目录可写，风险与现状一致，不新增额外假设
- **crate 成熟度**：`ip2region` crate 目前版本为 `0.1.0`，非 `lionsoul2014` 官方组织发布（官方仓库仅提供多语言 binding 源码，Rust binding 由社区维护），存在后续无人维护/API 变动的风险；因其只在启动时初始化一次、运行期只读查询，接口面很小，评估为可接受风险
- **传递依赖**：`ip2region` 间接引入 `speedy`/`speedy-derive`/`memoffset` 三个此前不存在于 `Cargo.lock` 的传递依赖（均为 `speedy` 序列化框架自身及其派生宏所需），均为成熟稳定的小型 crate，评估为可接受
