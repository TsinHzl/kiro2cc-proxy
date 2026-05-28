#!/bin/bash
# kiro2cc-proxy Linux 服务器启动脚本
# 支持 start / stop / restart / status 子命令
# 所有配置均可通过环境变量或 config.json 覆盖

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BINARY="$SCRIPT_DIR/target/release/kiro2cc-proxy"
PID_FILE="$SCRIPT_DIR/kiro2cc-proxy.pid"
LOG_DIR="${KIRO_LOG_DIR:-$SCRIPT_DIR/logs}"
LOG_FILE="$LOG_DIR/kiro2cc-proxy.log"

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
            echo "[!] kiro2cc-proxy 已在运行 (PID $OLD_PID)"
            exit 1
        fi
        rm -f "$PID_FILE"
    fi

    mkdir -p "$LOG_DIR"

    echo "[*] 启动 kiro2cc-proxy，日志: $LOG_FILE"
    cd "$SCRIPT_DIR"
    nohup "$BINARY" >> "$LOG_FILE" 2>&1 &
    echo $! > "$PID_FILE"
    sleep 1

    if kill -0 "$(cat "$PID_FILE")" 2>/dev/null; then
        echo "[*] 已启动，PID $(cat "$PID_FILE")"
        # 读取配置中的端口用于提示
        PORT=$(python3 -c "import json; c=json.load(open('$SCRIPT_DIR/config.json')); print(c.get('port',5678))" 2>/dev/null || echo "5678")
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
        sleep 1
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
            PORT=$(python3 -c "import json; c=json.load(open('$SCRIPT_DIR/config.json')); print(c.get('port',5678))" 2>/dev/null || echo "5678")
            echo "[*] kiro2cc-proxy 运行中 (PID $PID，端口 $PORT)"
        else
            echo "[!] PID 文件存在但进程已退出"
        fi
    else
        echo "[*] kiro2cc-proxy 未运行"
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
    restart) set +e; do_stop; sleep 1; do_start ;;
    status)  do_status ;;
    log)     do_log ;;
    *)
        echo "用法: $0 {start|stop|restart|status|log}"
        echo ""
        echo "环境变量配置（也可写入 config.json）："
        echo "  API_KEY=sk-your-key"
        echo "  ADMIN_API_KEY=sk-admin-key"
        echo "  PORT=5678"
        echo "  HOST=0.0.0.0"
        echo "  REGION=us-east-1"
        echo "  PROXY_URL=http://127.0.0.1:7890"
        echo "  KIRO_LOG_DIR=/var/log/kiro2cc-proxy"
        exit 1
        ;;
esac
