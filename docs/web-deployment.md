# Web / Docker 部署

Web 模式复用现有 React 界面，由 `nyaterm-web` 同时提供静态资源和 HTTP / WebSocket / SSE API。此 Beta 面向一个可信管理员：不同登录的终端、认证提示和运行中的 AI 请求隔离，但保存的连接、凭据、设置、known_hosts 和 AI 历史在同一实例内共享。

## 本地运行

需要 Node 24、pnpm 11.10 和 Rust 1.97.1（与 Dockerfile 一致）。

```powershell
pnpm install --frozen-lockfile
pnpm build:web
pnpm web:build:server
```

生成两个独立随机秘密并写入工作区外的受限目录。以下命令不打印秘密；登录密码可通过密码管理器读取保存。目录和文件的访问权限应限制为服务运行用户。

```powershell
$secretDir = Join-Path $env:LOCALAPPDATA 'NyaTermWebSecrets'
New-Item -ItemType Directory -Force -Path $secretDir | Out-Null
$loginBytes = [System.Security.Cryptography.RandomNumberGenerator]::GetBytes(32)
$keyBytes = [System.Security.Cryptography.RandomNumberGenerator]::GetBytes(32)
[System.IO.File]::WriteAllText((Join-Path $secretDir 'login'), [Convert]::ToBase64String($loginBytes))
[System.IO.File]::WriteAllText((Join-Path $secretDir 'encryption'), [Convert]::ToBase64String($keyBytes))
$env:NYATERM_WEB_PASSWORD_FILE = Join-Path $secretDir 'login'
$env:NYATERM_WEB_ENCRYPTION_KEY_FILE = Join-Path $secretDir 'encryption'
$env:NYATERM_WEB_BIND = '127.0.0.1:8080'
$env:NYATERM_WEB_DATA_DIR = Join-Path $env:LOCALAPPDATA 'NyaTermWebData'
pnpm web:serve
```

访问 `http://localhost:8080/`，使用生成的登录密码。PowerShell 示例要求 PowerShell 7 / .NET 的 `GetBytes(int)` API。Linux 可用 `umask 077` 后分别执行 `openssl rand -base64 32 > login` 和 `openssl rand -base64 32 > encryption`，然后配置相应 `_FILE` 变量。

## 环境变量

| 变量                                                             | 含义                                                                                                   |
| ---------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `NYATERM_WEB_PASSWORD` / `NYATERM_WEB_PASSWORD_FILE`             | 登录密码，至少 32 字符；推荐随机生成。指定 `_FILE` 时优先读取文件。                                    |
| `NYATERM_WEB_ENCRYPTION_KEY` / `NYATERM_WEB_ENCRYPTION_KEY_FILE` | 标准 Base64 编码的 32 字节随机密钥，必须持久保存，禁止重新生成后直接复用旧数据。                       |
| `NYATERM_WEB_BIND`                                               | 服务监听地址，默认 `127.0.0.1:8080`，容器为 `0.0.0.0:8080`。                                           |
| `NYATERM_WEB_DATA_DIR`                                           | redb 数据目录，默认 `./nyaterm-web-data`，容器为 `/data`。                                             |
| `NYATERM_WEB_DIST`                                               | 已构建 React 资源目录，默认 `dist`，容器为 `/app/dist`。                                               |
| `NYATERM_WEB_BASE_PATH`                                          | 前端构建及服务运行时使用的子路径，默认 `/`，两者必须一致并以 `/` 结尾；Docker 构建参数会同时设置两者。 |

HTTP 和 HTTPS 地址均可直接使用，无需配置访问 URL 或域名白名单。容器默认监听 `0.0.0.0:8080`，发布端口后可通过任意主机 IP 或域名访问；本地运行如需局域网访问，将监听地址设为 `0.0.0.0:8080`。前端请求 ID 使用 `crypto.getRandomValues` 兼容 HTTP，但剪贴板等浏览器能力仍可能要求 HTTPS。API 和 WebSocket 按当前请求的 Host 校验浏览器 Origin，并保留登录和 CSRF 校验。

服务启动会验证数据密钥；错误密钥会阻止启动。文件方式会去掉末尾换行。登录密码可独立替换，服务器重启使所有旧登录失效；加密密钥目前没有在线轮换 API。

## Docker

部署文件集中在 [deploy/web/](../deploy/web/README.md)。以下 Docker 和 Compose 命令均在仓库根目录执行；构建上下文仍是仓库根目录，使用根目录的 `.dockerignore`。

### 使用发布镜像

官方镜像 `ghcr.io/nyakang/nyaterm-web` 包含前端和 Rust 服务，支持 `linux/amd64` 和 `linux/arm64`，无需本机编译。先设置下面 Compose 段落要求的登录密码和持久加密密钥，再执行：

```sh
docker compose -f deploy/web/docker-compose.image.yml pull
docker compose -f deploy/web/docker-compose.image.yml up -d
```

默认使用 `latest`；可在 `.env` 中设置 `NYATERM_WEB_IMAGE=ghcr.io/nyakang/nyaterm-web:1.2.12`（替换为实际已发布版本），或指定 `ghcr.io/nyakang/nyaterm-web@sha256:<manifest-digest>`。更新或回滚时修改镜像引用，再执行 `pull` 和 `up -d`，保持原登录密码、加密密钥、项目名及数据卷；更新前停服备份数据，数据格式迁移后的回滚需恢复对应备份。源码配置与镜像配置单独使用，不叠加。

`latest` 仅由更高稳定版本更新；预发布及旧版本补发不会使它回退。首次发布后维护者需确认 GHCR package 为 Public，才能匿名拉取。发版触发条件、版本标签规则和补发入口见 [镜像发版说明](../deploy/web/README.md#镜像发版)。

官方镜像构建路径固定为 `/`，运行时不能仅修改 `NYATERM_WEB_BASE_PATH` 来改为子路径；子路径部署需使用原源码构建方式。镜像 Compose 沿用原来的 `8080:8080`，使用本机反向代理时应改为 `127.0.0.1:8080:8080`。

### 从源码构建

```sh
docker build -f deploy/web/Dockerfile -t nyaterm-web .
docker volume create nyaterm-data
docker run -d --name nyaterm-web --init \
  -p 127.0.0.1:8080:8080 \
  -e NYATERM_WEB_PASSWORD_FILE=/run/secrets/login \
  -e NYATERM_WEB_ENCRYPTION_KEY_FILE=/run/secrets/encryption \
  --mount type=bind,src=/absolute/private/secrets,dst=/run/secrets,readonly \
  --mount type=volume,src=nyaterm-data,dst=/data \
  --read-only --tmpfs /tmp --cap-drop ALL \
  --security-opt no-new-privileges:true \
  nyaterm-web
```

容器使用 UID / GID `10001:10001`。绑定目录时，确保该用户可以读取秘密文件和写入数据目录；新 named volume 会继承镜像中 `/data` 的所有权。默认端口映射仅暴露本机。如需远程访问，可发布到所需主机接口，或使用下面的 HTTPS 反向代理配置。

仓库还提供 `deploy/web/docker-compose.yml`，它通过外部环境变量注入密码和密钥：

```sh
docker compose -f deploy/web/docker-compose.yml up --build -d
```

提前在当前 shell 或仓库根目录的受限 `.env` 中设置两个必填秘密。使用外部环境文件时，在 Compose 命令中显式添加 `--env-file /absolute/private/nyaterm.env`。`.env*` 已排除在 Git 和 Docker build context 外。生产环境优先将 Compose 改为 `secrets` 挂载和 `_FILE` 变量，避免把秘密写入镜像。Dockerfile 使用 Node、Rust、Debian 三阶段构建，最终镜像只含 dist、服务程序和运行依赖。

Compose 默认项目名显式设为 `nyaterm`，与原先在名为 `nyaterm` 的仓库根目录运行的默认项目名一致，数据卷仍为 `nyaterm_nyaterm-data`。如果旧部署使用其他仓库目录名、`-p` 或 `COMPOSE_PROJECT_NAME`，迁移后须通过 `-p <原项目名>` 或 `COMPOSE_PROJECT_NAME` 沿用原值，确保继续使用原数据卷和镜像；原登录密码和加密密钥也须保持一致。

仓库 Compose 的 `8080:8080` 会发布到所有主机接口。仅本机访问或使用本机反向代理时，将它改为 `127.0.0.1:8080:8080`。

使用文件秘密时，可把下面的独立配置保存为工作区外的 `/absolute/private/compose.secrets.yml`。先按上面的命令构建 `nyaterm-web` 镜像，创建两个独立秘密文件，并确保 UID 10001 可以读取它们。此配置替代仓库 Compose，不与它叠加，避免原配置中必填的明文环境变量插值。

```yaml
name: nyaterm
services:
  nyaterm:
    image: nyaterm-web
    ports:
      - "127.0.0.1:8080:8080"
    environment:
      NYATERM_WEB_PASSWORD_FILE: /run/secrets/login
      NYATERM_WEB_ENCRYPTION_KEY_FILE: /run/secrets/encryption
    secrets:
      - login
      - encryption
    volumes:
      - nyaterm-data:/data
    restart: unless-stopped
    init: true
    read_only: true
    tmpfs:
      - /tmp
    security_opt:
      - no-new-privileges:true
    cap_drop:
      - ALL
secrets:
  login:
    file: /absolute/private/login
  encryption:
    file: /absolute/private/encryption
volumes:
  nyaterm-data:
```

```sh
docker compose -f /absolute/private/compose.secrets.yml up -d
```

如需 HTTPS，可配置下面的反向代理，无需在服务中指定访问域名。Compose 文件秘密以只读挂载提供；宿主机文件权限仍决定容器能否读取，不能依赖 Compose 的 `uid` / `gid` 为绑定文件改属主。

## HTTPS 和子路径

例如部署到 `https://terminal.example.com/nyaterm/`：

```sh
docker build -f deploy/web/Dockerfile --build-arg NYATERM_WEB_BASE_PATH=/nyaterm/ -t nyaterm-web .
```

Docker 镜像会沿用构建时的 `NYATERM_WEB_BASE_PATH`；直接运行服务时也设置 `NYATERM_WEB_BASE_PATH=/nyaterm/`。代理应保留浏览器使用的 Host 和完整路径，不能剥掉 `/nyaterm`。访问域名和端口无需在服务中配置。

使用仓库 Compose 时，设置 `NYATERM_WEB_BASE_PATH=/nyaterm/`，再执行 `up --build -d`。登录 Cookie 根据浏览器请求的 Origin 自动添加 Secure；TLS 可在代理终止，后端仍使用内部 HTTP。

Nginx 示例（TLS 证书配置按已有站点设置）：

```nginx
# http 块
map $http_upgrade $nyaterm_connection_upgrade {
    default upgrade;
    '' close;
}

# 已配置 TLS 的 server 块
location = /nyaterm { return 308 /nyaterm/; }
location /nyaterm/ {
    proxy_pass http://127.0.0.1:8080;
    proxy_http_version 1.1;
    proxy_set_header Host $http_host;
    proxy_set_header Origin $http_origin;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection $nyaterm_connection_upgrade;
    proxy_buffering off;
    proxy_request_buffering off;
    proxy_read_timeout 650s;
    client_max_body_size 0;
}
```

关闭响应缓冲以支持 SSE / 下载，关闭请求缓冲以支持流式上传。可根据部署需求调整上传大小和代理超时。Web 不提供跨源 API 或 Vite 开发代理；验证运行模式应使用构建后的 dist。

## 数据与能力范围

停服后备份整个数据目录，并单独备份加密密钥与登录密码。密钥丢失将无法解密凭据。不要让多个进程同时写同一个 redb volume，也不要把桌面用户数据目录直接作为服务器 volume。保存的私人密钥、密码和 AI provider 密钥使用原有 AES-GCM 存储。AI 历史和连接元数据属于实例共享数据，未作为整个数据库加密。

支持保存/临时 SSH、Telnet 和 VNC，密码/私钥与手工交互认证、主机指纹确认、终端输入/输出/resize、SFTP 浏览与单文件流式上传/下载、文本编辑、基础 AI Ask、设置持久化以及资源/GPU/NPU/进程监控。监控使用独立 SSH exec 通道，Docker 管理仍不支持。Telnet 支持原有字符编码、自动登录、本地回显、行编辑、NAWS 和 raw TCP；VNC 支持现有认证与服务器密钥确认、缩放、共享连接、只读、文本剪贴板和重连。三种协议均支持 SOCKS5、HTTP CONNECT 及代理认证、SSH 跳板（最多 8 层，使用跳板自身网络配置）。代理失败不回退到直连。插件部分提供兼容性及权限检查框架，当前没有浏览器插件安装/执行器。

Web 当前不支持本地 Shell、Serial、RDP、ProxyCommand、SSH agent/X11/证书登录、SSH 启动命令、非标准 SSH profile、非 UTF-8 SSH 终端/文件名、SFTP compatibility mode、CWD 自动跟踪、录制、SCP/Zmodem、本地文件 watcher、传输暂停/重试、同步备份和 AI Agent/MCP/本地附件。系统托盘、原生窗口、OS credential manager、系统全局快捷键、桌面通知、自动更新也不提供。浏览器文件、剪贴板、链接和 iframe 页面代替相应原生入口，仍受浏览器权限约束。

刷新页面时，按工作区 pane ID 重新附着当前登录的有效会话；旧连接租期结束后，保存的连接按原有工作区恢复流程重新创建。SFTP 仅对 SSH 会话提供。浏览器剪贴板需要 HTTPS 和浏览器权限，未授权时显示提示，画面和键鼠仍可使用。网络面板可管理代理及分组，Web 子窗口使用同源 iframe。单镜像部署，无需 noVNC、Guacamole 或额外服务。

更新时，源码部署需重新构建镜像，发布镜像部署需拉取目标版本，随后重建容器。普通 `docker compose down` 保留数据卷；`down -v` 会删除数据卷。恢复停服备份时，使用原加密密钥、恢复整个数据目录及 UID/GID 10001 的读写权限，再启动单个实例。浏览器中的密码备份用于配置迁移，不能代替整个数据目录的备份。

## Web Beta 能力矩阵与验收

| 能力                            | Web Beta                                                | 桌面兼容                           |
| ------------------------------- | ------------------------------------------------------- | ---------------------------------- |
| SSH/Telnet                      | 登录 owner 隔离、输入输出、刷新附着、有限重试恢复       | 原生协议与生命周期保持             |
| VNC                             | 现有 Web 认证、输入、剪贴板及恢复                       | 保持                               |
| 单文件上传/下载                 | 流式原始上传；浏览器 Blob 下载并提示失败                | 保持原生传输                       |
| 同名上传                        | ask/skip/rename/overwrite；POSIX 原子覆盖，拒绝目录覆盖 | 共用 duplicate_strategy 设置       |
| 文件编辑                        | 默认内部编辑器，工作区或 iframe；二进制/不支持编码下载  | 保留外部编辑器、watcher 和本地路径 |
| 传输设置                        | 仅冲突策略与内部编辑显示方式                            | 隐藏字段完整保留，无迁移           |
| 目录传输、调优、暂停/重试       | 未实现，隐藏入口；含目录选择禁用下载                    | 保持                               |
| 资源/GPU/NPU/进程监控           | 独立 SSH exec 与白名单进程信号                          | 保持                               |
| RDP、录制、云同步、AI Agent/MCP | 当前不支持                                              | 保持                               |

上传继续使用 `POST api/sessions/{id}/upload?path=...` 和原始文件正文；返回 `{ "bytes": 123, "status": "completed", "path": "/final/path" }`。跳过返回 bytes=0、status=skipped；改名返回 UUID 后缀的最终 path。覆盖依赖远端 `posix-rename@openssh.com`；不支持时明确失败并保留原文件，不回退到先删后写。

SSH/Telnet 初次附着及断线恢复的每轮预算为 25 秒，单次握手最多 5 秒；退避依次 1、2、4 秒，之后不超过 5 秒。WebSocket 断开、发送失败或超时只释放当前附着；服务端保留远端会话约 30 秒，期间可重新附着同一个 session ID。输出队列有界，断开期间队列填满会对远端施加背压；不保证重放已经交给旧 WebSocket 的字节。握手失败会查询当前会话；401/404 停止恢复，其余临时失败继续到预算结束。显式关闭、注销、登录过期、远端结束或租约到期会结束会话。注销从 File 菜单进入。

完整合并检查由 [Web merge checks](../.github/workflows/web-merge-checks.yml) 执行，包含前端 lint、国际化、Vitest、Web 构建、Core / Web Rust 测试、三个平台的桌面 cargo check，以及 Docker / Chromium 验收。Docker/OpenSSH 验收需要 Docker CLI：

```sh
pnpm exec playwright install chromium
pnpm test:web:e2e
```

脚本为根路径和 `/nyaterm/` 构建独立镜像，启动临时 OpenSSH 容器，检查登录、持续输出中的刷新与原会话重附着、文件编辑/传输和注销。失败产物保存在 `artifacts/web-e2e/`。部署后还需通过实际 HTTPS 代理确认登录、终端、SSE 与上传下载。

## 密码备份与文件导入

File 菜单的导出在浏览器中设置并确认本次备份密码，下载密码保护的 `.nya` 文件；导入选择文件、输入该文件的原备份密码，并确认覆盖实例配置。取消选择不会报错，执行期间不能重复提交。备份密码与 Web 登录密码、部署加密密钥相互独立，仅在本次请求中使用。

`.nya` 延续桌面已有格式和旧版解码能力。Web 导出将凭据包装到独立的备份密钥中；Web 导入解开源密钥，把凭据重新加密到目标实例。可以在使用不同部署密钥的 Web 实例之间迁移，不需要复制或替换目标 `NYATERM_WEB_ENCRYPTION_KEY`。桌面导出的备份密码是导出时的桌面主密码；当前桌面导入入口也使用桌面主密码解密文件，因此导入 Web 备份前需将桌面主密码设为该备份密码。

导入先校验密码、完整性、数据关系和凭据，再用单个 redb 事务恢复；失败保留原配置。备份中的 Serial、本地 Shell 等桌面配置会保留，Web 能力检查控制使用。导入成功后刷新连接、设置、快捷命令、历史和笔记。运行中的会话仍使用已建立的连接。

桌面备份中的旧连接内联明文密码会在导入时转换为目标实例的加密凭据，空字符串按未保存密码处理。符合 AES-GCM 密文格式的字段仍必须通过解密校验，不会在解密失败后当作明文保存；私钥、托管账号、OTP 等其他凭据继续严格校验。

连接导入支持 Xshell、MobaXterm、SecureCRT、WindTerm、Electerm、NyaTerm JSON 的单文件导出。服务端只解析上传内容，不读取上传内容指定的服务器路径。WindTerm 如果依赖外部配置或私钥，需先在桌面完整导入再迁移 `.nya`；FinalShell 目录、Termius 本机数据库和自动扫描 `~/.ssh/config` 仅桌面支持。Xshell、MobaXterm、SecureCRT 的导出通常不包含密码，导入后会提示补充凭据。主题、快捷命令、关键词规则和私钥的可见文件入口使用浏览器选择与下载。

| API                            | 正文与结果                                                | 上限                         |
| ------------------------------ | --------------------------------------------------------- | ---------------------------- |
| `POST api/backups/export`      | JSON `{ "password": "…" }`；返回 `.nya` 二进制            | 输出 50 MiB                  |
| `POST api/backups/import`      | multipart `file`、`password`；成功返回 `status=completed` | 文件 50 MiB，解压 50 MiB     |
| `POST api/imports/connections` | multipart `file`、`source`；返回 `imported`、`warnings`   | 文件 10 MiB，归档展开 50 MiB |

上述接口要求当前登录、正确的 Origin 和 CSRF；`source` 只接受 `xshell`、`mobaxterm`、`securecrt`、`windterm`、`electerm`、`nyaterm_json`，不接受服务器文件路径。

## 部署日志与诊断

服务默认输出 JSON 日志到 stdout，可用 `docker logs --tail 200 nyaterm-web` 或 `docker compose -f deploy/web/docker-compose.yml logs --tail 200 nyaterm` 查看。同时写入数据目录的 `logs/` 子目录。日志按 UTC 日期或 10 MiB 轮转，总量不超过 100 MiB；诊断设置中的级别和保留天数保存后立即生效，保留天数为 1–30 天。文件写入失败时继续 stdout 日志并保持业务运行，正常停机刷新日志。

仓库 Compose 未单独配置 Docker 日志轮转，沿用 Docker daemon 的日志驱动及限制。可在服务中添加 `logging: { driver: json-file, options: { max-size: "10m", max-file: "3" } }`，或在 `docker run` 中添加 `--log-opt max-size=10m --log-opt max-file=3`。Docker stdout 日志与应用数据目录日志分别轮转。

每个 API 响应包含 `X-Nyaterm-Request-Id`；JSON 错误也带 `request_id`。前端 invoke 的请求 ID 传入服务端，日志记录路由模板、操作、状态码、耗时和相关会话 ID。异步连接任务延续请求关联。底层错误先记录类别、错误码、固定原因及原始错误指纹，再返回通用错误。终端帧和命令正文不会逐条记录。

浏览器未登录时仅保留有界内存队列，登录后批量上传现有 logger、未捕获异常和 Promise rejection。每批最多 50 条、256 KiB，服务端每个登录每分钟最多接受 120 批；断网上报最多重试两次（1 秒、2 秒退避），丢弃数量合并到后续批次，不递归记录上报失败。`POST api/logs/frontend` 和 `GET api/diagnostics/export` 要求登录；POST 同时校验 Origin 与 CSRF。

在 Web 设置的诊断区下载 ZIP 包，包含最多 20 MiB 的近期脱敏日志，以及版本、系统、架构和能力清单。任意错误文本和不可信文本以指纹记录；密码、Cookie、CSRF、私钥、命令正文、终端内容不会写入诊断包，数据库和备份也不会收录。Web 隐藏打开本地日志目录入口。本次不提供页面日志查看器。

## 本机浏览器验收

安装 Chromium 后，以本机编译的 Web 服务和真实 dist 执行登录、主题/窄屏截图、备份往返、诊断下载及会话恢复：

```sh
pnpm exec playwright install chromium
pnpm build:web
cargo build --manifest-path src-tauri/crates/nyaterm-web/Cargo.toml --locked
node scripts/web-local-e2e.mjs
```

子路径验收先用 `NYATERM_WEB_BASE_PATH=/nyaterm/` 重新构建前端，再设置 `NYATERM_LOCAL_E2E_BASE_PATH=/nyaterm/` 执行同一脚本。测试使用临时数据目录和随机秘密，退出后清理服务进程。截图保存在 `artifacts/web-local-e2e/`。

该本机脚本只覆盖不依赖 OpenSSH 容器的浏览器功能；完整 SSH 刷新恢复和容器部署验收使用 `pnpm test:web:e2e`。当前提交的验证结果以对应的 CI run 为准。
