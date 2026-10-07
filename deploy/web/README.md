# Web / Docker 部署

安装 Docker 和 Compose，在仓库根目录的 `.env` 中设置：

- `NYATERM_WEB_PASSWORD`：至少 32 字符的随机登录密码。
- `NYATERM_WEB_ENCRYPTION_KEY`：Base64 编码的 32 字节随机密钥，需持久保存。

在仓库根目录执行：

```sh
docker compose -f deploy/web/docker-compose.yml up --build -d
docker compose -f deploy/web/docker-compose.yml logs --tail 200 nyaterm
docker compose -f deploy/web/docker-compose.yml down
```

访问 `http://localhost:8080/` 或 `http://服务器IP:8080/`，使用设置的密码登录。数据保存在 Docker 卷中，普通 `down` 不会删除数据。

HTTPS、子路径、备份及其他配置见 [部署指南](../../docs/web-deployment.md)。
