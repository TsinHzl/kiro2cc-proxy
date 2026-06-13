# 变更提案：refactor-p0-performance-bottlenecks

## 背景
通过架构审查发现，当前代码库在用量统计落盘（UsageTracker 等）时使用了同步的磁盘 I/O (`fs::write`)，这会直接阻塞 Tokio 的异步运行时线程，导致并发吞吐量低下。此外，RPM 监控 (`RpmTracker`) 的 `Vec<Instant>` 会随着请求数无限制增长，因为缺乏主动驱逐机制，长期运行存在导致 OOM 的隐患。这被标记为 P0 级别的系统稳定性风险。

## 目标范围
**在范围内：**
- 将 `src/model/usage.rs`、`src/model/throttle_log.rs`、`src/model/failure_log.rs` 中的磁盘同步写入 (`.save()`) 改造为无阻塞的异步写（Write-Behind），解除 Tokio worker thread 阻塞。
- 将 `src/model/rpm.rs` 中的时间戳列表 (`Vec<Instant>`) 替换为基于滑动时间窗口的环形桶（Ring Buffer）实现，确保内存使用和操作时间复杂度稳定在 $O(1)$。

**不在范围内：**
- HTTP 客户端 (reqwest) 相关重构。
- 不影响 Admin/User 侧端点及页面的核心接口展示（仅内部实现重构）。
- 不涉及 Moka 缓存库的全面替代（这是阶段 1 的部分内容，因独立性在另一变更中进行，本次专注解决高危稳定性 P0 瓶颈）。

## 技术方案
- **异步事件驱动落盘：** 利用 `tokio::sync::mpsc::unbounded_channel` 提供无锁异步信道发送日志事件，避免容量限制导致的反压（基于内存监控，由于只是微量日志事件，无边界通道的内存风险可控）。启动一个后台守护任务（`tokio::spawn`）接收事件，结合 `tokio::time::interval` 执行批量防抖写入（例如每 5 秒落盘一次或积攒一定数量落盘）。
- **优雅停机 (Graceful Shutdown)：** 增加应用退出时的通道数据刷新机制，确保在接收到 `SIGTERM`/`SIGINT` 时，后台任务能完整消费通道中剩余的日志事件并执行最后一次 `fs::write`，防止正常停机时的数据丢失。
- **RPM Ring Buffer：** 实现固定大小的槽位数组（例如针对 60 秒的监控，使用 60 个 Bucket）。在 `record_request()` 时通过时间戳映射到槽位进行原子递增或覆写，替代旧的 `Vec::push` 和 `retain`。

## 预期影响
- **性能：** 大幅提升高并发处理能力，完全消除 `fs::write` 在 HTTP handler 生命周期中造成的长尾延迟。
- **稳定性：** 系统内存使用将稳定收敛，完全解决潜在的 OOM 漏洞。
- **兼容性：** 由于仅优化了底层写入与追踪的实现机制，所有向外暴露的接口将保持不变。

## 风险
- 若进程被异常强杀（如 `SIGKILL` 且未能进行优雅退出），可能存在后台 Channel 缓冲中最多 5 秒钟的统计数据丢失。
- 使用 `unbounded_channel` 理论上存在 OOM 风险，但因为这是落盘事件且防抖消费很快，正常情况下内存积压极低。此权衡优先保证了核心业务（API 请求）不会因为写盘反压而超时阻塞。