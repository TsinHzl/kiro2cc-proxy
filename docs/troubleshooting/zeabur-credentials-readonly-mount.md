# Zeabur 凭据文件写入失败：Config File 只读挂载覆盖持久化卷

日期：2026-02-25

## 症状

- 通过 Admin UI 添加凭据后，内存中正常显示（API 返回正确）
- 刷新页面后凭据仍在（进程未重启时）
- 但 Zeabur File Management 中 `credentials.json` 始终只有最初的 1 个凭据
- 容器重启后新添加的凭据全部丢失
- 应用日志中没有任何写入错误
- 本地开发环境完全正常，问题仅在 Zeabur 部署时出现

## 根因

Zeabur 的 "Config File" 功能会将配置文件以 **只读 bind mount** 的方式挂载到容器内，覆盖持久化卷上的同名文件。

在容器内执行 `mount | grep config` 可以看到：

```
10.0.0.5:/... on /app/config type nfs (rw,...)          ← 持久化卷，可读写
/dev/vdb on /app/config/config.json type ext4 (ro,...)   ← Config File，只读
/dev/vdb on /app/config/credentials.json type ext4 (ro,...) ← Config File，只读！
```

三层挂载关系：

1. `/app/config` — NFS 持久化卷（rw），应用可以正常读写此目录下的文件
2. `/app/config/config.json` — Zeabur Config File（ro），覆盖了卷上的同名文件
3. `/app/config/credentials.json` — Zeabur Config File（ro），覆盖了卷上的同名文件

应用调用 `File::create("/app/config/credentials.json")` 时，实际操作的是 ro bind mount，写入静默失败或写到了被遮蔽的层，数据无法持久化。

## 为什么难以发现

- `std::fs::write` / `File::create` 在某些 overlay 场景下不一定返回错误
- 即使加了 `fsync` 也无法解决（问题不在刷盘，而在挂载层）
- 本地开发环境没有这种多层挂载，无法复现
- Zeabur 的 File Management 显示的是 bind mount 的内容（只读层），看起来"文件存在且有内容"，容易误导
- 应用内存中的数据是正确的，API 返回正常，表面上一切正常

## 解决方案

在 Zeabur Dashboard 中删除 `credentials.json` 的 Config File 配置，只保留持久化卷。

- `config.json` 保留为 Config File（只读即可，应用只读取不写入）
- `credentials.json` 不要设为 Config File（应用需要运行时写入）

Dockerfile CMD 中已有初始化逻辑，会在文件不存在时自动生成空数组：

```sh
if [ ! -f /app/config/credentials.json ]; then
  echo "[]" > /app/config/credentials.json;
fi
```

删除 Config File 后首次启动会丢失旧凭据（存在 ro mount 中），需要重新添加。

## 排查方法论（Zeabur 部署问题通用流程）

### 1. 使用 Zeabur Command 终端进入容器

Zeabur Dashboard → 服务 → Command 标签页，可以在运行中的容器内执行命令（alpine 镜像用 sh，没有 bash）。

### 2. 关键诊断命令

```sh
# 查看挂载情况 — 最重要的一步，能发现隐藏的 bind mount
mount | grep <目录名>

# 查看目录权限和文件时间戳
ls -la /app/config/

# 测试写入能力
echo "test" > /app/config/test.txt && cat /app/config/test.txt

# 查看文件实际内容
cat /app/config/credentials.json

# 查看进程信息
ps aux
```

### 3. 排查思路

当"写入成功但数据不持久化"时，按以下顺序排查：

1. **检查挂载** — `mount | grep` 看是否有意外的 bind mount 覆盖目标文件
2. **检查权限** — `ls -la` 看文件和目录的读写权限
3. **测试写入** — 手动 echo 写入同目录下的新文件，确认卷本身可写
4. **检查 fsync** — 容器环境下 `std::fs::write` 不调用 fsync，数据可能在页缓存中丢失
5. **检查日志级别** — 确认应用的日志级别能输出 info/warn/error

### 4. Zeabur 特有的坑

- Config File 会以 ro bind mount 覆盖持久化卷上的同名文件
- File Management 显示的可能是 bind mount 层的内容，不是卷的真实内容
- 需要运行时写入的文件，绝对不能设为 Config File
