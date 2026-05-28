#!/bin/bash
# kiro2cc-proxy macOS 本地启动脚本
# 双击即可启动，无需 Docker

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

# ============================================================
# 可选：本地覆盖配置（取消注释并修改）
# ============================================================
# export API_KEY=sk-kiro2cc-proxy-your-key
# export ADMIN_API_KEY=sk-admin-your-key
# export PORT=5678
# export HOST=127.0.0.1
# export REGION=us-east-1
# export PROXY_URL=http://127.0.0.1:7890
# ============================================================

CONFIG_DIR="$SCRIPT_DIR/app/config"
CONFIG_FILE="$CONFIG_DIR/config.json"
CREDENTIALS_FILE="$CONFIG_DIR/credentials.json"
BINARY="$SCRIPT_DIR/target/release/kiro2cc-proxy"

echo "=================================================="
echo "  kiro2cc-proxy 启动脚本"
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
    mkdir -p "$CONFIG_DIR"

    while [ -z "$API_KEY_INPUT" ]; do
        read -p "  API Key（访问此代理的密钥，自定义即可）: " API_KEY_INPUT
    done

    read -p "  Admin API Key（管理后台密码，直接回车跳过）: " ADMIN_KEY_INPUT

    while true; do
        read -p "  端口 [默认: 5678]: " input_port
        PORT_INPUT="${input_port:-5678}"
        if [[ "$PORT_INPUT" =~ ^[0-9]+$ ]] && [ "$PORT_INPUT" -ge 1024 ] && [ "$PORT_INPUT" -le 65535 ]; then
            break
        fi
        echo "  [!] 端口必须为 1024-65535 之间的整数，请重新输入"
    done

    read -p "  Region [默认: us-east-1]: " input_region
    REGION_INPUT="${input_region:-us-east-1}"

    echo ""
    echo "  [代理设置] Kiro API 需要通过代理访问（国内必须配置）"
    read -p "  本地 HTTP 代理端口（直接回车跳过，例如: 7890 / 10089）: " input_proxy_port
    PROXY_BLOCK=""
    if [ -n "$input_proxy_port" ]; then
        PROXY_BLOCK=",
  \"proxyUrl\": \"http://127.0.0.1:$input_proxy_port\""
    fi

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
  "region": "$REGION_INPUT"$ADMIN_BLOCK$PROXY_BLOCK
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
CONFIGURED_PORT=$(python3 -c "import json; c=json.load(open('$CONFIG_FILE')); print(c.get('port',5678))" 2>/dev/null || echo "5678")
OLD_PID=$(lsof -ti tcp:"$CONFIGURED_PORT" 2>/dev/null | head -1)
if [ -n "$OLD_PID" ] && [[ "$OLD_PID" =~ ^[0-9]+$ ]]; then
    echo "[*] 端口 $CONFIGURED_PORT 被 PID $OLD_PID 占用，正在终止..."
    kill "$OLD_PID" 2>/dev/null
    sleep 2
    if kill -0 "$OLD_PID" 2>/dev/null; then
        kill -9 "$OLD_PID" 2>/dev/null
    fi
fi

echo "[*] 启动 kiro2cc-proxy，端口: $CONFIGURED_PORT"
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
exec "$BINARY" --config "$CONFIG_FILE" --credentials "$CREDENTIALS_FILE"
