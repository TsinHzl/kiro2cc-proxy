# No-Docker Startup Scripts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Docker-based VPS deployment with shell-script-based startup (adapted from KiroProxy2cc4mac), while keeping the kiro-rs Rust binary as the reverse proxy core, and supporting both macOS local and Linux server deployment.

**Architecture:** Four new files are added to the repo root — an enhanced `start.command` for macOS double-click launch, a `start_server.sh` for Linux with start/stop/restart/status subcommands and PID-file management, a `kiro-rs.service` systemd unit for server auto-start, and an `install_server.sh` one-shot setup script. No changes to any Rust source files.

**Tech Stack:** Bash, systemd (Linux server), macOS `.command` double-click convention, `lsof`/`kill` for port management, `cargo build --release` for compilation.

---

### Task 1: Enhanced macOS `start.command`

**Files:**
- Modify: `start.command`

This replaces the existing minimal `start.command` with a robust version that mirrors KiroProxy2cc4mac's macOS launcher: config wizard on first run, port-conflict resolution, binary existence check, and auto-open browser.

- [ ] **Step 1: Review current `start.command`**

Read the file to confirm current content (already done in analysis — it runs `./target/release/kiro-rs` directly with a basic config check).

- [ ] **Step 2: Write the new `start.command`**

Replace the entire file with:

```bash
#!/bin/bash
# kiro-rs macOS 本地启动脚本
# 双击即可启动，无需 Docker

cd "$(dirname "$0")"

# ============================================================
# 可选：本地覆盖配置（取消注释并修改）
# ============================================================
# export API_KEY=sk-kiro-rs-your-key
# export ADMIN_API_KEY=sk-admin-your-key
# export PORT=8990
# export HOST=127.0.0.1
# export REGION=us-east-1
# export PROXY_URL=http://127.0.0.1:7890
# ============================================================

CONFIG_FILE="$(pwd)/config.json"
BINARY="$(pwd)/target/release/kiro-rs"

echo "=================================================="
echo "  kiro-rs 启动脚本"
echo "=================================================="

# ── 检查二进制是否存在 ──────────────────────────────────
if [ ! -f "$BINARY" ]; then
    echo "[!] 未找到编译好的二进制: $BINARY"
    echo "[*] 正在编译（首次需要几分钟）..."
    if ! command -v cargo &>/dev/null; then
        echo "[!] 未找到 cargo，请先安装 Rust: https://rustup.rs"
        read -p "按回车退出..."
        exit 1
    fi
    cargo build --release
    if [ $? -ne 0 ]; then
        echo "[!] 编译失败"
        read -p "按回车退出..."
        exit 1
    fi
    echo "[*] 编译完成 ✓"
fi

# ── 配置向导（首次运行） ────────────────────────────────
setup_config() {
    echo ""
    echo "未找到 config.json，需要先完成初始配置。"
    echo ""

    while [ -z "$API_KEY_INPUT" ]; do
        read -p "  API Key（访问此代理的密钥，自定义即可）: " API_KEY_INPUT
    done

    read -p "  Admin API Key（管理后台密码，直接回车跳过）: " ADMIN_KEY_INPUT

    read -p "  端口 [默认: 8990]: " input_port
    PORT_INPUT="${input_port:-8990}"

    read -p "  Region [默认: us-east-1]: " input_region
    REGION_INPUT="${input_region:-us-east-1}"

    ADMIN_BLOCK=""
    if [ -n "$ADMIN_KEY_INPUT" ]; then
        ADMIN_BLOCK=",
  \"adminApiKey\": \"$ADMIN_KEY_INPUT\""
    fi

    cat > "$CONFIG_FILE" <<EOF
{
  "host": "127.0.0.1",
  "port": $PORT_INPUT,
  "apiKey": "$API_KEY_INPUT",
  "tlsBackend": "rustls",
  "region": "$REGION_INPUT"$ADMIN_BLOCK
}
EOF
    echo ""
    echo "config.json 已生成 ✓"
}

if [ ! -f "$CONFIG_FILE" ]; then
    setup_config
elif ! grep -q '"apiKey"' "$CONFIG_FILE" 2>/dev/null; then
    echo "[!] config.json 中缺少 apiKey，请编辑: $CONFIG_FILE"
    open "$CONFIG_FILE"
    read -p "编辑完成后按回车继续..."
fi

# ── 读取端口并杀掉占用进程 ──────────────────────────────
CONFIGURED_PORT=$(python3 -c "import json; c=json.load(open('$CONFIG_FILE')); print(c.get('port',8990))" 2>/dev/null || echo "8990")
OLD_PID=$(lsof -ti tcp:"$CONFIGURED_PORT" 2>/dev/null)
if [ -n "$OLD_PID" ]; then
    echo "[*] 端口 $CONFIGURED_PORT 被 PID $OLD_PID 占用，正在终止..."
    kill -9 $OLD_PID 2>/dev/null
    sleep 1
fi

echo "[*] 启动 kiro-rs，端口: $CONFIGURED_PORT"
echo "[*] API 端点: http://127.0.0.1:${CONFIGURED_PORT}/v1/messages"
if grep -q '"adminApiKey"' "$CONFIG_FILE" 2>/dev/null; then
    echo "[*] 管理面板: http://127.0.0.1:${CONFIGURED_PORT}/admin"
fi
echo "=================================================="
echo ""

# 延迟 2 秒后自动打开管理面板（如果有 adminApiKey）
if grep -q '"adminApiKey"' "$CONFIG_FILE" 2>/dev/null; then
    (sleep 2 && open "http://127.0.0.1:${CONFIGURED_PORT}/admin") &
fi

# 前台运行，关闭终端窗口即停止
exec "$BINARY"
```

- [ ] **Step 3: Make executable**

```bash
chmod +x start.command
```

- [ ] **Step 4: Smoke test (manual)**

Double-click `start.command` in Finder, or run in terminal:
```bash
bash start.command
```
Expected: config wizard appears if `config.json` missing; otherwise service starts and prints port info.

- [ ] **Step 5: Commit**

```bash
git add start.command
git commit -m "feat: enhance start.command with config wizard and port-conflict handling"
```

---

### Task 2: Linux server `start_server.sh`

**Files:**
- Create: `start_server.sh`

Mirrors KiroProxy2cc4mac's `start_server.sh` but targets the kiro-rs binary. Supports `start`, `stop`, `restart`, `status` subcommands with PID-file management and `nohup` background execution.

- [ ] **Step 1: Create `start_server.sh`**

```bash
#!/bin/bash
# kiro-rs Linux 服务器启动脚本
# 支持 start / stop / restart / status 子命令
# 所有配置均可通过环境变量或 config.json 覆盖

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BINARY="$SCRIPT_DIR/target/release/kiro-rs"
PID_FILE="$SCRIPT_DIR/kiro-rs.pid"
LOG_DIR="${KIRO_LOG_DIR:-$SCRIPT_DIR/logs}"
LOG_FILE="$LOG_DIR/kiro-rs.log"

# ── 检查二进制 ──────────────────────────────────────────
check_binary() {
    if [ ! -f "$BINARY" ]; then
        echo "[!] 未找到二进制: $BINARY"
        echo "[*] 请先编译: cargo build --release"
        exit 1
    fi
}

do_start() {
    check_binary

    if [ -f "$PID_FILE" ]; then
        OLD_PID=$(cat "$PID_FILE")
        if kill -0 "$OLD_PID" 2>/dev/null; then
            echo "[!] kiro-rs 已在运行 (PID $OLD_PID)"
            exit 1
        fi
        rm -f "$PID_FILE"
    fi

    mkdir -p "$LOG_DIR"

    echo "[*] 启动 kiro-rs，日志: $LOG_FILE"
    cd "$SCRIPT_DIR"
    nohup "$BINARY" >> "$LOG_FILE" 2>&1 &
    echo $! > "$PID_FILE"
    sleep 1

    if kill -0 "$(cat "$PID_FILE")" 2>/dev/null; then
        echo "[*] 已启动，PID $(cat "$PID_FILE")"
        # 读取配置中的端口用于提示
        PORT=$(python3 -c "import json; c=json.load(open('$SCRIPT_DIR/config.json')); print(c.get('port',8990))" 2>/dev/null || echo "8990")
        echo "[*] API 端点: http://localhost:${PORT}/v1/messages"
    else
        echo "[!] 启动失败，请检查日志: $LOG_FILE"
        rm -f "$PID_FILE"
        exit 1
    fi
}

do_stop() {
    if [ ! -f "$PID_FILE" ]; then
        echo "[!] PID 文件不存在，服务可能未运行"
        return
    fi
    PID=$(cat "$PID_FILE")
    if kill -0 "$PID" 2>/dev/null; then
        kill "$PID"
        rm -f "$PID_FILE"
        echo "[*] 已停止 (PID $PID)"
    else
        echo "[!] 进程 $PID 不存在，清理 PID 文件"
        rm -f "$PID_FILE"
    fi
}

do_status() {
    if [ -f "$PID_FILE" ]; then
        PID=$(cat "$PID_FILE")
        if kill -0 "$PID" 2>/dev/null; then
            PORT=$(python3 -c "import json; c=json.load(open('$SCRIPT_DIR/config.json')); print(c.get('port',8990))" 2>/dev/null || echo "8990")
            echo "[*] kiro-rs 运行中 (PID $PID，端口 $PORT)"
        else
            echo "[!] PID 文件存在但进程已退出"
        fi
    else
        echo "[*] kiro-rs 未运行"
    fi
}

do_log() {
    if [ -f "$LOG_FILE" ]; then
        tail -f "$LOG_FILE"
    else
        echo "[!] 日志文件不存在: $LOG_FILE"
    fi
}

case "${1:-start}" in
    start)   do_start ;;
    stop)    do_stop ;;
    restart) do_stop; sleep 1; set +e; do_start ;;
    status)  do_status ;;
    log)     do_log ;;
    *)
        echo "用法: $0 {start|stop|restart|status|log}"
        echo ""
        echo "环境变量配置（也可写入 config.json）："
        echo "  API_KEY=sk-your-key"
        echo "  ADMIN_API_KEY=sk-admin-key"
        echo "  PORT=8990"
        echo "  HOST=0.0.0.0"
        echo "  REGION=us-east-1"
        echo "  PROXY_URL=http://127.0.0.1:7890"
        echo "  KIRO_LOG_DIR=/var/log/kiro-rs"
        exit 1
        ;;
esac
```

- [ ] **Step 2: Make executable**

```bash
chmod +x start_server.sh
```

- [ ] **Step 3: Smoke test**

```bash
# 测试 help 输出（不需要二进制存在）
bash start_server.sh help 2>&1 | grep -q "用法" && echo "PASS" || echo "FAIL"

# 测试 status（服务未运行时）
bash start_server.sh status
# Expected: [*] kiro-rs 未运行
```

- [ ] **Step 4: Commit**

```bash
git add start_server.sh
git commit -m "feat: add start_server.sh for Linux server deployment"
```

---

### Task 3: systemd service unit `kiro-rs.service`

**Files:**
- Create: `kiro-rs.service`

Mirrors KiroProxy2cc4mac's `kiro-proxy.service`. Enables `systemctl start/stop/enable` management and auto-restart on crash.

- [ ] **Step 1: Create `kiro-rs.service`**

```ini
[Unit]
Description=kiro-rs Anthropic API Reverse Proxy
After=network.target

[Service]
Type=simple
User=root
WorkingDirectory=/opt/kiro-rs

# 从 .env 文件加载配置（文件不存在时忽略）
EnvironmentFile=-/opt/kiro-rs/.env

ExecStart=/opt/kiro-rs/target/release/kiro-rs
Restart=always
RestartSec=10

# 日志写入 journald（用 journalctl -u kiro-rs -f 查看）
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
```

- [ ] **Step 2: Add `.env.example` for server env-var config**

Create `server.env.example`:

```bash
# kiro-rs 服务器环境变量配置
# 复制为 .env 并修改

# 必填
API_KEY=sk-kiro-rs-your-key

# 可选
# ADMIN_API_KEY=sk-admin-your-key
# PORT=8990
# HOST=0.0.0.0
# REGION=us-east-1
# AUTH_REGION=us-east-1
# API_REGION=us-east-1
# PROXY_URL=http://127.0.0.1:7890
# PROXY_USERNAME=
# PROXY_PASSWORD=
# LOAD_BALANCING_MODE=priority
```

- [ ] **Step 3: Commit**

```bash
git add kiro-rs.service server.env.example
git commit -m "feat: add systemd service unit and server env example"
```

---

### Task 4: Server one-shot install script `install_server.sh`

**Files:**
- Create: `install_server.sh`

Automates the full server setup: install Rust if missing, clone/copy repo, build binary, create config, install systemd service, enable and start.

- [ ] **Step 1: Create `install_server.sh`**

```bash
#!/bin/bash
# kiro-rs 服务器一键安装脚本
# 适用于 Debian/Ubuntu/CentOS Linux
# 用法: bash install_server.sh

set -e

INSTALL_DIR="/opt/kiro-rs"
SERVICE_NAME="kiro-rs"
REPO_DIR="$(cd "$(dirname "$0")" && pwd)"

echo "=================================================="
echo "  kiro-rs 服务器安装脚本"
echo "=================================================="

# ── 检查 root ──────────────────────────────────────────
if [ "$(id -u)" -ne 0 ]; then
    echo "[!] 请以 root 身份运行: sudo bash install_server.sh"
    exit 1
fi

# ── 安装 Rust（如未安装） ──────────────────────────────
if ! command -v cargo &>/dev/null; then
    echo "[*] 安装 Rust 工具链..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
    source "$HOME/.cargo/env"
fi

echo "[*] Rust 版本: $(rustc --version)"

# ── 编译 ──────────────────────────────────────────────
echo "[*] 编译 kiro-rs（首次需要几分钟）..."
cd "$REPO_DIR"
cargo build --release
echo "[*] 编译完成 ✓"

# ── 安装到 /opt/kiro-rs ────────────────────────────────
echo "[*] 安装到 $INSTALL_DIR ..."
mkdir -p "$INSTALL_DIR"

# 复制二进制
cp "$REPO_DIR/target/release/kiro-rs" "$INSTALL_DIR/kiro-rs"
chmod +x "$INSTALL_DIR/kiro-rs"

# 复制配置示例（不覆盖已有配置）
if [ ! -f "$INSTALL_DIR/config.json" ]; then
    if [ -f "$REPO_DIR/config.json" ]; then
        cp "$REPO_DIR/config.json" "$INSTALL_DIR/config.json"
    elif [ -f "$REPO_DIR/config.example.json" ]; then
        cp "$REPO_DIR/config.example.json" "$INSTALL_DIR/config.json"
        echo "[!] 已复制示例配置到 $INSTALL_DIR/config.json，请编辑填入真实 apiKey"
    fi
fi

# 复制凭证文件（不覆盖已有）
if [ ! -f "$INSTALL_DIR/credentials.json" ] && [ -f "$REPO_DIR/credentials.json" ]; then
    cp "$REPO_DIR/credentials.json" "$INSTALL_DIR/credentials.json"
fi

# ── 安装 systemd 服务 ──────────────────────────────────
echo "[*] 安装 systemd 服务..."

# 生成服务文件（使用实际安装路径）
cat > "/etc/systemd/system/${SERVICE_NAME}.service" <<EOF
[Unit]
Description=kiro-rs Anthropic API Reverse Proxy
After=network.target

[Service]
Type=simple
User=root
WorkingDirectory=$INSTALL_DIR
EnvironmentFile=-$INSTALL_DIR/.env
ExecStart=$INSTALL_DIR/kiro-rs
Restart=always
RestartSec=10
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable "$SERVICE_NAME"
systemctl start "$SERVICE_NAME"

sleep 2
if systemctl is-active --quiet "$SERVICE_NAME"; then
    PORT=$(python3 -c "import json; c=json.load(open('$INSTALL_DIR/config.json')); print(c.get('port',8990))" 2>/dev/null || echo "8990")
    echo ""
    echo "=================================================="
    echo "  安装完成 ✓"
    echo "  API 端点: http://localhost:${PORT}/v1/messages"
    echo "  查看日志: journalctl -u $SERVICE_NAME -f"
    echo "  停止服务: systemctl stop $SERVICE_NAME"
    echo "  重启服务: systemctl restart $SERVICE_NAME"
    echo "=================================================="
else
    echo "[!] 服务启动失败，请检查日志: journalctl -u $SERVICE_NAME -n 50"
    exit 1
fi
```

- [ ] **Step 2: Make executable**

```bash
chmod +x install_server.sh
```

- [ ] **Step 3: Smoke test (dry run — no actual systemd)**

```bash
# 验证脚本语法
bash -n install_server.sh && echo "PASS: syntax OK" || echo "FAIL: syntax error"
```

- [ ] **Step 4: Commit**

```bash
git add install_server.sh
git commit -m "feat: add install_server.sh for one-shot Linux server setup"
```

---

### Task 5: Update `.gitignore` and clean up

**Files:**
- Modify: `.gitignore`
- Delete: `docker-compose.yml` (optional — only if user confirms)

The `kiro-rs.pid` and `logs/` directory should be ignored. The `docker-compose.yml` can be kept for reference or removed.

- [ ] **Step 1: Update `.gitignore`**

Read current `.gitignore` first, then add:

```
# Runtime files
kiro-rs.pid
logs/
server.env
.env
```

- [ ] **Step 2: Verify `.gitignore` additions don't conflict with existing entries**

```bash
grep -n "pid\|logs\|\.env" .gitignore
```

Expected: shows the newly added lines without duplicates.

- [ ] **Step 3: Commit**

```bash
git add .gitignore
git commit -m "chore: ignore runtime pid/log files and .env"
```

---

### Task 6: Update README with new startup instructions

**Files:**
- Modify: `README.md`

Add a "快速启动" section near the top covering both macOS local and Linux server scenarios, replacing the Docker-centric VPS section.

- [ ] **Step 1: Read current README**

Read `README.md` to find the existing deployment/startup section.

- [ ] **Step 2: Add macOS local startup section**

Find the section describing local startup and replace/augment with:

```markdown
## 本地启动（macOS）

双击 `start.command` 即可。

首次运行会弹出配置向导，填入 API Key 后自动生成 `config.json` 并启动服务。

手动启动：
```bash
bash start.command
```

## 服务器部署（Linux）

### 方式一：一键安装（推荐）

```bash
# 克隆仓库
git clone <repo-url> /opt/kiro-rs-src
cd /opt/kiro-rs-src

# 编辑配置
cp config.example.json config.json
nano config.json  # 填入 apiKey

# 一键安装并注册 systemd 服务
sudo bash install_server.sh
```

安装完成后服务开机自启，常用命令：

```bash
systemctl status kiro-rs
systemctl restart kiro-rs
journalctl -u kiro-rs -f   # 实时日志
```

### 方式二：手动管理（无 systemd）

```bash
bash start_server.sh start    # 后台启动
bash start_server.sh status   # 查看状态
bash start_server.sh log      # 实时日志
bash start_server.sh stop     # 停止
bash start_server.sh restart  # 重启
```

### 环境变量配置

服务器上可用环境变量覆盖 `config.json`（适合容器/CI 场景）：

| 变量 | 说明 |
|------|------|
| `API_KEY` | 访问密钥 |
| `ADMIN_API_KEY` | 管理后台密钥 |
| `PORT` | 监听端口（默认 8990） |
| `HOST` | 监听地址（默认 127.0.0.1） |
| `REGION` | AWS 区域（默认 us-east-1） |
| `PROXY_URL` | HTTP/SOCKS5 代理 |
```

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: update README with no-docker startup instructions"
```

---

## Self-Review

**Spec coverage check:**
- ✅ macOS local startup → Task 1 (`start.command`)
- ✅ Linux server startup with start/stop/restart/status → Task 2 (`start_server.sh`)
- ✅ Server auto-start on boot → Task 3 (`kiro-rs.service`)
- ✅ One-shot server install → Task 4 (`install_server.sh`)
- ✅ No Docker required in any path
- ✅ kiro-rs Rust binary unchanged (no src/ modifications)
- ✅ Environment variable config for server → Task 3 (`server.env.example`) + Task 4

**Placeholder scan:** No TBD/TODO found. All code blocks are complete.

**Type consistency:** Shell scripts only — no type mismatches possible.

**Edge cases covered:**
- Binary not compiled → auto-compile or clear error message
- Port already in use → kill old process (macOS) or error (server)
- config.json missing → wizard (macOS) or copy example (server install)
- Rust not installed → install prompt (macOS) or auto-install (server)
