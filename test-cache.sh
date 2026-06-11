#!/usr/bin/env bash
# Kiro 真实缓存命中率测试脚本
# 原理：每轮请求后从 admin API 取 creditsUsed（Kiro 真实账单），
# 用反推公式 cache_read = creditsSaved / (0.9 × k_ref × input_price/M)
# 计算实际命中率，而非代理固定 76.5% 的模拟值。
#
# 公式推导：
#   credits = k × (input_price/M × (total − 0.9×R) + output_price/M × output)
#   creditsSaved = k × estimatedCost − creditsUsed
#               ≈ k × 0.9 × input_price/M × cache_read   (cache_creation 占比小可忽略)
#   → cache_read = creditsSaved / (0.9 × k × input_price/M)

set -uo pipefail

# ──────── 配置 ────────
API_KEY="sk-2b3f2712f19746c6a85272c48125f20e"
ADMIN_KEY="Hackair007!"
BASE_URL="http://localhost:5678"
MODEL="claude-sonnet-4-5"
ROUNDS=5
# k_ref（sonnet 实测值）
K_REF=7.06
INPUT_PRICE=3.0    # $/M tokens
OUTPUT_PRICE=15.0  # $/M tokens
# ──────────────────────

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
CYAN='\033[0;36m'; BOLD='\033[1m'; DIM='\033[2m'; RESET='\033[0m'

TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

echo -e "${BOLD}╔══════════════════════════════════════════════════════════════╗${RESET}"
echo -e "${BOLD}║        Kiro 真实缓存命中率测试（credits 反推法）             ║${RESET}"
echo -e "${BOLD}╚══════════════════════════════════════════════════════════════╝${RESET}"
echo -e "  模型: ${CYAN}${MODEL}${RESET} | k_ref=${CYAN}${K_REF}${RESET} | input=\$${INPUT_PRICE}/M | output=\$${OUTPUT_PRICE}/M"
echo ""

# ──────── 生成 ~10k token 初始大文本 ────────
generate_large_content() {
  python3 - <<'PYEOF'
sections = ["""
# Kiro2CC Proxy — Architecture & Developer Reference

## 1. Overview

kiro2cc-proxy is a high-performance reverse proxy written in Rust that bridges Anthropic-compatible
clients with the Kiro API backend. It implements full protocol translation between the Anthropic
Messages API format and Kiro's proprietary binary frame protocol, enabling seamless use of Kiro's
Claude models through standard Anthropic SDK clients such as claude-code, continue.dev, and cursor.

The proxy supports multi-account failover, OAuth token refresh, streaming SSE translation,
prompt cache simulation, and a built-in admin dashboard for monitoring usage metrics.

## 2. Core Components

### 2.1 Request Pipeline

  Client (Anthropic format)
    │
    ▼
  middleware.rs  ─── Authentication (x-api-key / Bearer)
                 ─── RPM rate limiting (per API key)
                 ─── Usage tracking injection
    │
    ▼
  handlers.rs   ─── Route dispatch: /v1/messages vs /cc/v1/messages
                ─── Payload validation and model normalization
    │
    ▼
  converter.rs  ─── Anthropic → Kiro protocol conversion
                ─── Tool schema normalization (Kiro rejects null fields)
                ─── History reconstruction with cache-friendly ordering
    │
    ▼
  provider.rs   ─── Account selection (priority / balanced / round-robin)
                ─── Retry logic: MAX 3 per account, MAX 9 total
    │
    ▼
  token_manager.rs ─── OAuth token pool management
                    ─── Automatic token refresh on 401
                    ─── Credential persistence to credentials.json
    │
    ▼
  Kiro API (binary frame protocol over HTTPS)
    │
    ▼
  parser/ (frame.rs + decoder.rs + crc.rs)
    ─── Binary frame deserialization
    ─── CRC32 integrity verification
    │
    ▼
  stream.rs  ─── Kiro events → Anthropic SSE state machine
             ─── meteringEvent → real cache token extraction
             ─── message_delta with usage statistics

### 2.2 Binary Frame Protocol

  ┌─────────────────────────────────────────────────────────┐
  │  Header (8 bytes)                                       │
  │  ├── Magic:   0xDEAD (2 bytes)                          │
  │  ├── Version: 0x01   (1 byte)                           │
  │  ├── Type:    0x01=data / 0x02=control / 0x03=error     │
  │  └── Length:  3 bytes big-endian                        │
  │  Payload: JSON event data                               │
  │  Trailer: CRC32 checksum (4 bytes)                      │
  └─────────────────────────────────────────────────────────┘

### 2.3 Token Management

  Social Auth: email+password → access+refresh tokens (~1h / ~30d TTL)
  IDC Auth:    AWS STS-style temporary credentials
  Failover:    per-account 3 retries, global 9 retries

### 2.4 Prompt Cache Architecture

  Kiro implements server-side prefix caching. The proxy structures requests:
  1. Tool definitions in history[2] to maximize prefix overlap
  2. System message always first, anchoring the cache prefix
  3. Each assistant response included verbatim in subsequent requests

  Cache token reporting:
    a) meteringEvent (real):  cache_read_input_tokens + cache_creation_input_tokens
    b) Simulation fallback:   fixed(0.85) × (1-0.1) = 76.5% (if Kiro omits fields)
    c) Credits inference:     creditsSaved / (0.9 × k × input_price/M)

### 2.5 Credit Accounting

  k ≈ 7.06 (1 credit ≈ $0.1416 USD)   [sonnet series]
  cache_read_tokens:     billed at  10% of full input rate
  cache_creation_tokens: billed at 125% of full input rate
  output_tokens:         billed at output rate

## 3. Admin API

  GET  /api/admin/api-keys/{id}/usage/records — per-request usage with creditsUsed
  GET  /api/admin/credentials/{id}/usage/records — per-credential records
  GET  /api/admin/server-info                 — health, uptime, account count

## 4. Error Codes

  400 invalid_request      / 401 authentication_error / 403 forbidden
  429 rate_limit_error     / 500 api_error            / 502 overloaded_error

## 5. Performance

  Idle ~20 MB RSS; 100 concurrent ~250 MB RSS.
  CRC32 hardware-accelerated on x86_64 via crc32fast.
  Streaming chunk-by-chunk — no full response buffering.
"""]

filler = """
## Appendix: Error Reference

| Code | Type                 | Cause                                 | Resolution                        |
|------|----------------------|---------------------------------------|-----------------------------------|
| 400  | invalid_request      | Malformed JSON / missing field        | Validate against API spec         |
| 401  | authentication_error | Missing or invalid API key            | Check x-api-key header            |
| 403  | forbidden            | Spending limit exceeded               | Contact admin                     |
| 429  | rate_limit_error     | RPM limit exceeded                    | Exponential backoff               |
| 500  | api_error            | Upstream Kiro internal error          | Retry with backoff                |
| 502  | overloaded_error     | All 9 retries exhausted               | Wait; check Kiro status           |

Connection pool: max_idle_per_host=10, connect_timeout=10s, read_timeout=300s.
Keep-alive enabled. For >100 req/s increase max_idle_per_host to 50.
ulimit -n ≥ 65536 recommended for high-throughput deployments.
"""

full = "\n".join(sections) + filler * 8
target = 40000
print(full[:target])
PYEOF
}

generate_small_message() {
  local round=$1
  printf "第 %d 轮：请简述 Kiro 前缀缓存在第 %d 次请求时的命中条件，以及 history[%d] 对缓存 key 的影响。" \
    "$round" "$round" "$round"
}

# ──────── 发送流式请求 ────────
send_request() {
  local messages_json="$1" out_file="$2"
  curl -s -N -X POST "${BASE_URL}/v1/messages" \
    -H "x-api-key: ${API_KEY}" \
    -H "content-type: application/json" \
    --max-time 120 \
    -d "$(jq -n --arg m "$MODEL" --argjson msgs "$messages_json" \
      '{model:$m,max_tokens:200,stream:true,messages:$msgs}')" \
    > "$out_file"
}

# ──────── 从 SSE 提取 assistant 文本 ────────
parse_assistant_text() {
  local f="$1"
  grep '^data: ' "$f" | sed 's/^data: //' \
    | jq -rs '[.[] | select(.type=="content_block_delta" and .delta.type=="text_delta") | .delta.text] | join("")' 2>/dev/null
}

# ──────── 从 SSE 提取 message_delta usage（proxy 报告的模拟值） ────────
parse_proxy_usage() {
  local f="$1"
  awk '/^event: message_delta/{found=1;next} found && /^data:/{print;found=0}' "$f" \
    | sed 's/^data: //' | jq -r '.usage // empty' 2>/dev/null
}

# ──────── 从 admin API 取最新 N 条记录 ────────
fetch_latest_records() {
  local n="$1"
  curl -s "${BASE_URL}/api/admin/api-keys/1/usage/records" \
    -H "Authorization: Bearer ${ADMIN_KEY}" \
    | jq --argjson n "$n" '.records[0:$n]' 2>/dev/null
}

# ──────── credits 反推命中率 ────────
infer_hit_rate() {
  python3 - "$@" <<'PYEOF'
import sys, json

k        = float(sys.argv[1])
inp_p    = float(sys.argv[2])
out_p    = float(sys.argv[3])
credits  = float(sys.argv[4])
inp_tok  = int(sys.argv[5])
out_tok  = int(sys.argv[6])

rate   = k * inp_p / 1_000_000          # credits per input token (full price)
est    = (inp_p * inp_tok + out_p * out_tok) / 1_000_000  # estimatedCost USD
saved  = k * est - credits               # creditsSaved

# cache_read = creditsSaved / (0.9 × rate)
cache_read = max(0, saved / (0.9 * rate))
cache_read = min(cache_read, inp_tok)
hit_rate   = cache_read / inp_tok * 100 if inp_tok > 0 else 0

print(f"{cache_read:.0f} {hit_rate:.1f} {saved:.6f}")
PYEOF
}

# ════════ 主流程 ════════

echo -e "${BOLD}[准备]${RESET} 生成初始大文本..."
LARGE_CONTENT=$(generate_large_content)
echo -e "  内容长度: ${CYAN}${#LARGE_CONTENT}${RESET} 字符（≈ $(( ${#LARGE_CONTENT} / 4 )) tokens）"
echo ""

MESSAGES=$(jq -n --arg c "$LARGE_CONTENT" '[{"role":"user","content":$c}]')
PREV_TEXT=""

# 记录测试开始前的记录数（用于精确匹配本次测试的记录）
RECORDS_BEFORE=$(curl -s "${BASE_URL}/api/admin/api-keys/1/usage/records" \
  -H "Authorization: Bearer ${ADMIN_KEY}" | jq '.records | length' 2>/dev/null || echo 0)

echo -e "${BOLD}┌──────┬──────────────┬──────────┬──────────────┬──────────────┬──────────┬──────────┐${RESET}"
echo -e "${BOLD}│ 轮次 │  总input tok │ out tok  │ creditsUsed  │ 推算cache_rd │ 真实命中 │ 模拟命中 │${RESET}"
echo -e "${BOLD}├──────┼──────────────┼──────────┼──────────────┼──────────────┼──────────┼──────────┤${RESET}"

TOTAL_CACHE_READ_REAL=0
TOTAL_INPUT_ALL=0

for round in $(seq 1 $ROUNDS); do
  if [ "$round" -gt 1 ] && [ -n "$PREV_TEXT" ]; then
    SMALL=$(generate_small_message "$round")
    MESSAGES=$(echo "$MESSAGES" | jq \
      --arg a "$PREV_TEXT" --arg u "$SMALL" \
      '. + [{"role":"assistant","content":$a},{"role":"user","content":$u}]')
  fi

  MSG_N=$(echo "$MESSAGES" | jq 'length')
  SSE_FILE="${TMP_DIR}/r${round}.sse"

  echo -ne "  ${DIM}第 ${round} 轮（${MSG_N} 条消息）请求中...${RESET}"
  send_request "$MESSAGES" "$SSE_FILE"

  # 检查 SSE 错误
  ERR=$(grep '^data: ' "$SSE_FILE" | sed 's/^data: //' \
    | jq -r 'select(.type=="error") | .error.message // empty' 2>/dev/null | head -1)
  if [ -n "$ERR" ]; then
    echo -e " ${RED}失败: ${ERR}${RESET}"
    continue
  fi

  # proxy 报告的模拟 usage
  PROXY_USAGE=$(parse_proxy_usage "$SSE_FILE")
  PROXY_INPUT=$(echo "$PROXY_USAGE"  | jq -r '.input_tokens // 0')
  PROXY_CREATE=$(echo "$PROXY_USAGE" | jq -r '.cache_creation_input_tokens // 0')
  PROXY_READ=$(echo "$PROXY_USAGE"   | jq -r '.cache_read_input_tokens // 0')
  PROXY_OUT=$(echo "$PROXY_USAGE"    | jq -r '.output_tokens // 0')
  PROXY_TOTAL=$(( PROXY_INPUT + PROXY_CREATE + PROXY_READ ))
  PROXY_HIT="0.0"
  [ "$PROXY_TOTAL" -gt 0 ] && PROXY_HIT=$(echo "scale=1; $PROXY_READ * 100 / $PROXY_TOTAL" | bc)

  PREV_TEXT=$(parse_assistant_text "$SSE_FILE")

  # 等待 admin 写入（usage tracker 是异步的）
  sleep 1

  # 取 admin 最新记录
  LATEST=$(curl -s "${BASE_URL}/api/admin/api-keys/1/usage/records" \
    -H "Authorization: Bearer ${ADMIN_KEY}" \
    | jq '.records[0]' 2>/dev/null)

  CREDITS=$(echo "$LATEST" | jq -r '.creditsUsed // empty')
  INP_TOK=$(echo "$LATEST" | jq -r '.inputTokens // 0')
  OUT_TOK=$(echo "$LATEST" | jq -r '.outputTokens // 0')

  echo -e "\r$(printf '%-52s' ' ')\r"

  if [ -z "$CREDITS" ] || [ "$CREDITS" = "null" ]; then
    printf "  │ %-4s │ %-12s │ %-8s │ %-12s │ %-12s │ %-8s │ %-8s │\n" \
      "$round" "${PROXY_TOTAL}" "${PROXY_OUT}" "N/A (无credits)" "N/A" "N/A" "${PROXY_HIT}%"
    continue
  fi

  # 反推真实命中率
  INFER=$(infer_hit_rate "$K_REF" "$INPUT_PRICE" "$OUTPUT_PRICE" "$CREDITS" "$INP_TOK" "$OUT_TOK")
  CACHE_READ_REAL=$(echo "$INFER" | awk '{print $1}')
  HIT_REAL=$(echo "$INFER" | awk '{print $2}')
  SAVED=$(echo "$INFER" | awk '{print $3}')

  TOTAL_CACHE_READ_REAL=$(echo "$TOTAL_CACHE_READ_REAL + $CACHE_READ_REAL" | bc)
  TOTAL_INPUT_ALL=$(( TOTAL_INPUT_ALL + INP_TOK ))

  # 着色
  HIT_INT=${HIT_REAL%.*}
  if [ "${HIT_INT:-0}" -ge 70 ] 2>/dev/null; then RCOLOR=$GREEN
  elif [ "${HIT_INT:-0}" -ge 40 ] 2>/dev/null; then RCOLOR=$YELLOW
  else RCOLOR=$RED; fi

  printf "  │ %-4s │ %-12s │ %-8s │ %-12s │ %-12s │ ${RCOLOR}%-8s${RESET} │ ${DIM}%-8s${RESET} │\n" \
    "$round" "$INP_TOK" "$OUT_TOK" \
    "$(printf '%.5f' "$CREDITS")" \
    "${CACHE_READ_REAL}" \
    "${HIT_REAL}%" \
    "${PROXY_HIT}%"
done

echo -e "${BOLD}└──────┴──────────────┴──────────┴──────────────┴──────────────┴──────────┴──────────┘${RESET}"
echo ""

if [ "$TOTAL_INPUT_ALL" -gt 0 ] && [ "$(echo "$TOTAL_CACHE_READ_REAL > 0" | bc)" = "1" ]; then
  OVERALL=$(echo "scale=1; $TOTAL_CACHE_READ_REAL * 100 / $TOTAL_INPUT_ALL" | bc)
  echo -e "  ${BOLD}总体真实命中率: ${GREEN}${OVERALL}%${RESET}  ${DIM}(credits反推法，k_ref=${K_REF}, \$${INPUT_PRICE}/M input)${RESET}"
  echo -e "  推算累计 cache_read: ${CYAN}${TOTAL_CACHE_READ_REAL%.*}${RESET} tokens"
fi

echo ""
echo -e "${BOLD}说明：${RESET}"
echo "  真实命中 = creditsSaved / (0.9 × k_ref × input_price/M)"
echo "  creditsSaved = k_ref × estimatedCost(USD) - creditsUsed"
echo "  模拟命中 = 代理固定公式 input×0.85×0.90 = 76.5%（与实际无关）"
echo ""
echo -e "  ${DIM}注：infer 公式忽略了 cache_creation 的溢价（约占总 token 的 8%），"
echo -e "  实际误差约 ±2%，cache_creation 越少则越精准。${RESET}"
