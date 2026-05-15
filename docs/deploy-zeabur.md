# Zeabur 部署指南

## 镜像地址

```
ghcr.io/dev-longshun/kiro-rs-commercial:latest
```

## 部署步骤

### 1. 创建服务

Add Service → Prebuilt Image → 输入上方镜像地址

### 2. 挂载持久化卷

- Mount Directory：`/app/config`
- 作用：保存凭据数据，防止重启丢失
- **注意**：`credentials.json` 不要添加到 Config File，应用需要运行时写入该文件

### 3. 添加配置文件

在 Config File 中添加 `/app/config/config.json`，内容如下：

```json
{
  "apiKey": "你的API密钥",
  "host": "0.0.0.0",
  "port": 8990,
  "adminApiKey": "你的管理后台密钥"
}
```

### 4. 开放网络端口

在 Networking 中开放端口 `8990`

### 不需要配置的项

- **环境变量** — Config File 已提供配置，无需设置
- **启动命令（Command）** — 镜像内置，无需填写
- **Dockerfile** — 预构建镜像部署不适用

## 版本标签

- `latest` — 打 `v*` tag 时更新（正式版本）
- `beta` — 每次推送到 `main` 分支时更新

## 常见问题

### 凭据重启后丢失

确保 `credentials.json` **没有**被添加到 Config File 中。Zeabur 的 Config File 会以只读方式挂载，覆盖持久化卷上的同名文件，导致应用无法写入凭据数据。详见 [排查记录](troubleshooting/zeabur-credentials-readonly-mount.md)。

### 镜像显示损坏

确认使用的标签存在。首次部署前需要先打一个 `v*` tag 触发构建才会生成 `latest` 标签。


502: SERVICE_UNAVAILABLE

https://kirofds1.zeabur.app/admin