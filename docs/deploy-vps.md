# VPS 部署指南（Docker）

## 镜像地址

```
ghcr.io/dev-longshun/kiro-rs-commercial:latest
```

## 前置要求

- Debian 12 或其他 Linux 发行版
- Docker 已安装

安装 Docker（如未安装）：

```bash
curl -fsSL https://get.docker.com | sh
```

## 部署步骤

### 1. 创建项目目录

```bash
mkdir -p ~/kiro-rs/data
```

### 2. 创建 docker-compose.yml

```bash
cat > ~/kiro-rs/docker-compose.yml << 'EOF'
services:
  kiro-rs:
    image: ghcr.io/dev-longshun/kiro-rs-commercial:latest
    container_name: kiro-rs
    extra_hosts:
      - "host.docker.internal:host-gateway"
    ports:
      - "127.0.0.1:8990:8990"
    volumes:
      - ./data:/app/config
    restart: unless-stopped
EOF
```

端口绑定 `127.0.0.1`，仅本机可访问。如需作为 New API 上游渠道，同机部署时填 `http://127.0.0.1:8990` 即可。

### 3. 创建配置文件

```bash
cat > ~/kiro-rs/data/config.json << 'EOF'
{
  "apiKey": "你的API密钥",
  "host": "0.0.0.0",
  "port": 8990,
  "adminApiKey": "你的管理后台密钥"
}
EOF
```

### 4. 拉取并启动

```bash
cd ~/kiro-rs
docker compose pull
docker compose up -d
```

### 5. 验证运行

```bash
docker compose logs -f
```

看到 `启动 Anthropic API 端点: 0.0.0.0:8990` 即为成功。

## 本地访问管理后台

端口绑定为 `127.0.0.1`，外部无法直接访问。通过 SSH 隧道将远程端口映射到本地，即可在浏览器中操作管理后台（包括添加凭据、复制等需要剪贴板的操作）。

### 方式一：命令行 SSH 隧道

```bash
ssh -L 8990:127.0.0.1:8990 -i /path/to/your/private-key root@服务器IP
```

### 方式二：Termius 端口转发

1. 左侧菜单进入 Port Forwarding
2. 新建规则，填写：
   - Local port number: `8990`
   - Bind address: `127.0.0.1`
   - Intermediate host: 选择对应服务器
   - Destination address: `127.0.0.1`
   - Destination port number: `8990`
3. 双击规则启用

隧道建立后，本地浏览器打开 `http://localhost:8990/admin` 即可访问管理后台。

## 版本标签

- `latest` — 打 `v*` tag 时更新（正式版本）
- `beta` — 每次推送到 `main` 分支时更新

## 常用运维命令

- 查看日志：`docker compose logs -f`
- 重启服务：`docker compose restart`
- 更新镜像：`docker compose pull && docker compose up -d`
- 停止服务：`docker compose down`

---

## 多实例分流部署（可选）

高并发场景下（~50 个同时在飞的流式连接），上游对同一出口 IP 的并发连接有隐性限制。通过多实例 + 不同代理 IP 分散出口，降低每个 IP 的并发压力。

### 架构

```
用户 → New API (:3000) → kiro-rs-1 (:8990, 直连)
                        → kiro-rs-2 (:8991, 代理 IP-A)
                        → kiro-rs-3 (:8992, 代理 IP-B)
                        → kiro-rs-4 (:8993, 代理 IP-C)
```

New API 配 4 个渠道，自动负载均衡。50 并发分散到 4 个 IP，每个 ~12 个。

### 1. 修改 docker-compose.yml

将单实例配置替换为多实例。以 4 实例为例：

```yaml
services:
  kiro-rs-1:
    image: ghcr.io/dev-longshun/kiro-rs-commercial:latest
    container_name: kiro-rs-1
    extra_hosts:
      - "host.docker.internal:host-gateway"
    ports:
      - "127.0.0.1:8990:8990"
    volumes:
      - ./data:/app/config
    restart: unless-stopped

  kiro-rs-2:
    image: ghcr.io/dev-longshun/kiro-rs-commercial:latest
    container_name: kiro-rs-2
    extra_hosts:
      - "host.docker.internal:host-gateway"
    ports:
      - "127.0.0.1:8991:8990"
    volumes:
      - ./data-2:/app/config
    environment:
      - PROXY_URL=socks5://代理IP-A:端口
      - PROXY_USERNAME=用户名
      - PROXY_PASSWORD=密码
    restart: unless-stopped

  # kiro-rs-3, kiro-rs-4 同理，端口递增 8992, 8993
```

说明：
- kiro-rs-1 保持直连（无代理），使用原有 `./data` 目录
- kiro-rs-2/3/4 通过环境变量注入代理配置，会覆盖 config.json 中的值
- 每个实例需要独立的 data 目录（运行时会写入 token 缓存等）

### 2. 创建各实例配置目录

```bash
cd ~/kiro-rs
for i in 2 3 4; do
  cp -r data "data-$i"
done
```

### 3. 启动并验证

```bash
docker compose up -d
docker compose logs -f
```

每个实例应显示 `启动 Anthropic API 端点: 0.0.0.0:8990`，带代理的实例还会显示 `已配置 HTTP 代理: socks5://...`。

### 4. New API 添加渠道

在 New API 后台「渠道管理」中为每个新实例添加渠道：

- 渠道 2：API 地址 `http://host.docker.internal:8991`
- 渠道 3：API 地址 `http://host.docker.internal:8992`
- 渠道 4：API 地址 `http://host.docker.internal:8993`

类型、密钥、模型选择与原渠道一致。

### 注意事项

- 4 个实例共享同一批号，但 429 冷却状态各自独立
- Admin UI 只需在 kiro-rs-1 (:8990) 上管理
- 某个代理 IP 不可用时，New API 会自动将流量分配到其他渠道
- 回撤：`docker compose down` 后恢复单实例 docker-compose.yml 即可
