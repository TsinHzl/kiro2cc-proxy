# New API 部署指南（同机 Docker）

## 说明

在已部署 kiro-rs 的 VPS 上，部署 New API 作为对外分发网关。kiro-rs 作为上游渠道，New API 负责用户管理、令牌分发和额度计费。

## 部署步骤

### 1. 创建目录并写入配置

```bash
mkdir -p ~/new-api

cat > ~/new-api/docker-compose.yml << 'EOF'
services:
  new-api:
    image: ghcr.io/dev-longshun/new-api-qns:latest
    container_name: new-api
    restart: always
    command: --log-dir /app/logs
    ports:
      - "3000:3000"
    volumes:
      - ./data:/data
      - ./logs:/app/logs
    environment:
      - SQL_DSN=postgresql://root:你的数据库密码@postgres:5432/new-api
      - REDIS_CONN_STRING=redis://redis
      - TZ=Asia/Shanghai
      - BATCH_UPDATE_ENABLED=true
    depends_on:
      - redis
      - postgres
    networks:
      - new-api-network

  redis:
    image: redis:latest
    container_name: redis
    restart: always
    networks:
      - new-api-network

  postgres:
    image: postgres:15
    container_name: postgres
    restart: always
    environment:
      POSTGRES_USER: root
      POSTGRES_PASSWORD: 你的数据库密码
      POSTGRES_DB: new-api
    volumes:
      - pg_data:/var/lib/postgresql/data
    networks:
      - new-api-network

volumes:
  pg_data:

networks:
  new-api-network:
    driver: bridge
EOF
```

两处 `你的数据库密码` 必须一致，这是内部数据库密码，不对外暴露。

### 2. 拉取并启动

```bash
cd ~/new-api
docker compose pull
docker compose up -d
```

### 3. 验证运行

```bash
curl http://localhost:3000/api/status
```

返回 `"success":true` 即为成功。

## 本地访问

与 kiro-rs 相同，通过 SSH 隧道或 Termius 端口转发访问：

- Local port: `3000`
- Destination address: `127.0.0.1`
- Destination port: `3000`

隧道建立后打开 `http://localhost:3000` 注册管理员账号。

## 对接 kiro-rs 上游渠道

在 New API 后台「渠道管理」中添加：

- 类型：Anthropic Claude
- 密钥：kiro-rs 的 apiKey
- API 地址：`http://host.docker.internal:8990`（不带 `/v1`）
- 模型：选择 kiro-rs 支持的模型

## 用户分发流程

1. 后台生成兑换码（设定额度面值）
2. 用户注册账号 → 兑换码充值 → 自行创建令牌
3. 用户使用令牌 + New API 地址调用 API，New API 自动匹配渠道

## 常用运维命令

- 查看日志：`docker compose logs -f new-api`
- 重启服务：`docker compose restart`
- 更新镜像：`docker compose pull && docker compose up -d`
- 停止服务：`docker compose down`
