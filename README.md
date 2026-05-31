# kiro2cc-proxy

一个用 Rust 编写的 Anthropic Claude API 兼容代理服务，将 Anthropic API 请求转换为 Kiro API 请求。

> **✅ 支持模型：Claude Sonnet 4.5 / Claude Sonnet 4.6 / Claude Opus 4.5 / Claude Opus 4.6 / Claude Opus 4.7 / Claude Opus 4.8 / Claude Haiku 4.5 / DeepSeek 3.2 / GLM-5 / MiniMax M2.1 / MiniMax M2.5 / Qwen3-Coder**

[English](README.en.md) | 中文

## 免责声明

本项目仅供研究使用，Use at your own risk，使用本项目所导致的任何后果由使用人承担，与本项目无关。本项目与 AWS/KIRO/Anthropic/Claude 等官方无关，不代表官方立场。

## 功能特性

- **Anthropic API 兼容**：完整支持 Anthropic Claude API 格式
- **流式响应**：支持 SSE (Server-Sent Events) 流式输出
- **Token 自动刷新**：自动管理和刷新 OAuth Token
- **多账号支持**：支持配置多个账号，按优先级自动故障转移
- **负载均衡**：支持 `priority`（按优先级）和 `balanced`（均衡分配）两种模式
- **智能重试**：单账号最多重试 3 次，单请求最多重试 9 次
- **Thinking 模式**：支持 Claude 的 extended thinking 功能
- **工具调用**：完整支持 function calling / tool use
- **WebSearch**：内置 WebSearch 工具转换逻辑
- **Admin 管理**：可选的 Web 管理界面，支持账号管理、余额查询等
- **账号级代理**：支持为每个账号单独配置 HTTP/SOCKS5 代理

---

## 目录

- [快速开始（新手必读）](#快速开始新手必读)
- [本地部署（macOS）](#本地部署macos)
- [本地部署（Windows）](#本地部署windows)
- [服务器部署（Linux）](#服务器部署linux)
- [获取 Kiro 账号](#获取-kiro-账号)
- [配置详解](#配置详解)
- [接入 Claude Code](#接入-claude-code)
- [API 端点](#api-端点)
- [模型映射](#模型映射)
- [Admin 管理面板](#admin-管理面板)
- [常见问题](#常见问题)
- [注意事项](#注意事项)

---

## 快速开始（新手必读）

**这个项目是什么？**

kiro2cc-proxy 是一个代理服务。它把标准的 Anthropic Claude API 请求转发给 Kiro（AWS 的 AI 编程工具），让你可以用 Claude Code使用Kiro账号的模型。

>  一句话说明白，就是：它能把登录的Kiro账号上的模型代理到claude code上进行使用。否则的话就只能在Kiro IDE或者Kiro Cli上使用。

**使用前提：**

1. 拥有一个 Kiro 账号（通过 [kiro.dev](https://kiro.dev) 注册，支持 Social 登录）
2. 从 Kiro IDE 或账号管理工具中导出账号（`refreshToken` 等信息）
3. > ⚠️ **【重要】国内用户**：必须配置本地 HTTP/SOCKS5 代理（Clash/V2Ray 等），否则所有 Claude 模型请求均会返回 `INVALID_MODEL_ID` 错误，无法使用。

**整体流程：**

```
安装依赖 → 构建项目 → 启动服务 → 填入账号 → 配置客户端
```

---

## 本地部署（macOS）

### 第一步：安装依赖

打开终端，安装 Node.js 和 Rust：

```bash
# 安装 Homebrew（如已安装跳过）
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

# 安装 Node.js
brew install node

# 安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# 安装完成后重新打开终端，或执行：
source "$HOME/.cargo/env"
```

### 第二步：获取项目代码

```bash
git clone https://github.com/TsinHzl/kiro2cc-proxy.git
cd kiro2cc-proxy
```

### （可选）安装 Shell 快捷命令

运行项目自带的一键安装脚本，以后可在任意终端直接使用 `build_kiro2cc_proxy` 和 `run_kiro2cc_proxy` 命令，无需切换到项目目录：

```bash
bash setup_shell_aliases.sh
source ~/.zshrc   # zsh 用户；bash 用户执行 source ~/.bashrc
```

安装后即可使用：

```bash
build_kiro2cc_proxy   # 等同于 ./build-mac.sh
run_kiro2cc_proxy     # 等同于 ./run-local-service-mac.sh
```

> 脚本仅适用于 macOS，自动修改 `~/.zshrc` 和 `~/.bashrc`（如存在），可重复运行（幂等）。

### 第三步：构建项目

```bash
./build-mac.sh
```

脚本会依次构建 admin-ui 前端、user-ui 前端，最后编译 Rust 二进制。首次构建约需 5~15 分钟。

构建成功后输出：
```
  构建成功！
  二进制位置: ./target/release/kiro2cc-proxy
```

> 后续除非更新了代码，否则无需重新构建。

### 第四步：启动服务

**方式一：双击启动（推荐）**

在 Finder 中找到项目目录，双击 `run-local-service-mac.sh` 文件。

**方式二：终端启动**

```bash
./run-local-service-mac.sh
```

**首次启动**会进入配置向导：

```
API Key（访问此代理密钥，自定义即可，可选）: [默认sk-my-proxy-key]
Admin Password（管理后台密码(http://ip:端口/admin页面)，必填）: [默认my-admin-pass]
端口 [默认: 5678]:
Region [默认: us-east-1]:
本地 HTTP 代理端口（例如: 7890 / 10089）: [填入你的代理端口]
```

- **⚠️【重要】本地 HTTP 代理端口**：也就是开魔法的端口。注意：**本地搭建的话，不配置将无法访问 Claude 模型，如Claude4.6和Claude4.7模型**

- > ⚠️ **【重要】代理端口（国内用户必须配置）**
  >
  > 常在终端上使用的命令如：export http_proxy=http://127.0.0.1:10089; export https_proxy=http://127.0.0.1:10089;
  >
  > 这里的10089就是你开魔法的端口
  >
  > 不知道端口号请查看代理软件的设置页面

- **Admin Password**：**管理面板的登录密码(http://ip:端口/admin页面)，建议设置**

配置完成后自动生成 `app/config/config.json`，服务启动，浏览器自动打开管理面板。

**后续启动**直接读取已有配置，无需重新填写。

### 第五步：填入 Kiro 账号

服务启动后，打开管理面板 `http://127.0.0.1:5678/admin`，在账号管理页面添加从 Kiro 导出的账号。

也可以直接创建 `app/config/credentials.json`，格式见[获取 Kiro 账号](#获取-kiro-账号)章节。

### 停止服务

在运行服务的终端窗口按 `Ctrl+C`，或直接关闭终端窗口。

---

## 本地部署（Windows）

### 第一步：安装依赖

1. 安装 [Node.js](https://nodejs.org)（LTS 版本）
2. 安装 [Rust](https://rustup.rs)（下载并运行 `rustup-init.exe`）
3. 安装 [Git](https://git-scm.com/download/win)

安装完成后重新打开 PowerShell，确认以下命令可用：

```powershell
node -v
cargo -v
git -v
```

### 第二步：获取项目代码

```powershell
git clone https://github.com/TsinHzl/kiro2cc-proxy.git
cd kiro2cc-proxy
```

### （可选）安装 PowerShell 快捷命令

运行项目自带的安装脚本，以后可在任意 PowerShell 直接使用 `build_kiro2cc_proxy` 和 `run_kiro2cc_proxy` 命令，无需切换到项目目录：

```powershell
.\setup_shell_aliases.ps1
. $PROFILE
```

安装后即可使用：

```powershell
build_kiro2cc_proxy   # 等同于 .\build-windows.ps1
run_kiro2cc_proxy     # 等同于 .\run-local-service-windows.ps1
```

> 脚本同时更新 Windows PowerShell 5.x 和 PowerShell 7+ 的 profile，可重复运行（幂等）。

### 第三步：构建项目

以管理员身份打开 PowerShell，先允许执行脚本（仅需一次）：

```powershell
Set-ExecutionPolicy -Scope CurrentUser RemoteSigned
```

然后构建：

```powershell
.\build-windows.ps1
```

脚本会依次构建 admin-ui 前端、user-ui 前端，最后编译 Rust 二进制。首次构建约需 5~15 分钟。

> 后续除非更新了代码，否则无需重新构建。

### 第四步：启动服务

```powershell
.\run-local-service-windows.ps1
```

**首次启动**会进入配置向导：

```
API Key（访问此代理密钥，自定义即可，可选）: [默认sk-my-proxy-key]
Admin Password（管理后台密码(http://ip:端口/admin页面)，必填）: [默认my-admin-pass]
端口 [默认: 5678]:
Region [默认: us-east-1]:
本地 HTTP 代理端口（例如: 7890 / 10089）: [填入你的代理端口]
```

- **⚠️【重要】本地 HTTP 代理端口**：也就是开魔法的端口。注意：**本地搭建的话，不配置将无法访问 Claude 模型，如Claude4.6和Claude4.7模型**

- > ⚠️ **【重要】代理端口（国内用户必须配置）**
  >
  > 常在终端上使用的命令如：export http_proxy=http://127.0.0.1:10089; export https_proxy=http://127.0.0.1:10089;
  >
  > 这里的10089就是你开魔法的端口
  >
  > 不知道端口号请查看代理软件的设置页面

- **Admin Password**：**管理面板的登录密码(http://ip:端口/admin页面)，建议设置**

配置完成后自动生成 `app/config/config.json`，服务启动，浏览器自动打开管理面板。

**后续启动**直接读取已有配置，无需重新填写。

### 第五步：填入 Kiro 账号

服务启动后，打开管理面板 `http://127.0.0.1:5678/admin`，在账号管理页面添加从 Kiro 导出的账号。

### 停止服务

在运行服务的 PowerShell 窗口按 `Ctrl+C`，或直接关闭窗口。

---

## 服务器部署（Linux）

### 方式一：Docker（最简单，推荐）

**前置要求**：服务器已安装 Docker 和 Docker Compose。

```bash
# 1. 克隆仓库
git clone https://github.com/TsinHzl/kiro2cc-proxy.git /opt/kiro2cc-proxy
cd /opt/kiro2cc-proxy

# 2. 创建配置文件（注意：配置文件在 data/ 目录下，不是 data/config/）
mkdir -p data
cp config.example.json data/config.json
nano data/config.json   # 填入 apiKey 和 adminApiKey
```

`data/config.json` 最小配置：

```json
{
  "host": "0.0.0.0",
  "port": 5678,
  "apiKey": "sk-your-api-key",
  "region": "us-east-1",
  "adminApiKey": "your-admin-password"
}
```

> ⚠️ **【重要】`port` 字段必须是整数**，不能填写 Docker 端口映射格式（如 `"0.0.0.0:5678:5678"`），否则服务启动失败。正确写法：`"port": 5678`。

```bash
# 3. 创建账号文件（也可启动后在管理面板添加）
echo "[]" > data/credentials.json

# 4. 启动
docker compose up -d

# 查看日志
docker compose logs -f

# 停止
docker compose down
```

服务启动后访问 `http://服务器IP:5678/admin` 进入管理面板。

> **注意**：`docker-compose.yml` 中 `ports` 默认为 `"5678:5678"`，监听所有网卡。如需限制只允许本机访问，可改为 `"127.0.0.1:5678:5678"`。同时确保云服务商安全组（腾讯云/阿里云等）已开放 5678 端口的入站规则，否则外网无法访问。

### 更新到最新版本

```bash
cd /opt/kiro2cc-proxy
git pull
docker compose pull
docker compose down && docker compose up -d
```

> **说明**：每次推送新 tag（如 `v1.x.x`）后，GitHub Actions 会自动构建并推送新镜像到 `ghcr.io`。`docker compose pull` 会拉取最新的 `latest` 镜像。

### 方式二：systemd 一键安装

适合不想用 Docker、希望直接跑二进制的场景。

```bash
# 1. 克隆仓库
git clone https://github.com/TsinHzl/kiro2cc-proxy.git /opt/kiro2cc-proxy-src
cd /opt/kiro2cc-proxy-src

# 2. 创建配置文件
cp config.example.json app/config/config.json
nano app/config/config.json   # 填入 apiKey

# 3. 一键安装（自动编译 + 注册 systemd 服务）
sudo bash install_server.sh
```

安装完成后服务开机自启，常用命令：

```bash
systemctl status kiro2cc-proxy       # 查看状态
systemctl restart kiro2cc-proxy      # 重启
systemctl stop kiro2cc-proxy         # 停止
journalctl -u kiro2cc-proxy -f       # 实时日志
```

### 方式三：手动后台运行（无 systemd）

```bash
bash start_server.sh start     # 后台启动
bash start_server.sh status    # 查看状态
bash start_server.sh log       # 实时日志
bash start_server.sh stop      # 停止
bash start_server.sh restart   # 重启
```

### 服务器代理配置

国内服务器无法直接访问 Kiro API，需要在 `config.json` 中配置代理：

```json
{
  "proxyUrl": "http://your-proxy-host:port"
}
```

或使用境外服务器（推荐），无需代理。

---

## 获取 Kiro 账号

### 完整流程：从 Kiro Account Manager 导出到管理面板导入

**第一步：从 Kiro Account Manager 导出账号 JSON**

1. 安装 Kiro IDE 或 Kiro Account Manager
2. 使用 GitHub / Google 等 Social 账号登录
3. 在账号管理界面找到"导出账号信息"或"Export Account"选项
4. 导出为 JSON 文件（或复制 JSON 内容）

**第二步：启动 kiro2cc-proxy 服务**

按照[本地部署](#本地部署macos)或[服务器部署](#服务器部署linux)章节启动服务，确保服务正常运行。

**第三步：通过管理面板导入账号（推荐）**

1. 打开管理面板：`http://127.0.0.1:5678/admin`（**服务器部署则替换为对应 IP**）
2. **输入 `config.json` 中配置的 `adminApiKey`（Admin Password）登录**
3. 进入账号管理页面
4. 将导出的 JSON 内容**直接粘贴**到输入框，或将 JSON 文件**拖拽**到页面上
5. 管理面板自动识别账号信息并显示，确认后保存即可

> ⚠️ **【重要】通过 HTTP 访问管理面板时导入账号会失败**
>
> 如果你通过 `http://服务器IP:端口/admin`（非 HTTPS、非 localhost）访问管理面板，浏览器会因安全策略禁用 `crypto.subtle` 加密 API，导致导入时报错 `Cannot read properties of undefined (reading 'digest')`，且后端不会有任何错误日志。
>
> **解决方案一（推荐）：为服务器绑定域名并配置 HTTPS**，通过 `https://` 访问管理面板即可正常导入。
>
> **解决方案二（临时绕过）：强制浏览器信任该 HTTP 地址**
>
> Chrome 用户在地址栏打开：`chrome://flags/#unsafely-treat-insecure-origin-as-secure`
> Edge 用户在地址栏打开：`edge://flags/#unsafely-treat-insecure-origin-as-secure`
>
> 在 "Insecure origins treated as secure" 下方的文本框中填入你的完整地址（如 `http://43.153.11.66:8990`），将右侧开关改为 **Enabled**，点击 **Relaunch** 重启浏览器后重试。

**第四步（可选）：手动创建账号文件**

也可以跳过管理面板，直接将导出的 JSON 内容保存为文件：
- 本地部署：`app/config/credentials.json`
- Docker 部署：`data/credentials.json`

文件格式见下方说明，保存后重启服务生效。

### credentials.json 格式

**Social 登录（单账号）：**

```json
{
  "refreshToken": "your-refresh-token",
  "expiresAt": "2025-12-31T02:32:45.144Z",
  "authMethod": "social"
}
```

**IDC/Builder-ID 登录（单账号）：**

```json
{
  "refreshToken": "your-refresh-token",
  "expiresAt": "2025-12-31T02:32:45.144Z",
  "authMethod": "idc",
  "clientId": "your-client-id",
  "clientSecret": "your-client-secret"
}
```

**多账号（数组格式，支持故障转移）：**

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

`priority` 数值越小优先级越高，单账号最多重试 3 次，单请求最多重试 9 次，自动故障转移。

---

## 配置详解

### config.json 字段说明

| 字段 | 必填 | 默认值 | 说明 |
|------|------|--------|------|
| `apiKey` | **是** | — | 客户端连接时使用的 API Key，自定义即可 |
| `host` | 否 | `127.0.0.1` | 监听地址，`0.0.0.0` 允许外网/局域网访问 |
| `port` | 否 | `5678` | 监听端口 |
| `region` | 否 | `us-east-1` | AWS 区域 |
| `authRegion` | 否 | 同 `region` | Token 刷新使用的区域 |
| `apiRegion` | 否 | 同 `region` | API 请求使用的区域 |
| `adminApiKey` | 否 | — | Admin Password（管理面板登录密码），不填则不启用管理面板 |
| `proxyUrl` | 否 | — | HTTP/SOCKS5 代理，如 `http://127.0.0.1:7890` |
| `proxyUsername` | 否 | — | 代理用户名 |
| `proxyPassword` | 否 | — | 代理密码 |
| `tlsBackend` | 否 | `rustls` | TLS 后端：`rustls` 或 `native-tls` |
| `loadBalancingMode` | 否 | `priority` | `priority`（按优先级）或 `balanced`（轮询） |

> **TLS 说明**：如遇到 Token 刷新失败或请求报错，尝试将 `tlsBackend` 改为 `native-tls`。

完整配置示例：

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

### 账号级代理

可以为每个账号单独配置代理，优先级高于全局代理：

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

`proxyUrl: "direct"` 表示该账号强制直连，不走任何代理。

### Region 配置优先级

**Auth Region**（Token 刷新）：`账号.authRegion` > `账号.region` > `config.authRegion` > `config.region`

**API Region**（API 请求）：`账号.apiRegion` > `config.apiRegion` > `config.region`

---

## 接入 Claude Code

### 方式一：环境变量（推荐）

服务启动后，在终端中设置以下环境变量即可让 Claude Code 使用本代理：

```bash
export ANTHROPIC_BASE_URL="http://127.0.0.1:5678"
export ANTHROPIC_API_KEY="在管理面板 API Key 管理页面创建的 Key"
```

**永久生效**（加入 `~/.zshrc` 或 `~/.bashrc`）：

```bash
echo 'export ANTHROPIC_BASE_URL="http://127.0.0.1:5678"' >> ~/.zshrc
echo 'export ANTHROPIC_API_KEY="在管理面板 API Key 管理页面创建的 Key"' >> ~/.zshrc
source ~/.zshrc
```

### 方式二：settings.json 配置

在 Claude Code 的配置文件中直接写入代理地址，无需每次设置环境变量。

配置文件路径：
- 全局配置：`~/.claude/settings.json`
- 项目配置：`<项目根目录>/.claude/settings.json`（仅对当前项目生效）

在配置文件中添加以下内容：

```json
{
  "env": {
    "ANTHROPIC_BASE_URL": "http://127.0.0.1:5678",
    "ANTHROPIC_API_KEY": "在管理面板 API Key 管理页面创建的 Key"
  }
}
```

如果文件已有其他配置，将 `env` 字段合并进去即可：

```json
{
  "theme": "dark",
  "env": {
    "ANTHROPIC_BASE_URL": "http://127.0.0.1:5678",
    "ANTHROPIC_API_KEY": "在管理面板 API Key 管理页面创建的 Key"
  }
}
```

**验证是否生效：**

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

## API 端点

### 标准端点 (/v1)

| 端点 | 方法 | 说明 |
|------|------|------|
| `/v1/models` | GET | 获取可用模型列表 |
| `/v1/messages` | POST | 创建消息（对话） |
| `/v1/messages/count_tokens` | POST | 估算 Token 数量 |

### Claude Code 兼容端点 (/cc/v1)

| 端点 | 方法 | 说明 |
|------|------|------|
| `/cc/v1/messages` | POST | 缓冲模式，`input_tokens` 更准确 |
| `/cc/v1/messages/count_tokens` | POST | 估算 Token 数量 |

> `/cc/v1/messages` 会等待上游流完成后再返回，`input_tokens` 使用实际值而非估算值，等待期间每 25 秒发送 `ping` 保活。

### 客户端认证

支持两种方式：

```
x-api-key: your-api-key
```
或
```
Authorization: Bearer your-api-key
```

---

## 模型映射

发送请求时可使用任意包含以下关键词的模型名，会自动映射到对应 Kiro 模型：

| 请求模型名（含关键词） | 实际使用的 Kiro 模型 |
|----------------------|-------------------|
| `*sonnet*`（含 4.6/4-6） | `claude-sonnet-4.6` |
| `*sonnet*`（其他） | `claude-sonnet-4.5` |
| `*opus*`（含 4.5/4-5） | `claude-opus-4.5` |
| `*opus*`（含 4.7/4-7） | `claude-opus-4.7` |
| `*opus*`（含 4.8/4-8） | `claude-opus-4.8` |
| `*opus*`（其他） | `claude-opus-4.6` |
| `*haiku*` | `claude-haiku-4.5` |
| `*deepseek*` | `deepseek-3.2` |
| `*glm*` | `glm-5` |
| `*minimax*`（含 2.5/2-5） | `minimax-m2.5` |
| `*minimax*`（其他） | `minimax-m2.1` |
| `*qwen*` | `qwen3-coder-next` |

---

## Admin 管理面板

配置了 `adminApiKey`（Admin Password）后，访问 `http://127.0.0.1:5678/admin` 进入管理面板。

功能：
- 查看所有账号状态（是否有效、失败次数等）
- 添加 / 删除账号
- 启用 / 禁用单个账号
- 调整账号优先级
- 查看各账号余额
- 重置账号失败状态

**Admin API**（需要 `x-api-key` 或 `Authorization: Bearer` 认证）：

| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/admin/credentials` | GET | 获取所有账号 |
| `/api/admin/credentials` | POST | 添加账号 |
| `/api/admin/credentials/:id` | DELETE | 删除账号 |
| `/api/admin/credentials/:id/balance` | GET | 查询余额 |

---

## 常见问题

**Q：启动后提示"已加载 0 个账号配置"**

需要创建 `app/config/credentials.json`（本地）或 `data/credentials.json`（Docker），参考[获取 Kiro 账号](#获取-kiro-账号)章节。

**Q：请求返回 `INVALID_MODEL_ID`**

> ⚠️ **【重要】** 国内 IP 无法直接访问 Claude 模型。必须在 `app/config/config.json` 中配置 `proxyUrl`（如 `"proxyUrl": "http://127.0.0.1:7890"`），或使用境外服务器。这是国内用户最常见的问题。

**Q：请求返回 401 Unauthorized**

客户端使用的 API Key 与 `config.json` 中的 `apiKey` 不一致，检查并对齐。

**Q：Token 刷新失败 / 请求报错**

尝试将 `config.json` 中的 `tlsBackend` 改为 `native-tls` 后重启服务。

**Q：通过管理面板导入账号时报错 `Cannot read properties of undefined (reading 'digest')`**

这是浏览器的安全策略限制，`crypto.subtle` 加密 API 只在 HTTPS 或 localhost 环境下可用。通过公网 IP + HTTP 访问管理面板时会触发此错误，后端不会有任何日志。

解决方案：
- **推荐**：为服务器绑定域名并配置 HTTPS，通过 `https://` 访问管理面板
- **临时绕过**：Chrome 打开 `chrome://flags/#unsafely-treat-insecure-origin-as-secure`，Edge 打开 `edge://flags/#unsafely-treat-insecure-origin-as-secure`，在文本框填入完整地址（如 `http://43.153.11.66:8990`），改为 Enabled 后重启浏览器

**Q：端口被占用**

`run-local-service-mac.sh` 会自动终止占用端口的进程。如仍报错，手动执行：
```bash
lsof -ti:5678 | xargs kill -9
```

**Q：Write Failed / 会话卡死**

输出过长被截断导致，调低客户端的 `max_tokens` 上限。

**Q：局域网内其他设备无法访问**

将 `config.json` 中的 `host` 改为 `0.0.0.0`，确认防火墙已开放对应端口。

**Q：如何更新到最新版本（Docker 部署）**

```bash
cd /opt/kiro2cc-proxy
git pull
docker compose pull
docker compose down && docker compose up -d
```

**Q：如何更新到最新版本（本地部署）**

```bash
git pull
./build-mac.sh
./run-local-service-mac.sh
```

---

## 注意事项

1. `credentials.json` 包含敏感 Token，不要提交到版本控制，不要分享给他人
2. 服务会自动刷新过期 Token，无需手动干预
3. 多账号模式下 Token 刷新后自动回写到文件
4. 国内用户必须配置代理才能访问 Claude 模型

---

## 项目结构

```
kiro2cc-proxy/
├── src/                    # Rust 源码
├── admin-ui/               # 管理面板前端
├── user-ui/                # 用户面板前端
├── app/config/             # 本地配置目录（gitignored）
├── config.example.json     # 配置示例
├── docker-compose.yml      # Docker 部署配置
├── Dockerfile              # Docker 镜像构建
├── build-mac.sh            # 一键构建脚本（macOS）
├── build-windows.ps1       # 一键构建脚本（Windows）
├── run-local-service-mac.sh         # macOS 本地启动脚本
├── run-local-service-windows.ps1   # Windows 本地启动脚本
├── setup_shell_aliases.sh  # macOS Shell 快捷命令安装脚本
├── setup_shell_aliases.ps1 # Windows PowerShell 快捷命令安装脚本
├── install_server.sh       # Linux systemd 一键安装脚本
└── start_server.sh         # Linux 手动后台管理脚本
```

---

## License

MIT

## 致谢

本项目基于 [kiro.rs](https://github.com/hank9999/kiro.rs) 二次开发，感谢原作者的开源贡献。
