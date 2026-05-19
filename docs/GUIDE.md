> **注：** 本文档由 **claude-sonnet-4-6** 模型自动生成。

# kiro-rs 使用指南

## 目录

- [快速开始](#快速开始)
- [首次构建](#首次构建)
- [启动与停止](#启动与停止)
- [配置说明](#配置说明)
- [凭据配置](#凭据配置)
- [管理面板](#管理面板)
- [Claude Code 接入](#claude-code-接入)
- [常见问题](#常见问题)

---

## 快速开始

```
1. 首次使用：运行 ./build-mac.sh 构建二进制
2. 启动服务：运行 ./run-local-service-mac.command
3. 按提示输入 API Key 和 Admin Password
4. 浏览器自动打开管理面板，添加凭据即可使用
```

---

## 首次构建

运行根目录下的构建脚本：

```bash
./build-mac.sh
```

脚本会依次完成：

1. 安装并构建 `admin-ui` 前端
2. 安装并构建 `user-ui` 前端
3. 编译 Rust 二进制 `./target/release/kiro-rs`

**前置要求：**

| 工具 | 安装命令 |
|------|----------|
| Node.js / npm | `brew install node` |
| Rust / cargo | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |

构建完成后，二进制位于 `./target/release/kiro-rs`，后续无需重新构建（除非更新了代码）。

---

## 启动与停止

### 启动

```bash
./run-local-service-mac.command
```

**首次启动**会进入配置向导，以下字段必须填写（不可留空）：

```
  API Key（号池 apiKey，不要泄露给他人）:
  Admin Password（号池后台管理登录密码）:
  端口 [默认: 5678]:
  Region [默认: us-east-1]:
```

配置完成后自动生成 `config.json`，服务启动，2 秒后浏览器自动打开管理面板。

**后续启动**直接读取已有 `config.json`，无需重新配置。

### 停止

在运行服务的终端窗口按 `Ctrl+C`。

---

## 配置说明

配置文件为根目录下的 `config.json`（参考 `config.example.json`）：

```json
{
  "host": "0.0.0.0",
  "port": 5678,
  "apiKey": "你的 API Key",
  "tlsBackend": "rustls",
  "region": "us-east-1",
  "adminApiKey": "你的管理面板密码"
}
```

| 字段 | 必填 | 默认值 | 说明 |
|------|------|--------|------|
| `apiKey` | **是** | — | 号池 apiKey，客户端连接时使用，不要泄露给他人 |
| `adminApiKey` | **是** | — | Admin Password（号池后台管理面板登录密码） |
| `host` | 否 | `0.0.0.0` | 监听地址，`0.0.0.0` 允许局域网访问 |
| `port` | 否 | `5678` | 监听端口 |
| `region` | 否 | `us-east-1` | AWS 区域 |
| `authRegion` | 否 | 同 `region` | Token 刷新区域 |
| `apiRegion` | 否 | 同 `region` | API 请求区域 |
| `proxyUrl` | 否 | — | HTTP/SOCKS5 代理，如 `http://127.0.0.1:7890` |
| `loadBalancingMode` | 否 | `priority` | `priority`（按优先级）或 `balanced`（轮询） |
| `tlsBackend` | 否 | `rustls` | `rustls` 或 `native-tls` |

修改 `config.json` 后重启服务生效。若需重新配置，删除 `config.json` 后重新运行 `./run-local-service-mac.command`。

---

## 凭据配置

凭据文件为根目录下的 `credentials.json`，支持单账号和多账号两种格式。

### Social 登录账号（单账号）

```json
{
  "refreshToken": "your-refresh-token",
  "expiresAt": "2025-12-31T02:32:45.144Z",
  "authMethod": "social",
  "machineId": "64位十六进制字符串"
}
```

### IDC 账号（单账号）

```json
{
  "refreshToken": "your-refresh-token",
  "expiresAt": "2025-12-31T02:32:45.144Z",
  "authMethod": "idc",
  "clientId": "your-client-id",
  "clientSecret": "your-client-secret",
  "region": "us-east-2",
  "machineId": "64位十六进制字符串"
}
```

### 多账号（数组格式）

```json
[
  {
    "refreshToken": "token-1",
    "expiresAt": "2025-12-31T02:32:45.144Z",
    "authMethod": "social",
    "machineId": "aaa...aaa",
    "priority": 0
  },
  {
    "refreshToken": "token-2",
    "expiresAt": "2025-12-31T02:32:45.144Z",
    "authMethod": "idc",
    "clientId": "xxx",
    "clientSecret": "xxx",
    "region": "us-east-2",
    "machineId": "bbb...bbb",
    "priority": 1,
    "disabled": false
  }
]
```

`priority` 数值越小优先级越高；`disabled: true` 暂时禁用该账号。

参考示例文件：
- `credentials.example.social.json` — Social 登录格式
- `credentials.example.idc.json` — IDC 格式
- `credentials.example.multiple.json` — 多账号格式

---

## 管理面板

服务启动后访问：`http://127.0.0.1:5678/admin`

访问时需要输入 `config.json` 中配置的 `adminApiKey`（Admin Password）。

管理面板功能：
- 查看已加载的凭据列表及状态
- 启用 / 禁用单个凭据
- 调整凭据优先级
- 查看各账号余额
- 重置账号状态

---

## Claude Code 接入

`./run-local-service-mac.command` 启动时会自动将以下环境变量写入 `~/.claude_profile`：

```bash
export ANTHROPIC_BASE_URL="http://127.0.0.1:5678"
export ANTHROPIC_API_KEY="你配置的 apiKey"
```

**当前终端**已自动 `source`，无需额外操作。

**其他终端窗口**需手动执行一次：

```bash
source ~/.claude_profile
```

或将以下内容加入 `~/.zshrc`，之后新开终端自动生效：

```bash
[ -f ~/.claude_profile ] && source ~/.claude_profile
```

---

## 常见问题

**Q: 启动报错 `配置文件中未设置 apiKey`**

删除 `config.json` 重新运行 `./run-local-service-mac.command`，按向导重新配置。

**Q: 已加载 0 个凭据配置**

需要创建 `credentials.json`，参考上方凭据配置章节。

**Q: Claude Code 请求返回 401 Unauthorized**

API Key 不匹配。确认 Claude Code 所在终端已执行 `source ~/.claude_profile`，或检查 `config.json` 中的 `apiKey` 与客户端配置是否一致。

**Q: 端口被占用**

`run-local-service-mac.command` 会自动杀掉占用配置端口的进程。如仍报错，手动执行：
```bash
lsof -ti:5678 | xargs kill -9
```

**Q: 局域网内其他设备无法访问**

`host` 已默认设为 `0.0.0.0`，确认防火墙未拦截对应端口即可。

**Q: 构建时 npm install 卡住**

脚本已配置使用淘宝镜像 `registry.npmmirror.com`。如仍卡住，检查网络连接后重试。

**Q: 如何更新代码后重新构建**

```bash
git pull
./build-mac.sh
./run-local-service-mac.command
```

