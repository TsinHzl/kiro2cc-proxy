#!/usr/bin/env bash
# 受控实验：验证 prompt cache 是否生效
#
# 用法: ./scripts/test-cache-effectiveness.sh [proxy_url] [api_key]
#
# 原理：同一 session UUID 的两条内容完全相同的请求，
# 若 prompt cache 生效，R2 的 effective_rate 应明显低于 R1（理论降幅 ~80%）。
# 对比服务器日志中 [usage] 入库 的 effective_rate 字段。

set -euo pipefail

PROXY_URL="${1:-http://localhost:3000}"
API_KEY="${2:-test-key}"
SESSION_UUID="$(uuidgen | tr '[:upper:]' '[:lower:]')"
USER_ID="user_0000000000000000000000000000000000000000000000000000000000000000_account__session_${SESSION_UUID}"

PAYLOAD=$(cat <<EOF
{
  "model": "claude-sonnet-4-6",
  "max_tokens": 64,
  "messages": [{"role": "user", "content": "Reply with exactly one word: pong"}],
  "metadata": {"user_id": "${USER_ID}"},
  "stream": true
}
EOF
)

echo "Session UUID : ${SESSION_UUID}"
echo "Proxy        : ${PROXY_URL}"
echo ""

# 从 SSE 流中提取 message_start 事件的 usage 字段
extract_usage() {
  grep "^data:" | grep "message_start" | head -1 | sed 's/^data: //' | jq '.message.usage // empty' 2>/dev/null || echo "(no usage found)"
}

echo "=== R1 (cold — no cache) ==="
R1=$(curl -s -X POST "${PROXY_URL}/v1/messages" \
  -H "Content-Type: application/json" \
  -H "x-api-key: ${API_KEY}" \
  -d "${PAYLOAD}")
echo "${R1}" | extract_usage

echo ""
echo "=== R2 (same session — cache should hit) ==="
R2=$(curl -s -X POST "${PROXY_URL}/v1/messages" \
  -H "Content-Type: application/json" \
  -H "x-api-key: ${API_KEY}" \
  -d "${PAYLOAD}")
echo "${R2}" | extract_usage

echo ""
echo "=== 如何判断结果 ==="
echo "在服务器日志中过滤（session UUID 前8位: ${SESSION_UUID:0:8}）："
echo ""
echo "  grep 'usage.*入库' <log_file>"
echo ""
echo "对比两条日志的 effective_rate 字段："
echo "  - R2 effective_rate 比 R1 低 ~80% → prompt cache 生效"
echo "  - R2 effective_rate 与 R1 相近    → prompt cache 未命中"
echo ""
echo "若 Kiro 已透传 cache 字段，日志中 cache_read / cache_creation 会有非 None 值。"
