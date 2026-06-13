# 高并发性能优化 — 状态交接

## 分支

`feat/high-concurrency-optimization`（已推送到远程，commit `cddd46b`）

main 分支未受影响。

## 已完成的改动

所有代码改动已完成并通过 `cargo check`：

- **信号量作用域重构**（provider.rs）：permit 仅包裹 HTTP 发送阶段，失败/sleep 时释放，不再阻塞后续请求
- **并发上限可配置化**（provider.rs + config.rs）：默认 200（原 50），支持 `MAX_CONCURRENT_REQUESTS` 环境变量
- **429 冷却机制**（token_manager.rs + provider.rs + config.rs）：被限流凭据自动冷却 60s，避免反复撞同一个号。`COOLDOWN_SECS` 环境变量可调
- **Admin API 冷却可见性**（token_manager.rs）：`CredentialEntrySnapshot` 新增 `cooldownRemainingSecs` 字段
- **连接池调优**（http_client.rs + config.rs）：`pool_max_idle_per_host` 默认 100（原 20），新增 HTTP/2 keepalive。`POOL_MAX_IDLE_PER_HOST` 环境变量可调
- **Per-credential 刷新锁**（token_manager.rs）：替换全局单锁为 per-credential HashMap，多凭据可并行刷新 token
- **estimate_tokens 去堆分配**（stream.rs）：直接迭代 chars，不再 collect Vec
- **Tokio runtime 显式配置**（main.rs）：8 worker threads
- **CI workflow**（docker-build.yaml）：临时加了 `feat/high-concurrency-optimization` 分支触发构建

## 当前部署状态

- 服务器（DMIT VPS）已切换到 `:beta` 镜像运行中
- `docker-compose.yml` 中镜像 tag 从 `:latest` 改为 `:beta`
- 备份文件 `docker-compose.yml.bak` 存在，可一键回滚

## 观察结论（2026-03-18 14:24 ~ 14:27 日志）

- 新配置已生效（日志确认 `max_concurrent_requests=200, pool_max_idle_per_host=100`）
- 当前负载下（约 20-30 并发）未触发 429，冷却机制未被激活
- 大部分请求 latency 2-5s，属于上游 AWS 正常响应时间
- TTFB 与优化前差异不明显，因为瓶颈点（信号量排队、429 反复撞号）在当前负载下未触发
- 优化无副作用，需等高峰期（RPM 上升、出现 429）时再对比效果

## 下一步

1. 继续观察 beta 版运行，重点关注高峰期日志中是否出现「冷却凭据」字样
2. 如果确认稳定且高峰期有改善 → 合并到 main，打 tag 发正式版：
   ```bash
   git checkout main
   git merge feat/high-concurrency-optimization
   git tag v2026.3.x
   git push origin main --tags
   ```
3. 服务器切回 `:latest`：
   ```bash
   cp docker-compose.yml.bak docker-compose.yml
   docker compose pull && docker compose up -d
   ```
4. 如果需要回滚：
   ```bash
   cp docker-compose.yml.bak docker-compose.yml
   docker compose pull && docker compose up -d
   ```
5. 合并后记得删除 CI workflow 中临时加的分支触发行
