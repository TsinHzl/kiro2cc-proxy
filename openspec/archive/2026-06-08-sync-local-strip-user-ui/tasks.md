# 任务清单：sync-local-strip-user-ui

## 状态：ARCHIVED

## 任务
- [x] 1. rsync `src/` 全量同步到 local（排除 user/、user_ui/），含新增 throttle_log.rs、metering.rs、code_reference.rs
- [x] 2. 适配 `main.rs`：移除 mod user / mod user_ui 声明、路由注册代码、apply_env_overrides 调用
- [x] 3. 适配 `config.rs`：移除 apply_env_overrides 方法和 use std::env
- [x] 4. 适配 `model/mod.rs`：确认 rsync 后已包含 `pub mod throttle_log;` 声明，若缺失则手动添加
- [x] 5. 校验 Cargo.toml `[dependencies]`：diff 两仓 deps 段，补齐差异后更新版本为 2026.2.10
- [x] 6. rsync `admin-ui/`（src + dist + public + 配置文件）
- [x] 7. 删除 local 的 `user-ui/` 目录
- [x] 8. 同步构建脚本（build-mac.sh、build-windows.ps1）并移除 user-ui 步骤、修正二进制名为 kiro-rs
- [x] 9. 同步 tools/、config.example.json、Cargo.lock
- [x] 10. `cargo check` 验证编译通过
- [x] 11. git commit + tag v2026.2.10 + push

## 验收标准
- [ ] local 仓库 `cargo check` 无错误
- [ ] local 仓库无 user-ui 相关代码和目录（`find . -path '*/user*' | grep -v target` 无结果）
- [ ] local 仓库 main.rs 不包含 mod user / mod user_ui 声明
- [ ] local 仓库 config.rs 不包含 apply_env_overrides
- [ ] admin-ui/dist/index.html 存在且非空
- [ ] git tag v2026.2.10 已创建并 push 到远端
