# kiro2cc-proxy

一个用 Rust 编写的 Anthropic Claude API 兼容代理服务，将 Anthropic API 请求转换为 Kiro API 请求。

[English](README.en.md) | 中文

## 免责声明

本项目仅供研究使用，Use at your own risk，使用本项目所导致的任何后果由使用人承担，与本项目无关。本项目与 AWS/KIRO/Anthropic/Claude 等官方无关，不代表官方立场。

## 功能特性

- **Anthropic API 兼容**：完整支持 Anthropic Claude API 格式
- **流式响应**：支持 SSE (Server-Sent Events) 流式输出
- **Token 自动刷新**：自动管理和刷新 OAuth Token
- **多凭据支持**：支持配置多个凭据，按优先级自动故障转移
- **负载均衡**：支持 `priority`（按优先级）和 `balanced`（均衡分配）两种模式
- **智能重试**：单凭据最多重试 3 次，单请求最多重试 9 次
- **Thinking 模式**：支持 Claude 的 extended thinking 功能
- **工具调用**：完整支持 function calling / tool use
- **WebSearch**：内置 WebSearch 工具转换逻辑
- **Admin 管理**：可选的 Web 管理界面，支持凭据管理、余额查询等
- **凭据级代理**：支持为每个凭据单独配置 HTTP/SOCKS5 代理

---

## 目录

- [快速开始（新手必读）](#快速开始新手必读)
- [本地部署（macOS）](#本地部署macos)
- [服务器部署（Linux）](#服务器部署linux)
- [获取 Kiro 凭据](#获取-kiro-凭据)
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

kiro-rs 是一个本地代理服务。它把标准的 Anthropic Claude API 请求转发给 Kiro（AWS 的 AI 编程工具），让你可以用 Kiro 账号免费使用 Claude 模型。

**使用前提：**

1. 拥有一个 Kiro 账号（通过 [kiro.dev](https://kiro.dev) 注册，支持 Social 登录）
2. 从 Kiro IDE 或账号管理工具中导出凭据（`refreshToken` 等信息）
3. **国内用户**：需要配置 HTTP/SOCKS5 代理，否则无法访问 Claude 模型

**整体流程：**

```
安装依赖 → 构建项目 → 启动服务 → 填入凭据 → 配置客户端
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
git clone <仓库地址>
cd kiro-rs
```

### 第三步：构建项目

```bash
./build.sh
```

脚本会依次构建 admin-ui 前端、user-ui 前端，最后编译 Rust 二进制。首次构建约需 5~15 分钟。

构建成功后输出：
```
  构建成功！
  二进制位置: ./target/release/kiro-rs
```

> 后续除非更新了代码，否则无需重新构建。

### 第四步：启动服务

**方式一：双击启动（推荐）**

在 Finder 中找到项目目录，双击 `start.command` 文件。

**方式二：终端启动**

```bash
./start.command
```

**首次启动**会进入配置向导：

```
  API Key（访问此代理的密钥，自定义即可）: sk-my-proxy-key
  Admin API Key（管理后台密码，直接回车跳过）: my-admin-pass
  端口 [默认: 8990]:
  Region [默认: us-east-1]:
  本地 HTTP 代理端口（直接回车跳过，例如: 7890 / 10089）: 7890
```

- **API Key**：自己随便设一个，客户端连接时用这个 Key 认证
- **Admin API Key**：管理面板的登录密码，建议设置
- **代理端口**：国内用户必须填写，填本地代理软件（Clash/V2Ray 等）的端口

配置完成后自动生成 `app/config/config.json`，服务启动，浏览器自动打开管理面板。

**后续启动**直接读取已有配置，无需重新填写。

### 第五步：填入 Kiro 凭据

服务启动后，打开管理面板 `http://127.0.0.1:8990/admin`，在凭据管理页面添加从 Kiro 导出的凭据。

也可以直接创建 `app/config/credentials.json`，格式见[获取 Kiro 凭据](#获取-kiro-凭据)章节。

### 停止服务

在运行服务的终端窗口按 `Ctrl+C`，或直接关闭终端窗口。

---

## 服务器部署（Linux）

### 方式一：Docker（最简单，推荐）

**前置要求**：服务器已安装 Docker 和 Docker Compose。

```bash
# 1. 克隆仓库
git clone <仓库地址> /opt/kiro-rs
cd /opt/kiro-rs

# 2. 创建数据目录和配置文件
mkdir -p data
cp config.example.json data/config.json
nano data/config.json   # 填入 apiKey 和 adminApiKey
```

`data/config.json` 最小配置：

```json
{
  "host": "0.0.0.0",
  "port": 8990,
  "apiKey": "sk-your-api-key",
  "region": "us-east-1",
  "adminApiKey": "your-admin-password"
}
```

```bash
# 3. 创建凭据文件（也可启动后在管理面板添加）
# 参考下方"获取 Kiro 凭据"章节
nano data/credentials.json

# 4. 启动
docker compose up -d

# 查看日志
docker compose logs -f

# 停止
docker compose down
```

服务启动后访问 `http://服务器IP:8990/admin` 进入管理面板。

> **注意**：Docker Compose 默认只监听 `127.0.0.1:8990`，如需外网访问，修改 `docker-compose.yml` 中的 `ports` 为 `"0.0.0.0:8990:8990"`，并确保防火墙已开放该端口。

### 方式二：systemd 一键安装

适合不想用 Docker、希望直接跑二进制的场景。

```bash
# 1. 克隆仓库
git clone <仓库地址> /opt/kiro-rs-src
cd /opt/kiro-rs-src

# 2. 创建配置文件
cp config.example.json app/config/config.json
nano app/config/config.json   # 填入 apiKey

# 3. 一键安装（自动编译 + 注册 systemd 服务）
sudo bash install_server.sh
```

安装完成后服务开机自启，常用命令：

```bash
systemctl status kiro-rs       # 查看状态
systemctl restart kiro-rs      # 重启
systemctl stop kiro-rs         # 停止
journalctl -u kiro-rs -f       # 实时日志
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

## 获取 Kiro 凭据

### 通过 Kiro Account Manager 导出

1. 安装并登录 Kiro IDE 或 Kiro Account Manager
2. 登录你的 Kiro 账号（支持 GitHub/Google 等 Social 登录）
3. 导出账号信息为 JSON 格式
4. 将导出的内容保存为 `app/config/credentials.json`（本地）或 `data/credentials.json`（Docker）

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

`priority` 数值越小优先级越高，单凭据最多重试 3 次，单请求最多重试 9 次，自动故障转移。

---

## 配置详解

### config.json 字段说明

| 字段 | 必填 | 默认值 | 说明 |
|------|------|--------|------|
| `apiKey` | **是** | — | 客户端连接时使用的 API Key，自定义即可 |
| `host` | 否 | `127.0.0.1` | 监听地址，`0.0.0.0` 允许外网/局域网访问 |
| `port` | 否 | `8990` | 监听端口 |
| `region` | 否 | `us-east-1` | AWS 区域 |
| `authRegion` | 否 | 同 `region` | Token 刷新使用的区域 |
| `apiRegion` | 否 | 同 `region` | API 请求使用的区域 |
| `adminApiKey` | 否 | — | 管理面板登录密码，不填则不启用管理面板 |
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
  "port": 8990,
  "apiKey": "sk-my-proxy-key",
  "region": "us-east-1",
  "adminApiKey": "my-admin-password",
  "proxyUrl": "http://127.0.0.1:7890",
  "tlsBackend": "rustls",
  "loadBalancingMode": "priority"
}
```

### 凭据级代理

可以为每个凭据单独配置代理，优先级高于全局代理：

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

`proxyUrl: "direct"` 表示该凭据强制直连，不走任何代理。

### Region 配置优先级

**Auth Region**（Token 刷新）：`凭据.authRegion` > `凭据.region` > `config.authRegion` > `config.region`

**API Region**（API 请求）：`凭据.apiRegion` > `config.apiRegion` > `config.region`

---

## 接入 Claude Code

服务启动后，在终端中设置以下环境变量即可让 Claude Code 使用本代理：

```bash
export ANTHROPIC_BASE_URL="http://127.0.0.1:8990"
export ANTHROPIC_API_KEY="你在 config.json 中设置的 apiKey"
```

**永久生效**（加入 `~/.zshrc` 或 `~/.bashrc`）：

```bash
echo 'export ANTHROPIC_BASE_URL="http://127.0.0.1:8990"' >> ~/.zshrc
echo 'export ANTHROPIC_API_KEY="your-api-key"' >> ~/.zshrc
source ~/.zshrc
```

**验证是否生效：**

```bash
curl http://127.0.0.1:8990/v1/messages \
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
| `*sonnet*` | `claude-sonnet-4.5` |
| `*opus*`（含 4.5/4-5） | `claude-opus-4.5` |
| `*opus*`（其他） | `claude-opus-4.6` |
| `*haiku*` | `claude-haiku-4.5` |

---

## Admin 管理面板

配置了 `adminApiKey` 后，访问 `http://127.0.0.1:8990/admin` 进入管理面板。

功能：
- 查看所有凭据状态（是否有效、失败次数等）
- 添加 / 删除凭据
- 启用 / 禁用单个凭据
- 调整凭据优先级
- 查看各账号余额
- 重置账号失败状态

**Admin API**（需要 `x-api-key` 或 `Authorization: Bearer` 认证）：

| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/admin/credentials` | GET | 获取所有凭据 |
| `/api/admin/credentials` | POST | 添加凭据 |
| `/api/admin/credentials/:id` | DELETE | 删除凭据 |
| `/api/admin/credentials/:id/balance` | GET | 查询余额 |

---

## 常见问题

**Q：启动后提示"已加载 0 个凭据配置"**

需要创建 `app/config/credentials.json`（本地）或 `data/credentials.json`（Docker），参考[获取 Kiro 凭据](#获取-kiro-凭据)章节。

**Q：请求返回 `INVALID_MODEL_ID`**

国内 IP 无法访问 Claude 模型，需要在 `config.json` 中配置 `proxyUrl`，或使用境外服务器。

**Q：请求返回 401 Unauthorized**

客户端使用的 API Key 与 `config.json` 中的 `apiKey` 不一致，检查并对齐。

**Q：Token 刷新失败 / 请求报错**

尝试将 `config.json` 中的 `tlsBackend` 改为 `native-tls` 后重启服务。

**Q：端口被占用**

`start.command` 会自动终止占用端口的进程。如仍报错，手动执行：
```bash
lsof -ti:8990 | xargs kill -9
```

**Q：Write Failed / 会话卡死**

输出过长被截断导致，调低客户端的 `max_tokens` 上限。

**Q：局域网内其他设备无法访问**

将 `config.json` 中的 `host` 改为 `0.0.0.0`，确认防火墙已开放对应端口。

**Q：如何更新到最新版本**

```bash
git pull
./build.sh
./start.command
```

---

## 注意事项

1. `credentials.json` 包含敏感 Token，不要提交到版本控制，不要分享给他人
2. 服务会自动刷新过期 Token，无需手动干预
3. 多凭据模式下 Token 刷新后自动回写到文件
4. 国内用户必须配置代理才能访问 Claude 模型

---

## 项目结构

```
kiro-rs/
├── src/                    # Rust 源码
├── admin-ui/               # 管理面板前端
├── user-ui/                # 用户面板前端
├── app/config/             # 本地配置目录（gitignored）
├── config.example.json     # 配置示例
├── docker-compose.yml      # Docker 部署配置
├── Dockerfile              # Docker 镜像构建
├── build.sh                # 一键构建脚本（macOS/Linux）
├── start.command           # macOS 本地启动脚本
├── install_server.sh       # Linux systemd 一键安装脚本
└── start_server.sh         # Linux 手动后台管理脚本
```

---

## License

MIT

## 致谢

本项目基于 [kiro.rs](https://github.com/hank9999/kiro.rs) 二次开发，感谢原作者的开源贡献。
