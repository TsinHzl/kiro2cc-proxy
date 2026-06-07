# 变更提案：sync-local-strip-user-ui

## 背景
kiro2cc-proxy 主仓库包含完整功能（本地部署 + 服务器部署 + 用户面板），
kiro2cc-proxy-local 仓库是面向个人本地使用的精简版本（包名 `kiro-rs`，日期版本格式 `2026.x.y`）。
主仓在 v2.2.19 ~ v2.3.0 期间积累了大量改动（协议解析优化、限流日志、IDC 认证修复、admin-ui 增强等），
local 仓库已严重落后，需要全量同步并剥离服务器/用户面板功能。

## 目标范围
**在范围内：**
- 全量同步 `src/` 核心代码到 local 仓库（anthropic、kiro、model、admin、admin_ui、common、cache、http_client、token 等）
- 新增 `throttle_log.rs`、`metering.rs`、`code_reference.rs` 等主仓新文件
- 同步 `admin-ui/` 前端源码和构建产物
- 同步构建脚本（build-mac.sh、build-windows.ps1）并移除 user-ui 构建步骤
- 同步 tools/、config.example.json
- 适配 `main.rs`：移除 user/user_ui 模块引用
- 适配 `config.rs`：移除 `apply_env_overrides()`（容器部署专用）
- 删除 local 仓库中的 `user-ui/` 目录
- 更新 Cargo.toml 版本号为 `2026.2.10`
- 验证 `cargo check` 通过
- 提交、打 tag `v2026.2.10`、push

**不在范围内：**
- 不修改主仓库（kiro2cc-proxy）的任何代码
- 不同步 Docker/服务器部署文件（Dockerfile、docker-compose.yml、fly.toml、systemd service、install_server.sh 等）
- 不同步 registration-service/
- 不同步 .github/workflows/
- 不同步 openspec/ 目录到 local
- 不改变 local 仓库的包名（保持 `kiro-rs`）和版本格式（保持 `2026.x.y`）

## 技术方案
1. **源码同步**：rsync `src/` 全量覆盖（排除 `user/`、`user_ui/`），包含新增的 `throttle_log.rs`、`metering.rs`、`code_reference.rs`
2. **入口适配**：`main.rs` 移除 `mod user` / `mod user_ui` 声明及相关路由注册代码，移除 `apply_env_overrides()` 调用
3. **配置适配**：`config.rs` 移除 `use std::env` 和整个 `apply_env_overrides` 方法
4. **模块声明适配**：`model/mod.rs` 新增 `pub mod throttle_log;` 声明（rsync 会带入文件，但需确认 mod.rs 中的声明已包含）
5. **Cargo.toml 依赖校验**：对比主仓与 local 的 `[dependencies]`，若有差异则同步（当前两仓 deps 一致，仅需更新版本号；但需验证无遗漏）
6. **前端同步**：rsync `admin-ui/src/` + `admin-ui/dist/` + `admin-ui/public/` + 配置文件
7. **构建脚本**：移除 user-ui 构建步骤，修正二进制名引用为 `kiro-rs`
8. **验证**：`cargo check` 确保编译通过
9. **发布**：git add → commit → tag v2026.2.10 → push

## 预期影响
- local 仓库将获得主仓 v2.2.19 ~ v2.3.0 的全部核心功能改进
- admin-ui 将获得限流日志页面等新功能
- 无 user-ui 功能，admin 面板保留完整

## 风险
| 风险 | 影响 | 缓解 |
|------|------|------|
| src 全量覆盖后编译失败 | 阻塞发布 | cargo check 验证通过后才 commit |
| admin-ui dist 与后端不匹配 | 前端白屏 | 同步 dist 来自同一版本的 build 产物 |
| Cargo.lock 版本冲突 | 编译失败 | 从主仓复制 Cargo.lock 保持依赖一致 |
| Cargo.toml 依赖差异 | unresolved import | 同步前 diff 两仓 deps 段，补齐差异 |
