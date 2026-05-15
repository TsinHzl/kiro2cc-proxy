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

if ! command -v cargo &>/dev/null; then
    echo "[!] cargo 仍不可用，请手动安装 Rust 后重试: https://rustup.rs"
    exit 1
fi

echo "[*] Rust 版本: $(rustc --version)"

# ── 编译（以原始用户身份，避免 root 污染 cargo 缓存） ──
echo "[*] 编译 kiro-rs（首次需要几分钟）..."
cd "$REPO_DIR"
if [ -n "$SUDO_USER" ]; then
    sudo -u "$SUDO_USER" cargo build --release
else
    cargo build --release
fi
echo "[*] 编译完成 ✓"

# ── 安装到 /opt/kiro-rs（编译成功后再创建目录） ────────
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
