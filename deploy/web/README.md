# Web / Docker 部署入口

此目录提供 Web Beta 的镜像构建与 Compose 配置。完整的环境变量、秘密生成、HTTPS / 子路径代理、备份和能力范围见 [部署指南](../../docs/web-deployment.md)。

## 文件与执行目录

- `Dockerfile`：Node、Rust、Debian 三阶段构建，生成前端资源与 `nyaterm-web` 服务。
- `docker-compose.yml`：服务运行配置与持久数据卷。
- 仓库根目录 `.dockerignore`：排除依赖、构建产物、秘密与本地数据。

以下命令均在仓库根目录执行。Dockerfile 的位置与构建上下文不同：上下文必须是仓库根目录，才能复制前端和 Rust 源码。

## Compose 启动

安装 Docker Engine / Docker Desktop 和 Compose V2。提前在当前 shell 或仓库根目录的受限 `.env` 中设置：

- `NYATERM_WEB_PASSWORD`：至少 32 字符的随机登录密码。
- `NYATERM_WEB_ENCRYPTION_KEY`：标准 Base64 编码的 32 字节随机密钥，需持久保存。
- `NYATERM_WEB_PUBLIC_URL`：浏览器实际访问地址，默认 `http://localhost:8080/`；非回环地址默认要求 HTTPS。

```sh
docker compose -f deploy/web/docker-compose.yml up --build -d
docker compose -f deploy/web/docker-compose.yml logs --tail 200 nyaterm
docker compose -f deploy/web/docker-compose.yml down
```

使用外部环境文件时，在 Compose 命令中添加 `--env-file /absolute/private/nyaterm.env`。生产环境推荐 `secrets` 挂载与 `_FILE` 变量，操作示例见部署指南。

默认项目名为 `nyaterm`，持久数据卷为 `nyaterm_nyaterm-data`。从旧版根目录 Compose 迁移时，如果原项目名不是 `nyaterm`，继续使用 `-p <原项目名>` 或 `COMPOSE_PROJECT_NAME`，并沿用原登录密码和加密密钥。普通 `down` 保留数据卷；`down -v` 会删除数据卷。

## 手工构建镜像

```sh
docker build -f deploy/web/Dockerfile -t nyaterm-web .
```

子路径部署示例：

```sh
docker build -f deploy/web/Dockerfile --build-arg NYATERM_WEB_BASE_PATH=/nyaterm/ -t nyaterm-web .
```

运行时的 `NYATERM_WEB_PUBLIC_URL` 路径须与构建路径一致。单独运行容器、挂载秘密和配置反向代理见部署指南。

## 部署验收

在已安装项目依赖及 Playwright Chromium 的 Docker 环境执行：

```sh
pnpm exec playwright install chromium
pnpm test:web:e2e
```

验收脚本分别构建根路径与 `/nyaterm/` 镜像，通过临时 OpenSSH 容器验证浏览器功能，失败产物写入 `artifacts/web-e2e/`。
