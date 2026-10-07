# Web / Docker 部署

安装 Docker 和 Compose，在仓库根目录的 `.env` 中设置：

- `NYATERM_WEB_PASSWORD`：至少 32 字符的随机登录密码。
- `NYATERM_WEB_ENCRYPTION_KEY`：Base64 编码的 32 字节随机密钥，需持久保存。

## 使用发布镜像

官方镜像 `ghcr.io/nyakang/nyaterm-web` 支持 `linux/amd64` 和 `linux/arm64`。`latest` 指向稳定版本，只由更高版本更新；生产部署可在 `.env` 中通过 `NYATERM_WEB_IMAGE` 固定完整版本或 digest：

```dotenv
NYATERM_WEB_IMAGE=ghcr.io/nyakang/nyaterm-web:1.2.12
# 或 ghcr.io/nyakang/nyaterm-web@sha256:<manifest-digest>
```

版本示例需替换为实际已发布的版本。在仓库根目录执行：

```sh
docker compose -f deploy/web/docker-compose.image.yml pull
docker compose -f deploy/web/docker-compose.image.yml up -d
docker compose -f deploy/web/docker-compose.image.yml logs --tail 200 nyaterm
docker compose -f deploy/web/docker-compose.image.yml down
```

更新时先修改 `NYATERM_WEB_IMAGE`（使用 `latest` 时无需修改），再运行 `pull` 和 `up -d`。回滚时改回原版本或 digest 后执行同样命令。更新前停服备份数据目录并保存原密钥；如果新版本迁移了数据格式，回滚还需恢复对应备份。

镜像配置与源码配置使用同一项目名 `nyaterm`、服务名和数据卷。切换时单独使用一个 Compose 文件，并沿用原项目名、登录密码和加密密钥；不要运行 `down -v`。若原部署使用 `-p` 或 `COMPOSE_PROJECT_NAME`，切换后仍需使用原值。

官方镜像只支持根路径 `/`。部署到 `/nyaterm/` 等子路径时，使用下面的源码构建配置。

## 从源码构建

在仓库根目录执行：

```sh
docker compose -f deploy/web/docker-compose.yml up --build -d
docker compose -f deploy/web/docker-compose.yml logs --tail 200 nyaterm
docker compose -f deploy/web/docker-compose.yml down
```

访问 `http://localhost:8080/` 或 `http://服务器IP:8080/`，使用设置的密码登录。数据保存在 Docker 卷中，普通 `down` 不会删除数据。

两种 Compose 配置的 `8080:8080` 均发布到所有主机接口；使用本机反向代理时可改为 `127.0.0.1:8080:8080`。

## 镜像发版

[Build and Release Web](../../.github/workflows/build-web-release.yml) 在推送 `v<SemVer>` 标签时自动构建，也可在 Actions 中选择版本标签手动补发。标签版本必须与 `package.json` 一致，且提交属于 `main`。镜像发版与桌面发版独立，不等待 GitHub Release 公开。

测试 workflow 时，在 **Actions → Build and Release Web → Run workflow** 中选择 `main`。此模式运行完整部署 E2E、amd64 / arm64 构建及容器冒烟测试，不登录 GHCR、不发布镜像，也不更新 `latest`；结果显示在 Actions 摘要中。手动选择版本标签仍执行正常发版。Workflow 需先合入默认分支，才会出现在手动运行列表中。

完整版本标签不带 `v`，如 `1.2.12`、`1.3.0-beta.1`。SemVer 构建元数据中的 `+` 在镜像标签中编码为 `_`，OCI 版本标签仍保留原始版本。预发布、旧版本及同版本补发均不会更新 `latest`，也不发布 major/minor 浮动标签。版本和架构 digest 可在 Actions 摘要中查看。

首次发布后，维护者需在 GitHub 的 `nyaterm-web` package 设置中确认可见性为 **Public**，以支持匿名拉取。Workflow 使用仓库的 `GITHUB_TOKEN` 发布，无需额外注册表密码；仓库及组织策略须允许 GitHub Actions 写入该 package。

HTTPS、子路径、备份及其他配置见 [部署指南](../../docs/web-deployment.md)。
