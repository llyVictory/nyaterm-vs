# Web / Docker 部署

Web 模式复用现有 React 界面，由 `nyaterm-web` 同时提供静态资源和 HTTP / WebSocket / SSE API。此 Beta 面向一个可信管理员：不同登录的终端、认证提示和运行中的 AI 请求隔离，但保存的连接、凭据、设置、known_hosts 和 AI 历史在同一实例内共享。

## 本地运行

需要 Node 24、pnpm 11.10 和 Rust 1.97.1（本次实际验证的版本）。

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
$env:NYATERM_WEB_PUBLIC_URL = 'http://localhost:8080/'
$env:NYATERM_WEB_BIND = '127.0.0.1:8080'
$env:NYATERM_WEB_DATA_DIR = Join-Path $env:LOCALAPPDATA 'NyaTermWebData'
pnpm web:serve
```

访问 `http://localhost:8080/`，使用生成的登录密码。PowerShell 示例要求 PowerShell 7 / .NET 的 `GetBytes(int)` API。Linux 可用 `umask 077` 后分别执行 `openssl rand -base64 32 > login` 和 `openssl rand -base64 32 > encryption`，然后配置相应 `_FILE` 变量。

## 环境变量

| 变量                                                             | 含义                                                                                                |
| ---------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| `NYATERM_WEB_PASSWORD` / `NYATERM_WEB_PASSWORD_FILE`             | 登录密码，至少 32 字符；推荐随机生成。指定 `_FILE` 时优先读取文件。                                 |
| `NYATERM_WEB_ENCRYPTION_KEY` / `NYATERM_WEB_ENCRYPTION_KEY_FILE` | 标准 Base64 编码的 32 字节随机密钥，必须持久保存，禁止重新生成后直接复用旧数据。                    |
| `NYATERM_WEB_PUBLIC_URL`                                         | 浏览器实际访问地址，含端口和 base path；默认 `http://localhost:8080/`。非回环地址默认必须为 HTTPS。 |
| `NYATERM_WEB_BIND`                                               | 服务监听地址，默认 `127.0.0.1:8080`，容器为 `0.0.0.0:8080`。                                        |
| `NYATERM_WEB_DATA_DIR`                                           | redb 数据目录，默认 `./nyaterm-web-data`，容器为 `/data`。                                          |
| `NYATERM_WEB_DIST`                                               | 已构建 React 资源目录，默认 `dist`，容器为 `/app/dist`。                                            |
| `NYATERM_WEB_BASE_PATH`                                          | **前端构建时**使用的路径，默认 `/`，必须与 PUBLIC_URL 的路径一致并以 `/` 结尾。                     |

`NYATERM_WEB_ALLOW_INSECURE_HTTP=true` 可显式允许局域网 HTTP 地址；同时将 `NYATERM_WEB_PUBLIC_URL` 设置为浏览器实际使用的 IP/域名和端口，并将监听地址设为 `0.0.0.0:8080`。前端请求 ID 使用 `crypto.getRandomValues` 兼容 HTTP，但剪贴板等浏览器能力仍可能要求 HTTPS。Host/Origin 校验继续生效，访问地址必须与 PUBLIC_URL 一致。

服务启动会验证数据密钥；错误密钥会阻止启动。文件方式会去掉末尾换行。登录密码可独立替换，服务器重启使所有旧登录失效；加密密钥目前没有在线轮换 API。

## Docker

```sh
docker build -t nyaterm-web .
docker volume create nyaterm-data
docker run -d --name nyaterm-web --init \
  -p 127.0.0.1:8080:8080 \
  -e NYATERM_WEB_PUBLIC_URL=http://localhost:8080/ \
  -e NYATERM_WEB_PASSWORD_FILE=/run/secrets/login \
  -e NYATERM_WEB_ENCRYPTION_KEY_FILE=/run/secrets/encryption \
  --mount type=bind,src=/absolute/private/secrets,dst=/run/secrets,readonly \
  --mount type=volume,src=nyaterm-data,dst=/data \
  --read-only --tmpfs /tmp --cap-drop ALL \
  --security-opt no-new-privileges:true \
  nyaterm-web
```

容器使用 UID / GID `10001:10001`。绑定目录时，确保该用户可以读取秘密文件和写入数据目录；新 named volume 会继承镜像中 `/data` 的所有权。默认端口映射仅暴露本机。如需远程访问，使用下面的 HTTPS 反向代理配置。

仓库还提供 `docker-compose.yml`，它通过外部环境变量注入密码和密钥：

```sh
docker compose -f docker-compose.yml up --build -d
```

提前在当前 shell 或受限 `.env` 中设置两个必填秘密。`.env*` 已排除在 Git 和 Docker build context 外。生产环境优先将 Compose 改为 `secrets` 挂载和 `_FILE` 变量，避免把秘密写入镜像。Dockerfile 使用 Node、Rust、Debian 三阶段构建，最终镜像只含 dist、服务程序和运行依赖。

## HTTPS 和子路径

例如部署到 `https://terminal.example.com/nyaterm/`：

```sh
docker build --build-arg NYATERM_WEB_BASE_PATH=/nyaterm/ -t nyaterm-web .
```

运行时设置 `NYATERM_WEB_PUBLIC_URL=https://terminal.example.com/nyaterm/`。前端和后端必须使用相同的 origin 和路径；后端会校验原始 Host / Origin，不使用转发头放宽校验。代理应保留完整路径，不能剥掉 `/nyaterm`。

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

测试与架构见 [web-architecture.md](web-architecture.md)。当前环境未安装 Docker CLI；镜像实构建及浏览器手工验收尚未执行，仍需在部署环境完成。更新后请重新构建镜像并重建容器，旧镜像不会自动获得协议支持。

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

SSH/Telnet 初次附着及断线恢复的每轮预算为 25 秒，单次握手最多 5 秒；退避依次 1、2、4 秒，之后不超过 5 秒。握手失败会查询当前会话；401/404 停止恢复，其余临时失败继续到预算结束。显式关闭、注销、服务端关闭均取消恢复。注销从 File 菜单进入。

本轮检查结果、只读 CI 和真实 Docker/OpenSSH/Playwright 验收命令见 [Web Beta 合并准备](web-merge-readiness.md)。本机尚未执行实际 Docker/Chromium，不能以接口 fixture 测试代替部署验收。
