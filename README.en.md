# kiro2cc-proxy

A Rust-based Anthropic Claude API-compatible proxy that converts Anthropic API requests into Kiro API requests.

> **✅ Supported Models: Claude Sonnet 4.5 / Claude Sonnet 4.6 / Claude Opus 4.5 / Claude Opus 4.6 / Claude Opus 4.7 / Claude Haiku 4.5 / DeepSeek 3.2 / GLM-5 / MiniMax M2.1 / MiniMax M2.5 / Qwen3-Coder**

[中文](README.md) | English

## Disclaimer

This project is for research purposes only. Use at your own risk. Any consequences arising from the use of this project are solely the responsibility of the user. This project is not affiliated with AWS, KIRO, Anthropic, or Claude in any official capacity.

## Features

- **Anthropic API Compatible**: Full support for the Anthropic Claude API format
- **Streaming Responses**: SSE (Server-Sent Events) streaming support
- **Auto Token Refresh**: Automatically manages and refreshes OAuth tokens
- **Multi-Credential Support**: Configure multiple credentials with automatic priority-based failover
- **Load Balancing**: `priority` (by priority) and `balanced` (round-robin) modes
- **Smart Retry**: Up to 3 retries per credential, up to 9 retries per request
- **Thinking Mode**: Supports Claude's extended thinking feature
- **Tool Use**: Full support for function calling / tool use
- **WebSearch**: Built-in WebSearch tool conversion logic
- **Admin Panel**: Optional web management UI for credential management, balance queries, etc.
- **Per-Credential Proxy**: Configure HTTP/SOCKS5 proxy per credential

---

## Table of Contents

- [Quick Start (New Users)](#quick-start-new-users)
- [Local Deployment (macOS)](#local-deployment-macos)
- [Local Deployment (Windows)](#local-deployment-windows)
- [Server Deployment (Linux)](#server-deployment-linux)
- [Getting Kiro Credentials](#getting-kiro-credentials)
- [Configuration Reference](#configuration-reference)
- [Claude Code Integration](#claude-code-integration)
- [API Endpoints](#api-endpoints)
- [Model Mapping](#model-mapping)
- [Admin Panel](#admin-panel)
- [FAQ](#faq)
- [Notes](#notes)

---

## Quick Start (New Users)

**What is this project?**

kiro2cc-proxy is a local proxy service. It forwards standard Anthropic Claude API requests to Kiro (AWS's AI coding tool), allowing you to use Claude models for free with a Kiro account.

**Prerequisites:**

1. A Kiro account (register at [kiro.dev](https://kiro.dev), supports Social login)
2. Credentials exported from Kiro IDE or account manager (`refreshToken` etc.)
3. > ⚠️ **[CRITICAL] Users in mainland China**: A local HTTP/SOCKS5 proxy (Clash/V2Ray etc.) is mandatory. Without it, all Claude model requests will return `INVALID_MODEL_ID` and the service will be unusable.

**Overall flow:**

```
Install dependencies → Build project → Start service → Add credentials → Configure client
```

---

## Local Deployment (macOS)

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
git clone <repo-url>
cd kiro2cc-proxy
```

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
  API Key (access key for this proxy, set anything you like): sk-my-proxy-key
  Admin API Key (admin panel password, press Enter to skip): my-admin-pass
  Port [default: 5678]:
  Region [default: us-east-1]:
  Local HTTP proxy port (press Enter to skip, e.g. 7890 / 10089): 7890
```

- **API Key**: Set any value — clients use this to authenticate
- **Admin API Key**: Password for the admin panel, recommended
- > ⚠️ **[CRITICAL] Proxy port (required for mainland China users)**: Without a proxy, Claude models are completely inaccessible. Enter the HTTP listen port of your local proxy software (Clash/V2Ray/Shadowsocks etc.), e.g. `7890` or `10089`. Check your proxy app's settings if you're unsure of the port number.

After setup, `app/config/config.json` is generated, the service starts, and the admin panel opens in your browser.

**Subsequent launches** read the existing config — no wizard needed.

### Step 5: Add Kiro Credentials

After the service starts, open the admin panel at `http://127.0.0.1:5678/admin` and add credentials exported from Kiro.

Alternatively, create `app/config/credentials.json` directly — see [Getting Kiro Credentials](#getting-kiro-credentials).

### Stop the Service

Press `Ctrl+C` in the terminal running the service, or close the terminal window.

---

## Local Deployment (Windows)

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
git clone <repo-url>
cd kiro2cc-proxy
```

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
  API Key (access key for this proxy, set anything you like): sk-my-proxy-key
  Admin API Key (admin panel password, press Enter to skip): my-admin-pass
  Port [default: 5678]:
  Region [default: us-east-1]:
  Local HTTP proxy port (press Enter to skip, e.g. 7890 / 10089): 7890
```

- > ⚠️ **[CRITICAL] Proxy port (required for mainland China users)**: Without a proxy, Claude models are completely inaccessible. Enter the HTTP listen port of your local proxy software (Clash/V2Ray/Shadowsocks etc.), e.g. `7890` or `10089`.

After setup, `app\config\config.json` is generated, the service starts, and the admin panel opens in your browser.

**Subsequent launches** read the existing config — no wizard needed.

### Step 5: Add Kiro Credentials

After the service starts, open the admin panel at `http://127.0.0.1:5678/admin` and add credentials exported from Kiro.

### Stop the Service

Press `Ctrl+C` in the PowerShell window, or close the window.

---

## Server Deployment (Linux)

### Option 1: Docker (Simplest, Recommended)

**Requirements**: Docker and Docker Compose installed on the server.

```bash
# 1. Clone the repo
git clone <repo-url> /opt/kiro2cc-proxy
cd /opt/kiro2cc-proxy

# 2. Create data directory and config
mkdir -p data
cp config.example.json data/config.json
nano data/config.json   # Fill in apiKey and adminApiKey
```

Minimal `data/config.json`:

```json
{
  "host": "0.0.0.0",
  "port": 5678,
  "apiKey": "sk-your-api-key",
  "region": "us-east-1",
  "adminApiKey": "your-admin-password"
}
```

```bash
# 3. Create credentials file (or add via admin panel after startup)
nano data/credentials.json

# 4. Start
docker compose up -d

# View logs
docker compose logs -f

# Stop
docker compose down
```

Access the admin panel at `http://your-server-ip:5678/admin`.

> **Note**: Docker Compose defaults to `127.0.0.1:5678`. For external access, change `ports` in `docker-compose.yml` to `"0.0.0.0:5678:5678"` and open the port in your firewall.

### Option 2: systemd One-Click Install

For running the binary directly without Docker.

```bash
# 1. Clone the repo
git clone <repo-url> /opt/kiro2cc-proxy-src
cd /opt/kiro2cc-proxy-src

# 2. Create config
cp config.example.json app/config/config.json
nano app/config/config.json   # Fill in apiKey

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

## Getting Kiro Credentials

### Full Flow: Export from Kiro Account Manager → Import via Admin Panel

**Step 1: Export account JSON from Kiro Account Manager**

1. Install Kiro IDE or Kiro Account Manager
2. Sign in with your GitHub / Google Social account
3. Find the "Export Account" option in the account management interface
4. Export as a JSON file (or copy the JSON content)

**Step 2: Start the kiro2cc-proxy service**

Follow the [Local Deployment](#local-deployment-macos) or [Server Deployment](#server-deployment-linux) section to start the service.

**Step 3: Import credentials via the Admin Panel (recommended)**

1. Open the admin panel: `http://127.0.0.1:5678/admin` (replace with your server IP for server deployments)
2. Log in with the `adminApiKey` configured in `config.json`
3. Go to the credentials management page
4. **Paste** the exported JSON content into the input field, or **drag and drop** the JSON file onto the page
5. The panel automatically recognizes the account info and displays it — confirm to save

> ⚠️ **Importing credentials fails when accessing the admin panel over HTTP**
>
> If you access the admin panel via `http://server-ip:port/admin` (not HTTPS, not localhost), the browser's security policy disables the `crypto.subtle` encryption API, causing an error `Cannot read properties of undefined (reading 'digest')` during import. The backend will show no error logs.
>
> **Solution 1 (recommended):** Bind a domain name to your server and configure HTTPS, then access the admin panel via `https://`.
>
> **Solution 2 (temporary workaround):** Force the browser to treat the HTTP address as secure.
>
> Chrome: open `chrome://flags/#unsafely-treat-insecure-origin-as-secure`
> Edge: open `edge://flags/#unsafely-treat-insecure-origin-as-secure`
>
> Enter your full address (e.g. `http://43.153.11.66:8990`) in the text box under "Insecure origins treated as secure", set the toggle to **Enabled**, then click **Relaunch** to restart the browser.

**Step 4 (optional): Create the credentials file manually**

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

Lower `priority` value = higher priority. Up to 3 retries per credential, 9 per request, with automatic failover.

---

## Configuration Reference

### config.json Fields

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `apiKey` | **Yes** | — | API key for client authentication, set any value |
| `host` | No | `127.0.0.1` | Listen address; `0.0.0.0` allows external/LAN access |
| `port` | No | `5678` | Listen port |
| `region` | No | `us-east-1` | AWS region |
| `authRegion` | No | same as `region` | Region used for token refresh |
| `apiRegion` | No | same as `region` | Region used for API requests |
| `adminApiKey` | No | — | Admin panel password; omit to disable admin panel |
| `proxyUrl` | No | — | HTTP/SOCKS5 proxy, e.g. `http://127.0.0.1:7890` |
| `proxyUsername` | No | — | Proxy username |
| `proxyPassword` | No | — | Proxy password |
| `tlsBackend` | No | `rustls` | TLS backend: `rustls` or `native-tls` |
| `loadBalancingMode` | No | `priority` | `priority` or `balanced` (round-robin) |

> **TLS note**: If you encounter token refresh failures or request errors, try switching `tlsBackend` to `native-tls`.

Full example:

```json
{
  "host": "0.0.0.0",
  "port": 5678,
  "apiKey": "sk-my-proxy-key",
  "region": "us-east-1",
  "adminApiKey": "my-admin-password",
  "proxyUrl": "http://127.0.0.1:7890",
  "tlsBackend": "rustls",
  "loadBalancingMode": "priority"
}
```

### Per-Credential Proxy

Override the global proxy for individual credentials:

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

`proxyUrl: "direct"` forces direct connection for that credential, ignoring any global proxy.

### Region Priority

**Auth Region** (token refresh): `credential.authRegion` > `credential.region` > `config.authRegion` > `config.region`

**API Region** (API requests): `credential.apiRegion` > `config.apiRegion` > `config.region`

---

## Claude Code Integration

### Option 1: Environment Variables (recommended)

Set these environment variables in your terminal to route Claude Code through this proxy:

```bash
export ANTHROPIC_BASE_URL="http://127.0.0.1:5678"
export ANTHROPIC_API_KEY="API key created in the admin panel's API Key Management page"
```

**Persist across sessions** (add to `~/.zshrc` or `~/.bashrc`):

```bash
echo 'export ANTHROPIC_BASE_URL="http://127.0.0.1:5678"' >> ~/.zshrc
echo 'export ANTHROPIC_API_KEY="your-api-key"' >> ~/.zshrc
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

## API Endpoints

### Standard Endpoints (/v1)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/v1/models` | GET | List available models |
| `/v1/messages` | POST | Create a message (chat) |
| `/v1/messages/count_tokens` | POST | Estimate token count |

### Claude Code Compatible Endpoints (/cc/v1)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/cc/v1/messages` | POST | Buffered mode with accurate `input_tokens` |
| `/cc/v1/messages/count_tokens` | POST | Estimate token count |

> `/cc/v1/messages` buffers the full upstream stream before returning, providing accurate `input_tokens` from `contextUsageEvent`. Sends `ping` events every 25 seconds while waiting.

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

## Model Mapping

Any model name containing the following keywords is automatically mapped:

| Request model name (keyword) | Kiro model used |
|------------------------------|----------------|
| `*sonnet*` (including 4.6/4-6) | `claude-sonnet-4.6` |
| `*sonnet*` (others) | `claude-sonnet-4.5` |
| `*opus*` (including 4.5/4-5) | `claude-opus-4.5` |
| `*opus*` (including 4.7/4-7) | `claude-opus-4.7` |
| `*opus*` (others) | `claude-opus-4.6` |
| `*haiku*` | `claude-haiku-4.5` |
| `*deepseek*` | `deepseek-3.2` |
| `*glm*` | `glm-5` |
| `*minimax*` (including 2.5/2-5) | `minimax-m2.5` |
| `*minimax*` (others) | `minimax-m2.1` |
| `*qwen*` | `qwen3-coder-next` |

---

## Admin Panel

When `adminApiKey` is configured, access the admin panel at `http://127.0.0.1:5678/admin`.

Features:
- View all credential statuses (validity, failure count, etc.)
- Add / delete credentials
- Enable / disable individual credentials
- Adjust credential priority
- Check account balance
- Reset credential failure state

**Admin API** (requires `x-api-key` or `Authorization: Bearer` header):

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/admin/credentials` | GET | List all credentials |
| `/api/admin/credentials` | POST | Add a credential |
| `/api/admin/credentials/:id` | DELETE | Delete a credential |
| `/api/admin/credentials/:id/balance` | GET | Query balance |

---

## FAQ

**Q: Service starts but shows "0 credentials loaded"**

Create `app/config/credentials.json` (local) or `data/credentials.json` (Docker). See [Getting Kiro Credentials](#getting-kiro-credentials).

**Q: Requests return `INVALID_MODEL_ID`**

> ⚠️ **[CRITICAL]** Mainland China IPs cannot access Claude models directly. You must add `proxyUrl` to `app/config/config.json` (e.g. `"proxyUrl": "http://127.0.0.1:7890"`), or use an overseas server. This is the most common issue for users in China.

**Q: Requests return 401 Unauthorized**

The API key used by the client doesn't match `apiKey` in `config.json`. Check and align them.

**Q: Token refresh fails / request errors**

Try changing `tlsBackend` to `native-tls` in `config.json` and restart the service.

**Q: Importing credentials via the admin panel fails with `Cannot read properties of undefined (reading 'digest')`**

This is a browser security policy restriction. The `crypto.subtle` encryption API is only available in HTTPS or localhost environments. Accessing the admin panel via a public IP + HTTP triggers this error — the backend will show no logs.

Solutions:
- **Recommended:** Bind a domain name to your server and configure HTTPS, then access via `https://`
- **Temporary workaround:** Chrome: open `chrome://flags/#unsafely-treat-insecure-origin-as-secure`; Edge: open `edge://flags/#unsafely-treat-insecure-origin-as-secure`. Enter your full address (e.g. `http://43.153.11.66:8990`) in the text box, set to **Enabled**, click **Relaunch**

**Q: Port already in use**

`run-local-service-mac.sh` automatically kills the process occupying the configured port. If it still fails:
```bash
lsof -ti:5678 | xargs kill -9
```

**Q: Write Failed / session hangs**

Output truncated due to excessive length. Lower the `max_tokens` limit in your client.

**Q: Other devices on LAN can't connect**

Set `host` to `0.0.0.0` in `config.json` and ensure your firewall allows the port.

**Q: How to update to the latest version**

```bash
git pull
./build-mac.sh
./run-local-service-mac.sh
```

---

## Notes

1. `credentials.json` contains sensitive tokens — never commit it to version control or share it
2. The service auto-refreshes expired tokens — no manual intervention needed
3. In multi-credential mode, refreshed tokens are automatically written back to the file
4. Mainland China users must configure a proxy to access Claude models

---

## Project Structure

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
├── install_server.sh       # Linux systemd one-click install
└── start_server.sh         # Linux manual background process manager
```

---

## License

MIT

## Acknowledgements

This project is based on [kiro.rs](https://github.com/hank9999/kiro.rs). Thanks to the original author for the open-source contribution.
