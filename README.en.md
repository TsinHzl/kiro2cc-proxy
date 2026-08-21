# 🌉 kiro2cc-proxy

**A Rust-based Anthropic Claude API-compatible proxy that converts Anthropic API requests into Kiro API requests.**

[![Tests](https://github.com/TsinHzl/kiro2cc-proxy/actions/workflows/test.yaml/badge.svg)](https://github.com/TsinHzl/kiro2cc-proxy/actions/workflows/test.yaml)
[![codecov](https://codecov.io/gh/TsinHzl/kiro2cc-proxy/graph/badge.svg)](https://codecov.io/gh/TsinHzl/kiro2cc-proxy)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](#-license)

**🇺🇸 English** | **[🇨🇳 中文](README.md)**

> **✅ Supported Models: Claude Sonnet 5 / Claude Sonnet 4.5 / Claude Sonnet 4.6 / Claude Opus 4.5 / Claude Opus 4.6 / Claude Opus 4.7 / Claude Opus 4.8 / Claude Opus 5 / Claude Haiku 4.5 / DeepSeek 3.2 / GLM-5 / MiniMax M2.1 / MiniMax M2.5 / Qwen3-Coder / GPT-5.6 Sol / GPT-5.6 Terra / GPT-5.6 Luna**

### 📖 Quick Navigation

[🚀 Quick Start](#-quick-start-new-users) ・ [🍎 macOS Deployment](#-local-deployment-macos) ・ [🪟 Windows Deployment](#-local-deployment-windows) ・ [🐧 Linux Deployment](#-server-deployment-linux) ・ [🔑 Get Accounts](#-getting-kiro-accounts) ・ [⚙️ Configuration](#-configuration-reference) ・ [🤖 Claude Code Integration](#-claude-code-integration) ・ [🧬 Codex CLI / OpenAI SDK](#-codex-cli--openai-sdk-integration) ・ [🔌 API Endpoints](#-api-endpoints) ・ [🗺️ Model Mapping](#-model-mapping) ・ [🛡️ Admin Panel](#-admin-panel) ・ [❓ FAQ](#-faq) ・ [⚠️ Notes](#-notes)

---

## ⚠️ Disclaimer

This project is for research purposes only. Use at your own risk. Any consequences arising from the use of this project are solely the responsibility of the user. This project is not affiliated with AWS, KIRO, Anthropic, or Claude in any official capacity.

---

## ✨ Features

| Feature | Description |
|---|---|
| **🔌 Anthropic API Compatible** | Full support for the Anthropic Claude API format |
| **🧬 OpenAI Compatible / Codex CLI** | Exposes `/v1/chat/completions` and `/v1/responses` endpoints for direct Codex CLI and OpenAI SDK client integration |
| **📡 Streaming Responses** | SSE (Server-Sent Events) streaming support |
| **🔄 Auto Token Refresh** | Automatically manages and refreshes OAuth tokens |
| **👥 Multi-Account Support** | Configure multiple accounts with automatic priority-based failover |
| **⚖️ Load Balancing** | `priority` (by priority) and `balanced` (round-robin) modes |
| **🔁 Smart Retry** | Up to 3 retries per account, up to 9 retries per request |
| **🧠 Thinking Mode** | Supports Claude's extended thinking feature |
| **🛠️ Tool Use** | Full support for function calling / tool use |
| **🔍 WebSearch** | Built-in WebSearch tool conversion logic |
| **🛡️ Admin Panel** | Optional web management UI for account management, balance queries, etc. |
| **🌐 Per-Account Proxy** | Configure HTTP/SOCKS5 proxy per account |

---

## 🚀 Quick Start (New Users)

**What is this project?**

kiro2cc-proxy is a proxy service. It forwards standard Anthropic Claude API requests to Kiro (AWS's AI coding tool), allowing you to use Claude Code with models from your Kiro account.

> In short: it proxies the models on your logged-in Kiro account to Claude Code. Without it, you can only use those models inside Kiro IDE or Kiro CLI.

**Prerequisites:**

1. A Kiro account (register at [kiro.dev](https://kiro.dev), supports Social login)
2. Accounts exported from Kiro IDE or account manager (`refreshToken` etc.)
3. > ⚠️ **[CRITICAL] Users in mainland China**: A local HTTP/SOCKS5 proxy (Clash/V2Ray etc.) is mandatory. Without it, all Claude model requests will return `INVALID_MODEL_ID` and the service will be unusable.

**Overall flow:**

```
Install dependencies → Build project → Start service → Add accounts → Configure client
```

---

## 🍎 Local Deployment (macOS)

### Step 1: Install Dependencies

Open Terminal and install Node.js and Rust:

```bash
# Install Homebrew (skip if already installed)
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

# Install Node.js
brew install node

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# After installation, reopen Terminal or run:
source "$HOME/.cargo/env"
```

### Step 2: Get the Code

```bash
git clone https://github.com/TsinHzl/kiro2cc-proxy.git
cd kiro2cc-proxy
```

### (Optional) Install Shell Aliases

Run the one-click installer to make `build_kiro2cc_proxy` and `run_kiro2cc_proxy` available from any terminal — no need to navigate to the project directory each time:

```bash
bash setup_shell_aliases.sh
source ~/.zshrc   # zsh users; bash users run: source ~/.bashrc
```

After installation:

```bash
build_kiro2cc_proxy   # equivalent to ./build-mac.sh
run_kiro2cc_proxy     # equivalent to ./run-local-service-mac.sh
```

> macOS only. Modifies `~/.zshrc` and `~/.bashrc` (if they exist). Safe to run multiple times (idempotent).

### Step 3: Build the Project

```bash
./build-mac.sh
```

This script builds the admin-ui frontend, user-ui frontend, and then compiles the Rust binary. First build takes 5–15 minutes.

On success:
```
  Build complete!
  Binary: ./target/release/kiro2cc-proxy
```

> No need to rebuild unless you update the code.

### Step 4: Start the Service

**Option A: Double-click (recommended)**

In Finder, navigate to the project directory and double-click `run-local-service-mac.sh`.

**Option B: Terminal**

```bash
./run-local-service-mac.sh
```

**First launch** shows a setup wizard:

```
Admin Password (admin panel password (http://ip:port/admin), press Enter to skip): [default: my-admin-pass]
Port [default: 5678]:
Region [default: us-east-1]:
Local HTTP proxy port (e.g. 7890 / 10089): [enter your proxy port]
```

- **⚠️ [CRITICAL] Local HTTP proxy port**: This is the port your VPN/proxy software listens on. **Without it, Claude models such as Claude 4.6 and Claude 4.7 will be inaccessible when running locally.**

- > ⚠️ **[CRITICAL] Proxy port (required for mainland China users)**
  >
  > A common way to check: run `export http_proxy=http://127.0.0.1:10089; export https_proxy=http://127.0.0.1:10089;` in your terminal — the `10089` here is your proxy port.
  >
  > If you don't know the port number, check the settings page of your proxy software.

- **Admin Password**: **The login password for the admin panel (http://ip:port/admin). Setting this is recommended.**

After setup, `app/config/config.json` is generated, the service starts, and the admin panel opens in your browser automatically.

**Subsequent launches** read the existing config — no wizard needed.

### Step 5: Add Kiro Accounts

After the service starts, open the admin panel at `http://127.0.0.1:5678/admin` and add accounts exported from Kiro.

Alternatively, create `app/config/credentials.json` directly — see the "Getting Kiro Accounts" section.

### Stop the Service

Press `Ctrl+C` in the terminal running the service, or close the terminal window.

---

## 🪟 Local Deployment (Windows)

### Step 1: Install Dependencies

1. Install [Node.js](https://nodejs.org) (LTS version)
2. Install [Rust](https://rustup.rs) (download and run `rustup-init.exe`)
3. Install [Git](https://git-scm.com/download/win)

After installation, reopen PowerShell and verify these commands work:

```powershell
node -v
cargo -v
git -v
```

### Step 2: Get the Code

```powershell
git clone https://github.com/TsinHzl/kiro2cc-proxy.git
cd kiro2cc-proxy
```

### (Optional) Install PowerShell Aliases

Run the one-click installer to make `build_kiro2cc_proxy` and `run_kiro2cc_proxy` available from any PowerShell window — no need to navigate to the project directory each time:

```powershell
.\setup_shell_aliases.ps1
. $PROFILE
```

After installation:

```powershell
build_kiro2cc_proxy   # equivalent to .\build-windows.ps1
run_kiro2cc_proxy     # equivalent to .\run-local-service-windows.ps1
```

> Updates both Windows PowerShell 5.x and PowerShell 7+ profiles. Safe to run multiple times (idempotent).

### Step 3: Build the Project

Open PowerShell as Administrator and allow script execution (one-time):

```powershell
Set-ExecutionPolicy -Scope CurrentUser RemoteSigned
```

Then build:

```powershell
.\build-windows.ps1
```

This script builds the admin-ui frontend, user-ui frontend, and then compiles the Rust binary. First build takes 5–15 minutes.

> No need to rebuild unless you update the code.

### Step 4: Start the Service

```powershell
.\run-local-service-windows.ps1
```

**First launch** shows a setup wizard:

```
Admin Password (admin panel password (http://ip:port/admin), press Enter to skip): [default: my-admin-pass]
Port [default: 5678]:
Region [default: us-east-1]:
Local HTTP proxy port (e.g. 7890 / 10089): [enter your proxy port]
```

- **⚠️ [CRITICAL] Local HTTP proxy port**: This is the port your VPN/proxy software listens on. **Without it, Claude models such as Claude 4.6 and Claude 4.7 will be inaccessible when running locally.**

- > ⚠️ **[CRITICAL] Proxy port (required for mainland China users)**
  >
  > A common way to check: run `export http_proxy=http://127.0.0.1:10089; export https_proxy=http://127.0.0.1:10089;` in your terminal — the `10089` here is your proxy port.
  >
  > If you don't know the port number, check the settings page of your proxy software.

- **Admin Password**: **The login password for the admin panel (http://ip:port/admin). Setting this is recommended.**

After setup, `app\config\config.json` is generated, the service starts, and the admin panel opens in your browser automatically.

**Subsequent launches** read the existing config — no wizard needed.

### Step 5: Add Kiro Accounts

After the service starts, open the admin panel at `http://127.0.0.1:5678/admin` and add accounts exported from Kiro.

### Stop the Service

Press `Ctrl+C` in the PowerShell window, or close the window.

---

## 🐧 Server Deployment (Linux)

### Option 1: Docker (Simplest, Recommended)

**Requirements**: Docker and Docker Compose installed on the server.

```bash
# 1. Clone the repo
git clone https://github.com/TsinHzl/kiro2cc-proxy.git /opt/kiro2cc-proxy
cd /opt/kiro2cc-proxy

# 2. Create config file (note: config lives in data/, not data/config/)
mkdir -p data
cp config.example.json data/config.json
nano data/config.json   # Fill in adminPsw
```

Minimal `data/config.json`:

```json
{
  "host": "0.0.0.0",
  "port": 5678,
  "region": "us-east-1",
  "adminPsw": "your-admin-password"
}
```

> ⚠️ **`port` must be an integer**, not a Docker port-mapping string (e.g. `"0.0.0.0:5678:5678"`). Correct: `"port": 5678`. The service will fail to start if this is wrong.

```bash
# 3. Create accounts file (or add via admin panel after startup)
echo "[]" > data/credentials.json

# 4. Start
docker compose up -d

# View logs
docker compose logs -f

# Stop
docker compose down
```

Access the admin panel at `http://your-server-ip:5678/admin`.

> **Note**: `docker-compose.yml` binds to `5678:5678` (all interfaces) by default. To restrict to localhost only, change `ports` to `"127.0.0.1:5678:5678"`. Also make sure your cloud provider's security group (Tencent Cloud / Alibaba Cloud etc.) has an inbound rule allowing port 5678, otherwise external access will be refused.

#### Updating to the Latest Version

```bash
cd /opt/kiro2cc-proxy
git pull
docker compose pull
docker compose down && docker compose up -d
```

> Each time a new tag (e.g. `v1.x.x`) is pushed, GitHub Actions automatically builds and pushes a new image to `ghcr.io`. `docker compose pull` fetches the latest `latest` image.

### Option 2: systemd One-Click Install

For running the binary directly without Docker.

```bash
# 1. Clone the repo
git clone https://github.com/TsinHzl/kiro2cc-proxy.git /opt/kiro2cc-proxy-src
cd /opt/kiro2cc-proxy-src

# 2. Create config
cp config.example.json app/config/config.json
nano app/config/config.json   # Fill in adminPsw

# 3. Install (auto-compiles + registers systemd service)
sudo bash install_server.sh
```

The service starts automatically on boot. Common commands:

```bash
systemctl status kiro2cc-proxy       # Check status
systemctl restart kiro2cc-proxy      # Restart
systemctl stop kiro2cc-proxy         # Stop
journalctl -u kiro2cc-proxy -f       # Live logs
```

### Option 3: Manual Background Process (No systemd)

```bash
bash start_server.sh start     # Start in background
bash start_server.sh status    # Check status
bash start_server.sh log       # Live logs
bash start_server.sh stop      # Stop
bash start_server.sh restart   # Restart
```

### Proxy Configuration for Servers

Servers in mainland China cannot access Kiro API directly. Add a proxy to `config.json`:

```json
{
  "proxyUrl": "http://your-proxy-host:port"
}
```

Using an overseas server is recommended — no proxy needed.

---

## 🔑 Getting Kiro Accounts

### Full Flow: Export from Kiro Account Manager → Import via Admin Panel

**Step 1: Export account JSON from Kiro Account Manager**

1. Install Kiro IDE or Kiro Account Manager
2. Sign in with your GitHub / Google Social account
3. Find the "Export Account" option in the account management interface
4. Export as a JSON file (or copy the JSON content)

**Step 2: Start the kiro2cc-proxy service**

Follow the "Local Deployment" or "Server Deployment" section to start the service and confirm it is running.

**Step 3: Import accounts via the Admin Panel (recommended)**

1. Open the admin panel: `http://127.0.0.1:5678/admin` (**replace with your server IP for server deployments**)
2. **Log in with the `adminPsw` (Admin Password) configured in `config.json`**
3. Go to the accounts management page
4. **Paste** the exported JSON content into the input field, or **drag and drop** the JSON file onto the page
5. The panel automatically recognizes the account info and displays it — confirm to save

> ℹ️ **Importing accounts over HTTP**
>
> Since v2.7.3, a pure-JS fallback is built in, so importing accounts works fine even when accessing the admin panel via `http://server-ip:port/admin` (not HTTPS, not localhost) — no need to configure HTTPS or browser flags.
>
> If you're on v2.7.2 or earlier, the browser's security policy still disables the `crypto.subtle` encryption API in this case, causing an error `Cannot read properties of undefined (reading 'digest')`. Please upgrade to the latest version.

**Step 4 (optional): Create the accounts file manually**

You can skip the admin panel and save the exported JSON directly as a file:
- Local deployment: `app/config/credentials.json`
- Docker deployment: `data/credentials.json`

See the format reference below. Restart the service after saving.

### credentials.json Format

**Social login (single account):**

```json
{
  "refreshToken": "your-refresh-token",
  "expiresAt": "2025-12-31T02:32:45.144Z",
  "authMethod": "social"
}
```

**IDC/Builder-ID login (single account):**

```json
{
  "refreshToken": "your-refresh-token",
  "expiresAt": "2025-12-31T02:32:45.144Z",
  "authMethod": "idc",
  "clientId": "your-client-id",
  "clientSecret": "your-client-secret"
}
```

**Multiple accounts (array format, with failover):**

```json
[
  {
    "refreshToken": "token-1",
    "expiresAt": "2025-12-31T02:32:45.144Z",
    "authMethod": "social",
    "priority": 0
  },
  {
    "refreshToken": "token-2",
    "expiresAt": "2025-12-31T02:32:45.144Z",
    "authMethod": "social",
    "priority": 1
  }
]
```

Lower `priority` value = higher priority. Up to 3 retries per account, 9 per request, with automatic failover.

---

## ⚙️ Configuration Reference

### config.json Fields

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `host` | No | `127.0.0.1` | Listen address; `0.0.0.0` allows external/LAN access |
| `port` | No | `5678` | Listen port |
| `region` | No | `us-east-1` | AWS region |
| `authRegion` | No | same as `region` | Region used for token refresh |
| `apiRegion` | No | same as `region` | Region used for API requests |
| `adminPsw` | No | — | Admin Password (admin panel login password); omit to disable admin panel |
| `proxyUrl` | No | — | HTTP/SOCKS5 proxy, e.g. `http://127.0.0.1:7890` |
| `proxyUsername` | No | — | Proxy username |
| `proxyPassword` | No | — | Proxy password |
| `tlsBackend` | No | `rustls` | TLS backend: `rustls` or `native-tls` |
| `loadBalancingMode` | No | `priority` | `priority` (by priority) or `balanced` (round-robin) |

> **TLS note**: If you encounter token refresh failures or request errors, try switching `tlsBackend` to `native-tls`.

Full example:

```json
{
  "host": "0.0.0.0",
  "port": 5678,
  "region": "us-east-1",
  "adminPsw": "my-admin-password",
  "proxyUrl": "http://127.0.0.1:7890",
  "tlsBackend": "rustls",
  "loadBalancingMode": "priority"
}
```

> **Client authentication**: `config.json` no longer provides a global API key. Clients calling `/v1/messages` or `/cc/v1/messages` must authenticate with a sub API key created and enabled in the admin panel (login with `adminPsw`). Note: if `adminPsw` is left unset, the admin panel itself is disabled — you won't be able to create sub API keys until it's configured.

### Per-Account Proxy

Override the global proxy for individual accounts:

```json
[
  {
    "refreshToken": "token-a",
    "authMethod": "social",
    "proxyUrl": "socks5://proxy-a.example.com:1080"
  },
  {
    "refreshToken": "token-b",
    "authMethod": "social",
    "proxyUrl": "direct"
  }
]
```

`proxyUrl: "direct"` forces direct connection for that account, ignoring any global proxy.

### Region Priority

**Auth Region** (token refresh): `account.authRegion` > `account.region` > `config.authRegion` > `config.region`

**API Region** (API requests): `account.apiRegion` > `config.apiRegion` > `config.region`

---

## 🤖 Claude Code Integration

### Option 1: Environment Variables (recommended)

Set these environment variables in your terminal to route Claude Code through this proxy:

```bash
export ANTHROPIC_BASE_URL="http://127.0.0.1:5678"
export ANTHROPIC_API_KEY="API key created in the admin panel's API Key Management page"
```

**Persist across sessions** (add to `~/.zshrc` or `~/.bashrc`):

```bash
echo 'export ANTHROPIC_BASE_URL="http://127.0.0.1:5678"' >> ~/.zshrc
echo 'export ANTHROPIC_API_KEY="API key created on the admin panel API Key Management page"' >> ~/.zshrc
source ~/.zshrc
```

### Option 2: settings.json

Configure the proxy directly in Claude Code's settings file — no need to set environment variables each time.

Config file locations:
- Global: `~/.claude/settings.json`
- Per-project: `<project-root>/.claude/settings.json` (applies to current project only)

Add the following to the config file:

```json
{
  "env": {
    "ANTHROPIC_BASE_URL": "http://127.0.0.1:5678",
    "ANTHROPIC_API_KEY": "API key created in the admin panel's API Key Management page"
  }
}
```

If the file already has other settings, merge the `env` field in:

```json
{
  "theme": "dark",
  "env": {
    "ANTHROPIC_BASE_URL": "http://127.0.0.1:5678",
    "ANTHROPIC_API_KEY": "API key created in the admin panel's API Key Management page"
  }
}
```

**Verify it works:**

```bash
curl http://127.0.0.1:5678/v1/messages \
  -H "Content-Type: application/json" \
  -H "x-api-key: your-api-key" \
  -d '{
    "model": "claude-sonnet-4-20250514",
    "max_tokens": 100,
    "messages": [{"role": "user", "content": "hi"}]
  }'
```

---

## 🧬 Codex CLI / OpenAI SDK Integration

In addition to the Anthropic protocol, the proxy also exposes two OpenAI-compatible endpoints, so you can drive Codex CLI or any OpenAI SDK client with your Kiro quota. Both endpoints work by "translate + forward": the request is converted to Anthropic format and reuses the full `/v1/messages` downstream pipeline (multi-account failover, RPM counting, usage tracking, rate limiting), then the response is converted back to OpenAI format.

### Codex CLI Setup

Edit `~/.codex/config.toml` (shared by the Codex CLI and the ChatGPT.app desktop client):

```toml
model = "gpt-5.6-terra"
model_provider = "kiro"

[model_providers.kiro]
name = "kiro2cc-proxy"
base_url = "http://127.0.0.1:5678/v1"  # 127.0.0.1 for local deployment; replace with your domain/public IP for server deployments
wire_api = "responses"        # recommended; or "chat"
env_key = "KIRO_API_KEY"
```

Put the key in `~/.codex/.env` (Codex statically links the Rust `dotenvy` crate and reloads this file on every invocation — everything stays inside `~/.codex/`, no shell environment variables needed, and changes take effect immediately without a restart):

```bash
echo 'KIRO_API_KEY=your-api-key' > ~/.codex/.env
chmod 600 ~/.codex/.env
codex
```

> **Avoid duplicate definitions**: if you created a profile with `-p/--profile <name>` (`~/.codex/<name>.config.toml`), the profile is layered on top of `config.toml` — only fields explicitly set in the profile override the base config; everything else is inherited. If the profile's `model` / `model_provider` etc. are identical to the base config, the profile is redundant (verify by running `codex exec` once without `-p` and once with `-p <name>`, then diff the output) — just delete it instead of maintaining two copies.

> **ChatGPT.app desktop client** reads the same `config.toml` (the top-level `model_provider` applies; there's currently no separate profile mechanism for the app), but it's a long-running process that loads the config once at startup — changing `config.toml` requires a **full app restart** to take effect. `~/.codex/.env`, on the other hand, is re-read on every call, so edits apply immediately without restarting anything.

**`wire_api` values:**

| Value | Endpoint used | Notes |
|-------|---------------|-------|
| `responses` | `/v1/responses` | Codex CLI's default protocol, most complete feature set (reasoning events, freeform tools) |
| `chat` | `/v1/chat/completions` | Generic Chat Completions protocol, widest compatibility |

Both support streaming and tool-call round trips; `responses` is recommended for everyday use.

### OpenAI SDK Setup

```python
from openai import OpenAI

client = OpenAI(base_url="http://127.0.0.1:5678/v1", api_key="your-api-key")

resp = client.chat.completions.create(
    model="gpt-5.6-terra",
    messages=[{"role": "user", "content": "hello"}],
)
print(resp.choices[0].message.content)
```

Model name can be `gpt-5.6-terra` / `gpt-5.6-luna` / `gpt-5.6-sol`, or any `claude-*` model name (passed through as-is to upstream). Codex CLI's built-in `gpt-5-codex` / `gpt-5.1-codex` auto-map to `gpt-5.6-terra`, and `gpt-5.1-codex-max` maps to `gpt-5.6-luna`.

### Known Limitations

- **`previous_response_id` is not supported** — the proxy is stateless and doesn't persist prior responses. Requests containing this field return 400 immediately without an upstream call; send the full conversation history in `input` instead (Codex CLI already does this by default, no extra config needed).
- **`tool_choice` only supports `auto`** — other values (`required` or a specific function name) are logged as WARN and treated as `auto`; this is an existing limitation of the upstream Kiro API.
- **`reasoning.effort` has no effect on `gpt-5.6-luna`** — this model always returns `thinking=0` upstream.
- **`include: ["reasoning.encrypted_content"]` is ignored** — the proxy doesn't produce encrypted reasoning content.

---

## 🔌 API Endpoints

### Standard Endpoints (/v1)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/v1/models` | GET | List available models |
| `/v1/messages` | POST | Create a message (chat) |
| `/v1/messages/count_tokens` | POST | Estimate token count |

### OpenAI Compatible Endpoints (/v1)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/v1/chat/completions` | POST | OpenAI Chat Completions protocol; supports streaming, tool calls, `reasoning_effort` |
| `/v1/responses` | POST | OpenAI Responses protocol; Codex CLI's default `wire_api` |

> See [🧬 Codex CLI / OpenAI SDK Integration](#-codex-cli--openai-sdk-integration) for details.

### Claude Code Compatible Endpoints (/cc/v1)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/cc/v1/messages` | POST | Buffered mode with accurate `input_tokens` |
| `/cc/v1/messages/count_tokens` | POST | Estimate token count |

> `/cc/v1/messages` waits for the full upstream stream to complete before returning. `input_tokens` uses the actual value rather than an estimate. Sends a `ping` keepalive every 25 seconds while waiting.

### Client Authentication

Two methods supported:

```
x-api-key: your-api-key
```
or
```
Authorization: Bearer your-api-key
```

---

## 🗺️ Model Mapping

Any model name containing the following keywords is automatically mapped to the corresponding Kiro model:

| Request model name (keyword) | Kiro model used |
|------------------------------|----------------|
| `*sonnet*` (including 4.6/4-6) | `claude-sonnet-4.6` |
| `*sonnet*` (including 5/sonnet-5) | `claude-sonnet-5` |
| `*sonnet*` (others) | `claude-sonnet-4.5` |
| `*opus*` (including 5/opus-5) | `claude-opus-5` |
| `*opus*` (including 4.5/4-5) | `claude-opus-4.5` |
| `*opus*` (including 4.7/4-7) | `claude-opus-4.7` |
| `*opus*` (including 4.8/4-8) | `claude-opus-4.8` |
| `*opus*` (others) | `claude-opus-4.6` |
| `*fable*` | `claude-fable-5` |
| `*haiku*` | `claude-haiku-4.5` |
| `*deepseek*` | `deepseek-3.2` |
| `*glm*` | `glm-5` |
| `*minimax*` (including 2.5/2-5) | `minimax-m2.5` |
| `*minimax*` (others) | `minimax-m2.1` |
| `*qwen*` | `qwen3-coder-next` |
| `*gpt*` (including terra) | `gpt-5.6-terra` |
| `*gpt*` (including luna) | `gpt-5.6-luna` |
| `*gpt*` (including sol, or 5.6/5-6 without a variant name, defaults to the flagship tier) | `gpt-5.6-sol` |

---

## 🛡️ Admin Panel

When `adminPsw` is configured, access the admin panel at `http://127.0.0.1:5678/admin`.

Features:
- View all account statuses (validity, failure count, etc.)
- Add / delete accounts
- Enable / disable individual accounts
- Adjust account priority
- Check account balance
- Reset account failure state

**Admin API** (requires `x-api-key` or `Authorization: Bearer` header):

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/admin/credentials` | GET | List all accounts |
| `/api/admin/credentials` | POST | Add an account |
| `/api/admin/credentials/:id` | DELETE | Delete an account |
| `/api/admin/credentials/:id/balance` | GET | Query balance |

---

## ❓ FAQ

**Q: Service starts but shows "0 accounts loaded"**

Create `app/config/credentials.json` (local) or `data/credentials.json` (Docker). See the "Getting Kiro Accounts" section.

**Q: Requests return <code>INVALID_MODEL_ID</code>**

> ⚠️ **[CRITICAL]** Mainland China IPs cannot access Claude models directly. You must add `proxyUrl` to `app/config/config.json` (e.g. `"proxyUrl": "http://127.0.0.1:7890"`), or use an overseas server. This is the most common issue for users in China.

**Q: When using GPT-5.6 models (sol/terra/luna), thinking mode / output effort / max_tokens settings seem to have no effect**

The Kiro backend schema for the GPT-5.6 series does not support `additionalModelRequestFields` (covering the thinking / output_config effort / max_tokens sub-fields) — same as the Claude 4.5 generation (Sonnet 4.5 / Opus 4.5 / Haiku 4.5), this field is skipped entirely. This is a known upstream limitation, not a bug in this project.

**Q: Requests return 401 Unauthorized**

Check whether the client is using a sub API key that was created and enabled in the admin panel — disabled or deleted keys are rejected.

**Q: Token refresh fails / request errors**

Try changing `tlsBackend` to `native-tls` in `config.json` and restart the service.

**Q: Importing accounts via the admin panel fails with <code>Cannot read properties of undefined (reading 'digest')</code>**

This was fixed in v2.7.3: the `crypto.subtle` encryption API is only available in HTTPS or localhost environments, so accessing the admin panel via a public IP + HTTP used to trigger this error. Since v2.7.3, it automatically falls back to a pure-JS implementation — no need to configure HTTPS. If you still see this error, please upgrade to the latest version.

**Q: Enterprise IdC account requests return 502 with <code>profileArn is required for this request</code> in the logs**

Enterprise IdC accounts calling the Q endpoint require a `profileArn`, but the IdC token refresh response doesn't include it — it must be entered manually. The admin panel's "Add Account / Edit Account" dialog now has a **Profile ARN** field; fill in a value like `arn:aws:codewhisperer:<region>:<account-id>:profile/<profile-id>`. You can obtain the `profileArn` from the Kiro IDE local cache or via `ListAvailableProfiles`; its region must match the account's `apiRegion`. Social accounts usually don't need this field.

**Q: Can sub-API-Key spending limits be metered in real Kiro credits instead of estimated USD?**

Yes. When creating/editing a sub API Key, the limit unit can be set to "USD estimate" or "real credits" (`limitUnit`: usd/credits). With credits, the limit is checked against the real `credits_used` accumulated in usage records (falls back to `estimated_cost × k_ref` for older records without `credits_used`). Defaults to `usd`, fully backward compatible.

**Q: Port already in use**

`run-local-service-mac.sh` automatically kills the process occupying the configured port. If it still fails:

```bash
lsof -ti:5678 | xargs kill -9
```

**Q: Write Failed / session hangs**

Output truncated due to excessive length. Lower the `max_tokens` limit in your client.

**Q: Other devices on LAN can't connect**

Set `host` to `0.0.0.0` in `config.json` and ensure your firewall allows the port.

**Q: How to update to the latest version (Docker deployment)**

```bash
cd /opt/kiro2cc-proxy
git pull
docker compose pull
docker compose down && docker compose up -d
```

**Q: How to update to the latest version (local deployment)**

```bash
git pull
./build-mac.sh
./run-local-service-mac.sh
```

---

## ⚠️ Notes

1. `credentials.json` contains sensitive tokens — never commit it to version control or share it
2. The service auto-refreshes expired tokens — no manual intervention needed
3. In multi-account mode, refreshed tokens are automatically written back to the file
4. Mainland China users must configure a proxy to access Claude models

---

## 📂 Project Structure

```
kiro2cc-proxy/
├── src/                    # Rust source code
├── admin-ui/               # Admin panel frontend
├── user-ui/                # User panel frontend
├── app/config/             # Local config directory (gitignored)
├── config.example.json     # Config example
├── docker-compose.yml      # Docker deployment config
├── Dockerfile              # Docker image build
├── build-mac.sh            # One-click build script (macOS)
├── build-windows.ps1       # One-click build script (Windows)
├── run-local-service-mac.sh         # macOS local startup script
├── run-local-service-windows.ps1   # Windows local startup script
├── setup_shell_aliases.sh  # macOS shell alias installer
├── setup_shell_aliases.ps1 # Windows PowerShell alias installer
├── install_server.sh       # Linux systemd one-click install
└── start_server.sh         # Linux manual background process manager
```

---

### 📜 License

MIT

### 🙏 Acknowledgements

This project is based on [kiro.rs](https://github.com/hank9999/kiro.rs). Thanks to the original author for the open-source contribution.
