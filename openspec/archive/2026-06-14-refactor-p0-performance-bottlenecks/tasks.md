# 任务清单：refactor-p0-performance-bottlenecks

## 状态：ARCHIVED

## 任务
- [x] **任务 1: 重构 RPM 跟踪器 (Ring Buffer)** 
  - 将 `src/model/rpm.rs` 中的 `Vec<Instant>` 改写为 `[Bucket; 60]` 的固定大小的数组实现，达到 O(1) 的记录和清理性能。
- [x] **任务 2: 改造 UsageTracker 为异步 MPSC**
  - 在 `src/model/usage.rs` 中初始化 `tokio::sync::mpsc::unbounded_channel`，移除直接 `.save()`。
  - 建立专门的异步任务进行防抖和聚合写入，并处理通道关闭（Graceful Shutdown）时的最后一次落盘。
- [x] **任务 3: 改造 ThrottleLog 为异步 MPSC**
  - 在 `src/model/throttle_log.rs` 中实施相同的 MPSC 通道、防抖逻辑和优雅停机处理。
- [x] **任务 4: 改造 FailureLog 为异步 MPSC**
  - 在 `src/model/failure_log.rs` 中实施相同的 MPSC 通道、防抖逻辑和优雅停机处理。
- [x] **任务 5: 单元测试调整与验证**
  - 为 `RpmTracker` 提供单独的测试用例验证 60 秒边界表现。
  - 确保整个项目 `cargo test` 正常通过。

## 验收标准
- [ ] `src/model/rpm.rs` 中的 `Vec<Instant>` 已被移除，逻辑正确。
- [ ] `usage.rs`, `throttle_log.rs`, `failure_log.rs` 的 HTTP 处理链路不再包含同步 `fs::write` 操作。
- [ ] 后台任务正确处理了防抖（如只在间隔时间或积累一定量时才进行文件写入）。
- [ ] 后台任务支持通道关闭信号，在退出前能将所有内存缓冲的数据刷入磁盘。
- [ ] `cargo test` 所有测试顺利通过。