# Web 接口兼容审计与 DBX 对照

审计日期：2026-10-04。范围：生产前端 `src/` 的命令调用、Web HTTP/WS/SSE 适配器、`nyaterm-web` 的命令与专用路由、共享 Core，以及 `temp/dbx` 的桌面/Web 实现。下方审计结论、统计和接口清单保留修复前基线；实现状态以新增的“修复进展”节为准。候选缺口不等同于已复现错误。

## 修复进展（2026-10-04）

本轮已完成主要公共 UI 缺口与浏览器替代流程。276 个命令、462 个调用点及 184 个未匹配命令仍是**原始审计基线**，未重新扫描；附带 `web-api-inventory.json` 也保留基线。不得将基线中“未匹配”当作当前实现状态或错误数量。

| 功能 | 已实现行为 |
| --- | --- |
| 命令建议与快捷命令 | 返回真实 `{commands,categories}`；保存、upsert、计数、搜索、导入、变更事件与持久历史可用；恢复建议和快捷命令面板。共享导入保留既有排序、创建时间和使用次数。Web 仅把显式提交记入历史，shell 确认候选真实返回 `false`，不从终端输出猜测命令。 |
| 普通配置 | 实现语言保存、组递归删除、连接清空、连接/组/凭据排序、known_hosts 清空以及连接图标/监控资产更新。语言仍属于服务端实例共享 UI 配置，不是独立浏览器偏好。 |
| SFTP | 图片/PDF 二进制预览、链接创建/目标替换、非递归权限与数值 UID/GID 修改、普通文件复制/移动、缺失项查询。预览上限 25 MiB，受调用方更低限额约束；普通流式复制上限 1 GiB，支持所属会话间复制/移动；目录仅支持同会话移动。支持重复项询问、跳过、改名和覆盖，取消/失败清理临时文件。 |
| 终端生命周期 | Web 后台标签不再调度桌面 renderer detach；保留浏览器输出策略。隐藏持久录制与深度历史入口，浏览器可导出当前 xterm 缓冲区文本（含正确的软换行拼接）。 |
| 秘密解锁 | `verify_master_password` 真正验证已配置主密码；未配置时验证 Web 登录密码。使用固定长度摘要常量时间比较及尝试限速，错误密码返回 `false`，不更换加密密钥。四种语言的 UI 已说明 Web 验证方式。 |
| OTP | 加密保存/删除、遮蔽列表、秘密读取、TOTP/HOTP 生成及 URI 解析；HOTP 成功生成后原子更新计数器。浏览器解析二维码图片。 |
| 笔记 | 树、CRUD、移动/重命名/删除、版本冲突与事件；浏览器下载 JSON snapshot。版本冲突保留 `Revision conflict` 语义并返回 HTTP 409。 |
| 翻译与远端监控 | 抽离 Tauri 依赖并共享翻译/监控实现；开放资源、GPU、NPU、进程监控及白名单信号操作。命令通过当前用户所属 SSH 会话的独立 exec 通道执行，限制时间、并发与输出，不进入交互 PTY。 |
| 浏览器文件操作 | 主题 JSON 导入导出、快捷命令 JSON/WindTerm 导入与 JSON 导出、连接/AI 图标、背景图片、剪贴板 PNG 上传及终端文本下载使用 File/Blob。图标和背景图有大小/尺寸限制；桌面背景文件路径在 Web 不触发服务端文件读取。 |
| 其他公共接口 | About 返回真实 Web runtime 与服务端 OS，隐藏 ConPTY；AI 审计写入强制脱敏，provider key 读取恢复。 |

SFTP 覆盖和链接目标替换使用 OpenSSH `posix-rename@openssh.com` 原子替换扩展；远端不支持时明确失败，保留原目标。仅发送需要修改的属性，避免库的默认零值覆盖文件大小、时间戳或所有者；未指定的 UID/GID 保持原值。目前所有者/组名解析、递归属性修改、目录复制和跨会话目录移动尚未实现。

秘密解锁沿用桌面的“界面揭示前验证”语义。服务端秘密 API 仍由 Web 登录与 CSRF 保护，**没有新增逐次解锁授权 lease**；主密码修改继续保持桌面流程。共享存储、配置、命令历史与笔记遵循现有单实例契约，会话、提示和远端操作按登录 owner 隔离。

### 保留边界

深度终端历史、持久录制、递归传输/完整任务取消、云备份、HTTP MCP、RDP 与 Docker 管理仍需独立服务端设计；本轮保持相关入口隔离。客户端本地终端、当前串口、托盘、系统快捷键、本机 CLI、native plugin 和文件 watcher 继续保持桌面专属。浏览器快捷命令导入支持 JSON/WindTerm，XTS 压缩包导入仍属于桌面流程。Web 尚无终端 CWD 跟踪，CWD 查询返回真实 `null`，不声称目录跟随已恢复。

### 验证

- 共享 Core：355 项单元测试通过，包含迁移后的翻译、监控和快捷命令测试。
- Web：4 项集成测试全部通过，覆盖 HTTP 数据契约、配置持久化、真实密码/限速、RFC HOTP 计数、笔记冲突、SSH/SFTP/WS/SSE owner 边界、预览限额、流式复制移动、权限、链接原子替换及独立监控通道。
- 前端：相关回归测试共 55 项通过，覆盖命令建议、后台终端、笔记、文件剪贴板、终端菜单/搜索以及浏览器 PNG 上传和背景路径隔离。
- `pnpm build:web`、TypeScript、`pnpm lint`、四语言格式检查及桌面 `cargo check --locked` 通过。lint 仍有原有 `CommandSuggestions.tsx` hook dependency 警告；构建仍提示较大的 chunk 和旧 Browserslist 数据。
- 本轮未进行真实浏览器交互/视觉验收、外部翻译服务实测或 GPU/NPU 硬件实测；自动化覆盖不能替代这些环境验证。

## 结论

当前问题是接口覆盖、数据契约和能力边界不完整，并不是 Web 性能不足。SSH/Telnet/VNC 的主连接路径已经存在，但公共 UI 中仍有未适配的入口；某些功能虽然不再报 501，却只返回空数据，不能算实现完成。

`src/lib/backend` 是**前端的平台/传输适配层**，真正的 Web 后端是 Rust 的 `src-tauri/crates/nyaterm-web`。`supports()` 是运行环境能力判断，不是 HTTP 接口实现，也不是服务端鉴权。UI 隐藏入口、调用层选择合适实现、服务端验证权限和参数，需要同时成立。

静态扫描发现 276 个直接使用字符串命令名的命令、462 个调用点：72 个能匹配 Web 服务端处理入口，20 个由前端 HTTP/浏览器适配器处理，184 个没有在这两个入口集合中找到。还有 18 处动态命令表达式。**184 不等于 184 个 Web 报错**：包括已被运行环境分支、面板过滤隔离的桌面命令，且动态命令不能仅靠字符串统计确认。72 中也包括占位处理，不代表功能全部可用。

## 已确认的公共入口缺口

| 优先级 | 功能与触发条件                                                    | 当前接口/行为                                                                                                                                                    | Web 适合度与处理建议                                                                                                                                                             |
| ------ | ----------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| P0     | 命令建议；输入命令、手动打开建议                                  | `fuzzy_search_commands` 未注册；`get_command_history` / `fuzzy_search_history` 只返回 `[]`；`get_quick_commands` 返回 `[]`，但调用方需要 `{commands,categories}` | 适合实现。复用 Core 的 `config/quick_command.rs`、`core/history`、`utils/fuzzy`，补齐读写、搜索、事件后恢复建议和设置。修正返回形状是必要条件                                    |
| P0     | 快捷命令；终端右键“保存为快捷命令”、AI 保存命令、底部快捷命令面板 | `upsert_quick_command`、`save_quick_commands`、`increment_quick_command_use_count` 未注册；部分入口不受侧栏面板过滤保护                                          | 适合实现。统一快捷命令 CRUD 与事件；浏览器文件导入导出单独适配。不能只隐藏侧栏或建议开关                                                                                         |
| P0     | 后台 SSH/Telnet 标签空闲超过 120 秒                               | `xterminalHibernationController` 调用未实现的 `detach_session_renderer`；没有 Web 平台分支                                                                       | 现有桌面协议不适合直接照搬。先关闭 Web 的深度 renderer detach；继续保留输出调度、渲染节流。将来实现 Web 专用的暂停/快照/重放协议。不能伪造 detach 成功，否则可能丢输出           |
| P0     | 凭据管理；已有主密码时点击解锁                                    | `verify_master_password` 未注册，而 `SecretUnlockFooter` 仍依赖它                                                                                                | 需要专门设计 Web 登录与秘密解锁语义。必须保留真实验证，不能返回 `true` 或用隐藏错误替代。主密码变更/密钥迁移继续隔离                                                             |
| P1     | 语言切换                                                          | Header 调用未注册的 `save_app_language`                                                                                                                          | 适合实现。复用设置更新，明确浏览器 UI 偏好与服务器实例配置的归属                                                                                                                 |
| P1     | 保存连接树；删除文件夹、清空连接、拖动排序                        | `delete_group`、`clear_all_connections`、`reorder_items` 未注册                                                                                                  | 适合实现。共享配置操作、子组删除语义、排序语义和 `connections-changed` 事件；不能只删当前层或只在前端乐观更新                                                                    |
| P1     | known_hosts 的“清空”                                              | `get_known_hosts`、单项删除已实现，`clear_known_hosts` 未注册                                                                                                    | 适合实现。保持现有确认对话框与服务端写入验证，清空后下次 SSH 仍应重新验证指纹                                                                                                    |
| P1     | 凭据拖动排序                                                      | `reorder_credentials` 未注册                                                                                                                                     | 适合实现；共享排序规则，避免前端排序成功后刷新回退                                                                                                                               |
| P1     | 会话快速切换器；输入查询                                          | `fuzzy_search_candidates` 未注册                                                                                                                                 | 适合实现。可在浏览器本地搜索，或用共享 Core 的无副作用搜索；不需要因为缺接口隐藏整个切换器                                                                                       |
| P1     | AI 上下文、文件管理器“同步 CWD”                                   | 调用 `get_terminal_cwd`；服务端只处理 `try_get_terminal_cwd` / `get_session_cwd`，且返回 null                                                                    | 分两步。AI 上下文应接受“暂无 CWD”并停止错误请求；CWD 同步须有实际跟踪结果才能启用，不能仅增加别名就声称完成                                                                      |
| P1     | 终端搜索；切换“深度历史”                                          | `terminal_history_search` 未实现，而模式按钮可见                                                                                                                 | 适合后续实现，但要补有界终端历史、分页/搜索、会话 ownership。当前仅开放实际可用的缓冲区搜索；不要返回空成功来冒充深度历史搜索                                                    |
| P1     | SFTP 图片/PDF 预览                                                | 动态生成 `read_remote_file_bytes`，没有服务端分支；文本预览已实现                                                                                                | 适合实现。复用已有远程读流和 Core 的文件类型；校验 maxBytes、取消和响应形状。动态命令必须纳入审计                                                                                |
| P1     | SFTP 创建链接、修改权限/所有者/链接目标                           | `create_remote_symlink`、`update_remote_file_attributes`、`update_remote_symlink_target` 未实现                                                                  | 适合实现。是远端 SFTP 操作，不依赖客户端本地文件系统；保持路径、权限和会话验证                                                                                                   |
| P1     | SFTP 新建文件/目录                                                | 前端传递 mode，Web 分支没有应用 mode                                                                                                                             | 接口存在仍有语义差异。应用用户选择的权限，失败应如实返回，不应静默忽略参数                                                                                                       |
| P1     | SFTP 文件复制/剪切/粘贴                                           | 动态 `copy_file_entry` / `move_file_entry`、`find_missing_remote_entries` 缺失                                                                                   | 同一远端 rename/move 适合优先实现；跨会话复制和递归目录需任务、取消、重名处理。暂未实现的动作应单独 gate，不能把整个 SFTP 隐藏                                                   |
| P1     | AI 命令插入/保存等动作的审计                                      | `append_ai_audit` 未注册；共享 Core 已有实现                                                                                                                     | 适合实现。保留脱敏与审计字段，复用 Core；失败不应长期靠 `.catch(() => {})` 掩盖                                                                                                  |
| P1     | AI 设置；主动查看 provider API key                                | `reveal_ai_provider_api_key` 未注册                                                                                                                              | 适合在既定登录/秘密解锁策略下实现；列表仍保持掩码。与通用 secret getters 使用一致策略                                                                                            |
| P1     | 新建/编辑连接的自定义图标                                         | 仍通过 native dialog + `import_connection_icon(path)`；删除 `delete_connection_custom_icon` 缺失                                                                 | 浏览器 File 选择、图片校验/缩放和 data URL/上传替代路径参数；删除配置元数据适合服务端实现                                                                                        |
| P1     | 主题导入导出、AI provider 图标、背景图片                          | 仍使用 native dialog、`read_theme_file` / `write_theme_file` / `import_ai_provider_icon` / 本地图片路径                                                          | 适合浏览器替代：File、JSON 解析、Blob 下载、受限图片资源。客户端文件路径不能被当作 Docker 主机路径；桌面保存的本地背景路径也不应触发 Web 读文件请求                              |
| P2     | About 对话框                                                      | `get_support_info` 未注册，打开即查询                                                                                                                            | 适合返回 Web 版本/运行环境信息；ConPTY 等桌面信息不显示。服务器 OS 与浏览器 OS 需区分                                                                                            |
| P2     | OTP 管理页                                                        | `get_otp_entries` 在 HTTP adapter 返回空列表；保存/删除/生成/二维码导入缺失，OTP Tab 仍可见                                                                      | TOTP 存储、生成适合服务端实现；二维码适合浏览器上传/解析。保留手工 SSH keyboard-interactive 提示不依赖 OTP 管理库。不要将占位空数组称为完成                                      |
| P2     | 笔记、翻译、远程 GPU/NPU/进程/Docker 管理                         | 目前面板或设置大多被隔离，相关接口未注册；翻译仍有终端右键入口                                                                                                   | 都有适合 Web 的部分。笔记、翻译可复用 Core/API；监控和 Docker 针对**用户 SSH 的远端主机**执行，绝不能改成管理 Docker 部署宿主机。需要抽离 Tauri 依赖后实现；未完成前隔离所有入口 |

重点源码：`src/hooks/useCommandHistory.ts`，`src/components/terminal/xterminalHibernationController.ts`，`src/components/terminal/TerminalContextMenu.tsx`，`src/components/terminal/TerminalSearchBar.tsx`，`src/components/dialog/terminal/SessionQuickSwitcherDialog.tsx`，`src/components/panel/file-explorer/FilePreviewContent.tsx`，`src/components/dialog/file-explorer/PropertiesDialog.tsx`，`src/components/panel/ai/AIAssistantPanel.tsx`，`src/components/panel/security-auth/SecretUnlockFooter.tsx`。

## 适合 Web、浏览器替代和桌面专属的边界

| 分类                       | 功能                                                                                                                                                                                                | 判断                                                                                                                                             |
| -------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| 应直接补齐 Web API         | 连接/组/排序、凭据、known_hosts、快捷命令、命令建议、笔记、OTP、远端 SFTP 元数据、AI 通用设置/审计、翻译                                                                                            | 多数是 Core 数据操作或远程协议，不需要用户电脑原生能力。缺路由不是“不适合 Web”                                                                   |
| 需要浏览器替代             | 文件导入导出、图标、主题、背景图、剪贴板、子窗口、导出当前终端文本                                                                                                                                  | 使用 File/Blob、Clipboard API、同源 iframe；浏览器权限/安全上下文单独处理。不能为了复用旧接口开放服务器任意本地路径                              |
| 能做，但需要服务端专门设计 | 持久终端录制/深度搜索、递归传输/任务取消、远程监控与 Docker 管理、云同步备份、SSH 高级功能、Web HTTP MCP、RDP                                                                                       | 不属于浏览器本质限制；需要存储限额、任务生命周期、网络/会话归属和配置。当前可以隐藏未完成入口，但理由是阶段/协议差异，不是“不可能实现”           |
| 保持桌面专属               | 用户电脑本地 Shell、当前串口协议、系统托盘、系统全局快捷键、原生窗口透明/拖动/最小化、打开本地目录、OS credential manager、桌面自更新、用户电脑的 Codex/Claude CLI、native plugin、桌面文件 watcher | 现有 Web 部署无法直接访问客户端 OS。不要把“本地终端”替换成 Docker 服务器 shell。WebSerial 等需要全新协议与浏览器支持，不能复用当前服务器串口调用 |
| 仅隔离桌面底层协议         | Tauri Channel、桌面输出 ACK credits、renderer detach/snapshot、native drag-drop 路径                                                                                                                | 在 transport/platform 层替换或明确拒绝；这些差异不应传播成大量业务 UI 分支                                                                       |

现有 ACK 适配是合理的：Web WS 没有桌面的字节确认协议，浏览器适配层完成本地 ACK 状态即可，不能持续请求 `ack_session_output`。与此不同，对存在实际副作用的命令（保存、删除、解锁、detach 等）不可伪造成功。

现有设置与面板 gates 已隔离许多功能，例如 AiAgentsTab、云同步、录制运行状态、网络隧道管理、桌面 VNC Channel。不应把这些已隔离调用算成当前可复现错误。不过，侧栏/设置 gate 不会自动覆盖右键菜单、快捷键、AI 动作、子页面和后台生命周期；本次发现的缺口正包含这些路径。

## DBX 是怎样做的

DBX **没有使用 NyaTerm 的 `supports()` 或这条 import 路径**，但并不是没有平台差异判断。它使用 `isTauriRuntime()`，在 transport 层、Vue 组件和 composables 中都能找到判断。

| 层次             | DBX 源码                                                                                  | 做法与可借鉴点                                                                                                                                                   |
| ---------------- | ----------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 统一 API         | `temp/dbx/apps/desktop/src/lib/backend/api.ts:12`                                         | `getBackend()` 懒加载 `tauri.ts` / `http.ts`；`forward()` 转发具名业务方法。业务 UI 不直接拼未知 IPC 命令                                                        |
| 两套适配         | `temp/dbx/apps/desktop/src/lib/backend/http.ts`、`tauri.ts`                               | 导入共同的请求/响应类型。适合 Web 的功能有实际 HTTP 实现；纯桌面操作在 HTTP 方法中明确拒绝；部分只读 OS 查询返回明确空值                                         |
| 浏览器持久化     | `temp/dbx/apps/desktop/src/lib/backend/browserAppStateStorage.ts`、`http.ts:2338`         | UI/打开标签等存 IndexedDB，localStorage 兜底；不将所有浏览器 UI 状态写成实例共享配置。NyaTerm 当前的 `save_app_ui_settings` 更新共享 settings 文档，值得单独评估 |
| 浏览器文件适配   | `temp/dbx/apps/desktop/src/lib/backend/http.ts:979`                                       | 离线导入明确要求 `File`；文件上传用 multipart，下载用浏览器链接。不能把客户端 path 等同于服务器 path                                                             |
| 服务端共享核心   | `temp/dbx/crates/dbx-web/src/routes/history.rs`                                           | Web 路由调共享 `state.app.storage`；历史保存/搜索/删除有实际实现，不是统一空数组                                                                                 |
| 真正不支持的能力 | `temp/dbx/crates/dbx-web/src/routes/ai.rs:122`                                            | Web 拒绝桌面 CLI provider；UI 也通过 runtime 隔离。这与把 REST provider 的 AI 功能整体隐藏不同                                                                   |
| MCP              | `temp/dbx/crates/dbx-web/src/web_mcp.rs`、`main.rs:1388`                                  | 有独立 Web MCP endpoint，状态区分 management availability。说明 MCP 本身适合 Web，桌面 stdio/本机集成与 Web HTTP 服务是不同实现                                  |
| 契约测试         | `temp/dbx/apps/desktop/src/lib/backend/__tests__/userSkillsCapabilityContract.spec.ts` 等 | 同时检查桌面入口、HTTP 方法、Web 路由、拒绝行为；已经做过 browser state、cloud sync、import/export 等契约测试                                                    |

因此，不建议照搬 DBX 的所有代码或删掉 NyaTerm 的能力判断。应该借鉴它对**具名 API、响应类型、浏览器状态/文件替代、契约测试**的组织。NyaTerm 现有统一传输与共享 Core 方向可保留。

桌面 Rust 的 `temp/dbx/src-tauri/src/commands/history.rs:9` 调用 `state.storage.save_history_entry`；Web 的 `crates/dbx-web/src/routes/history.rs:30` 调用 `state.app.storage.save_history_entry`。`WebState` 包含共享 Core 的 `Arc<AppState>`。两端共享的是实际存储/业务服务，Tauri command 与 Axum route 分别处理各自 transport；Rust 中不会导入 TypeScript 的 `supports()`。

## 对 `supports()` 的具体调整建议

1. 将“环境固有限制”和“本版本尚未实现”明确区分。`nativeWindows` / `tray` 是环境差异；`commandSuggestions` / `remoteMonitoring` 是当前实现状态。后者未来可启用，不应被固定成 Web 永久不支持。
2. UI 能力判断保留在功能入口层；原生窗口、文件、剪贴板、输出 ACK 等替代集中到 adapter。避免把业务能力错误地归因于 `nativeFiles` / `nativeWindows`，例如远端目录下载、SSH 隧道并不是原生窗口功能。
3. 使用具名、类型明确的操作覆盖主要业务调用，或定义命令请求/响应映射。当前 `invoke<T>(任意字符串)` 中 T 只代表调用方断言，不能验证实际 HTTP 响应形状。
4. 建立单一命令/能力清单，标记真实实现、浏览器替代、桌面专属、暂未实现。服务端按实现状态报告能力，前端 UI 消费它；不要依赖几份手写清单各自同步。
5. 不为减少 F12 日志而普遍 `.catch(() => {})`、统一 `return []` / `null` 或关闭所有功能。无副作用的 optional OS 信息可以降级，关键副作用必须实现或显式不可用。
6. 兼容验收覆盖完整入口：主页面、设置、侧栏、右键菜单、快捷键、AI 动作、子 iframe，以及 120 秒以上的后台会话。HTTP unit test 通过不等于 Docker 真实浏览器验收。

## 建议实现顺序

第一批：真实快捷命令与搜索 API、恢复建议、修正 DTO、会话切换搜索、连接树/语言/known_hosts 基础操作；同时隔离未适配的 renderer detach。秘密解锁先确认 Web 策略，禁止假成功。

第二批：SFTP 二进制预览、元数据修改、移动/复制；浏览器主题/图标/导入导出；AI 审计与密钥显式读取。补齐接口事件与跨 iframe 刷新。

第三批：OTP、笔记、翻译、SSH 远端监控/进程/Docker 管理。按模块抽 Core service，再接 desktop/Web adapter。

第四批：终端持久记录/深度搜索、递归传输任务、云备份、SSH 扩展、HTTP MCP/RDP 等独立协议和生命周期工作。未实现前按具体能力隐藏入口。

每批新增真实集成/契约测试：请求名/参数/响应形状/事件一致；Web 不调用 native command；拒绝路径不伪造成功；mutation 保持 ownership、CSRF 与现有加密流程。也验证 `/nyaterm/` 等 base path 和浏览器 HTTP/HTTPS 差异。

## 全量静态命令清单

下面的“服务端入口”只表示能找到匹配处理，“前端适配”只表示在 HTTP/浏览器 adapter 中能找到对应命令；两者都不证明功能语义完整。“未匹配”包括已隔离的桌面专属调用，需要结合上面的触发路径判断。所有调用位置列在 JSON 快照中。

完整调用点：[web-api-inventory.json](web-api-inventory.json)。扫描使用 TypeScript AST 提取直接调用，服务端匹配检查 `commands.rs` 与 SFTP/AI allowlist，动态表达式另列。

| 命令                                          | 静态入口   | 首个调用位置                                                                                                                                       | 调用点数 |
| --------------------------------------------- | ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------- | -------- |
| `ack_session_output`                          | 前端适配   | [src/components/terminal/outputAckCoordinator.ts:335](../src/components/terminal/outputAckCoordinator.ts#L335)                                     | 1        |
| `activate_plugin_version`                     | 未匹配     | [src/lib/plugins.ts:53](../src/lib/plugins.ts#L53)                                                                                                 | 1        |
| `append_ai_audit`                             | 未匹配     | [src/components/panel/ai/AIAssistantPanel.tsx:483](../src/components/panel/ai/AIAssistantPanel.tsx#L483)                                           | 1        |
| `append_frontend_logs`                        | 未匹配     | [src/lib/logger.ts:300](../src/lib/logger.ts#L300)                                                                                                 | 1        |
| `apply_portable_update`                       | 未匹配     | [src/lib/updater.ts:190](../src/lib/updater.ts#L190)                                                                                               | 1        |
| `attach_session`                              | 前端适配   | [src/lib/appSessionFactory.ts:73](../src/lib/appSessionFactory.ts#L73)                                                                             | 5        |
| `begin_github_gist_device_flow`               | 未匹配     | [src/components/settings/SyncBackupTab.tsx:255](../src/components/settings/SyncBackupTab.tsx#L255)                                                 | 1        |
| `cancel_ai_chat_stream`                       | 服务端入口 | [src/components/panel/ai/AIAssistantPanel.tsx:1000](../src/components/panel/ai/AIAssistantPanel.tsx#L1000)                                         | 2        |
| `cancel_docker_sudo_password`                 | 未匹配     | [src/components/dialog/docker/DockerSudoPasswordDialog.tsx:72](../src/components/dialog/docker/DockerSudoPasswordDialog.tsx#L72)                   | 1        |
| `cancel_github_gist_device_flow`              | 未匹配     | [src/components/settings/SyncBackupTab.tsx:272](../src/components/settings/SyncBackupTab.tsx#L272)                                                 | 1        |
| `cancel_marketplace_plugin_review`            | 未匹配     | [src/lib/plugins.ts:19](../src/lib/plugins.ts#L19)                                                                                                 | 1        |
| `cancel_otp_request`                          | 未匹配     | [src/components/dialog/connections/OtpDialog.tsx:97](../src/components/dialog/connections/OtpDialog.tsx#L97)                                       | 1        |
| `cancel_session_creation`                     | 服务端入口 | [src/App.tsx:809](../src/App.tsx#L809)                                                                                                             | 2        |
| `cancel_ssh_auth_request`                     | 服务端入口 | [src/components/dialog/connections/SshAuthDialog.tsx:278](../src/components/dialog/connections/SshAuthDialog.tsx#L278)                             | 1        |
| `cancel_transfer`                             | 未匹配     | [src/context/TransferContext.tsx:743](../src/context/TransferContext.tsx#L743)                                                                     | 3        |
| `check_portable_update`                       | 未匹配     | [src/lib/updater.ts:42](../src/lib/updater.ts#L42)                                                                                                 | 1        |
| `claim_external_open_requests`                | 前端适配   | [src/hooks/useExternalOpenRequests.ts:79](../src/hooks/useExternalOpenRequests.ts#L79)                                                             | 1        |
| `clear_ai_history`                            | 服务端入口 | [src/components/panel/ai/AIAssistantPanel.tsx:1173](../src/components/panel/ai/AIAssistantPanel.tsx#L1173)                                         | 1        |
| `clear_all_connections`                       | 未匹配     | [src/components/panel/saved-connections/index.tsx:819](../src/components/panel/saved-connections/index.tsx#L819)                                   | 1        |
| `clear_known_hosts`                           | 未匹配     | [src/components/panel/security-auth/KnownHostsManagementTab.tsx:64](../src/components/panel/security-auth/KnownHostsManagementTab.tsx#L64)         | 1        |
| `clear_plugin_logs`                           | 未匹配     | [src/lib/plugins.ts:34](../src/lib/plugins.ts#L34)                                                                                                 | 1        |
| `close_plugin_scope`                          | 未匹配     | [src/lib/plugins.ts:57](../src/lib/plugins.ts#L57)                                                                                                 | 1        |
| `close_rdp_session`                           | 未匹配     | [src/App.tsx:1176](../src/App.tsx#L1176)                                                                                                           | 2        |
| `close_session`                               | 服务端入口 | [src/context/AppProvider.tsx:1054](../src/context/AppProvider.tsx#L1054)                                                                           | 4        |
| `close_vnc_session`                           | 服务端入口 | [src/App.tsx:1182](../src/App.tsx#L1182)                                                                                                           | 2        |
| `configure_plugin`                            | 未匹配     | [src/lib/plugins.ts:46](../src/lib/plugins.ts#L46)                                                                                                 | 1        |
| `create_local_session`                        | 未匹配     | [src/context/AppProvider.tsx:1160](../src/context/AppProvider.tsx#L1160)                                                                           | 7        |
| `create_multiplexed_ssh_session`              | 未匹配     | [src/App.tsx:1931](../src/App.tsx#L1931)                                                                                                           | 1        |
| `create_note`                                 | 未匹配     | [src/components/note-editor/NoteEditor.tsx:337](../src/components/note-editor/NoteEditor.tsx#L337)                                                 | 2        |
| `create_note_folder`                          | 未匹配     | [src/hooks/useNotesTree.ts:144](../src/hooks/useNotesTree.ts#L144)                                                                                 | 1        |
| `create_plugin_scope`                         | 未匹配     | [src/lib/plugins.ts:56](../src/lib/plugins.ts#L56)                                                                                                 | 1        |
| `create_rdp_session`                          | 未匹配     | [src/context/AppProvider.tsx:1221](../src/context/AppProvider.tsx#L1221)                                                                           | 4        |
| `create_remote_symlink`                       | 未匹配     | [src/components/dialog/file-explorer/NewSymlinkDialog.tsx:44](../src/components/dialog/file-explorer/NewSymlinkDialog.tsx#L44)                     | 1        |
| `create_serial_session`                       | 未匹配     | [src/context/AppProvider.tsx:1190](../src/context/AppProvider.tsx#L1190)                                                                           | 9        |
| `create_ssh_session`                          | 前端适配   | [src/context/AppProvider.tsx:1149](../src/context/AppProvider.tsx#L1149)                                                                           | 6        |
| `create_telnet_session`                       | 前端适配   | [src/context/AppProvider.tsx:1175](../src/context/AppProvider.tsx#L1175)                                                                           | 9        |
| `create_temporary_ssh_session`                | 前端适配   | [src/lib/appSessionFactory.ts:181](../src/lib/appSessionFactory.ts#L181)                                                                           | 3        |
| `create_vnc_session`                          | 前端适配   | [src/context/AppProvider.tsx:1205](../src/context/AppProvider.tsx#L1205)                                                                           | 3        |
| `delete_ai_session`                           | 服务端入口 | [src/components/panel/ai/AIAssistantPanel.tsx:1197](../src/components/panel/ai/AIAssistantPanel.tsx#L1197)                                         | 1        |
| `delete_command_history`                      | 未匹配     | [src/hooks/useCommandHistory.ts:373](../src/hooks/useCommandHistory.ts#L373)                                                                       | 1        |
| `delete_connection`                           | 服务端入口 | [src/components/panel/saved-connections/index.tsx:733](../src/components/panel/saved-connections/index.tsx#L733)                                   | 1        |
| `delete_connection_custom_icon`               | 未匹配     | [src/pages/NewSessionPage.tsx:676](../src/pages/NewSessionPage.tsx#L676)                                                                           | 1        |
| `delete_credential`                           | 服务端入口 | [src/components/panel/security-auth/CredentialManagementTab.tsx:250](../src/components/panel/security-auth/CredentialManagementTab.tsx#L250)       | 1        |
| `delete_group`                                | 未匹配     | [src/components/panel/saved-connections/index.tsx:808](../src/components/panel/saved-connections/index.tsx#L808)                                   | 1        |
| `delete_known_host`                           | 服务端入口 | [src/components/panel/security-auth/KnownHostsManagementTab.tsx:50](../src/components/panel/security-auth/KnownHostsManagementTab.tsx#L50)         | 1        |
| `delete_local_file`                           | 未匹配     | [src/components/dialog/file-explorer/DeleteDialog.tsx:49](../src/components/dialog/file-explorer/DeleteDialog.tsx#L49)                             | 1        |
| `delete_note_node`                            | 未匹配     | [src/hooks/useNotesTree.ts:193](../src/hooks/useNotesTree.ts#L193)                                                                                 | 1        |
| `delete_otp_entry`                            | 未匹配     | [src/components/panel/security-auth/OtpManagementTab.tsx:181](../src/components/panel/security-auth/OtpManagementTab.tsx#L181)                     | 1        |
| `delete_password`                             | 服务端入口 | [src/components/panel/security-auth/PasswordManagementTab.tsx:310](../src/components/panel/security-auth/PasswordManagementTab.tsx#L310)           | 1        |
| `delete_proxy`                                | 服务端入口 | [src/components/panel/NetworkPanel.tsx:726](../src/components/panel/NetworkPanel.tsx#L726)                                                         | 1        |
| `delete_remote_file`                          | 服务端入口 | [src/lib/terminalZmodemUpload.ts:233](../src/lib/terminalZmodemUpload.ts#L233)                                                                     | 4        |
| `delete_ssh_key`                              | 服务端入口 | [src/components/panel/security-auth/KeyManagementTab.tsx:207](../src/components/panel/security-auth/KeyManagementTab.tsx#L207)                     | 1        |
| `delete_tunnel`                               | 未匹配     | [src/components/panel/NetworkPanel.tsx:738](../src/components/panel/NetworkPanel.tsx#L738)                                                         | 1        |
| `detach_session_renderer`                     | 未匹配     | [src/components/terminal/xterminalHibernationController.ts:300](../src/components/terminal/xterminalHibernationController.ts#L300)                 | 1        |
| `detect_claude_code_cli`                      | 未匹配     | [src/components/settings/AiTab.tsx:606](../src/components/settings/AiTab.tsx#L606)                                                                 | 1        |
| `detect_codex_cli`                            | 未匹配     | [src/components/settings/AiTab.tsx:577](../src/components/settings/AiTab.tsx#L577)                                                                 | 1        |
| `docker_compose_action`                       | 未匹配     | [src/components/panel/DockerManager.tsx:895](../src/components/panel/DockerManager.tsx#L895)                                                       | 1        |
| `docker_compose_service_action`               | 未匹配     | [src/components/panel/DockerManager.tsx:496](../src/components/panel/DockerManager.tsx#L496)                                                       | 1        |
| `docker_container_action`                     | 未匹配     | [src/components/panel/DockerManager.tsx:372](../src/components/panel/DockerManager.tsx#L372)                                                       | 1        |
| `docker_image_remove`                         | 未匹配     | [src/components/panel/DockerManager.tsx:720](../src/components/panel/DockerManager.tsx#L720)                                                       | 1        |
| `docker_network_remove`                       | 未匹配     | [src/components/panel/DockerManager.tsx:825](../src/components/panel/DockerManager.tsx#L825)                                                       | 1        |
| `docker_system_prune`                         | 未匹配     | [src/components/panel/DockerManager.tsx:600](../src/components/panel/DockerManager.tsx#L600)                                                       | 1        |
| `docker_volume_remove`                        | 未匹配     | [src/components/panel/DockerManager.tsx:772](../src/components/panel/DockerManager.tsx#L772)                                                       | 1        |
| `download_portable_update`                    | 未匹配     | [src/lib/updater.ts:109](../src/lib/updater.ts#L109)                                                                                               | 1        |
| `download_remote_directory`                   | 未匹配     | [src/context/TransferContext.tsx:660](../src/context/TransferContext.tsx#L660)                                                                     | 1        |
| `download_remote_file`                        | 未匹配     | [src/pages/RemoteFileEditorPage.tsx:314](../src/pages/RemoteFileEditorPage.tsx#L314)                                                               | 4        |
| `export_config`                               | 未匹配     | [src/hooks/useConfigTransfer.tsx:46](../src/hooks/useConfigTransfer.tsx#L46)                                                                       | 1        |
| `export_diagnostics`                          | 未匹配     | [src/hooks/useConfigTransfer.tsx:106](../src/hooks/useConfigTransfer.tsx#L106)                                                                     | 1        |
| `export_notes`                                | 未匹配     | [src/components/panel/notes/NotesPanel.tsx:115](../src/components/panel/notes/NotesPanel.tsx#L115)                                                 | 1        |
| `export_quick_commands`                       | 未匹配     | [src/components/panel/QuickCommands.tsx:597](../src/components/panel/QuickCommands.tsx#L597)                                                       | 1        |
| `find_missing_remote_entries`                 | 未匹配     | [src/lib/transferDuplicateResolution.ts:15](../src/lib/transferDuplicateResolution.ts#L15)                                                         | 1        |
| `finish_recording_scope`                      | 服务端入口 | [src/lib/appSessionFactory.ts:88](../src/lib/appSessionFactory.ts#L88)                                                                             | 2        |
| `fuzzy_search_candidates`                     | 未匹配     | [src/components/dialog/terminal/SessionQuickSwitcherDialog.tsx:207](../src/components/dialog/terminal/SessionQuickSwitcherDialog.tsx#L207)         | 1        |
| `fuzzy_search_commands`                       | 未匹配     | [src/hooks/useCommandHistory.ts:232](../src/hooks/useCommandHistory.ts#L232)                                                                       | 1        |
| `fuzzy_search_history`                        | 服务端入口 | [src/hooks/useCommandHistory.ts:226](../src/hooks/useCommandHistory.ts#L226)                                                                       | 1        |
| `generate_otp_code`                           | 未匹配     | [src/hooks/useOtpCode.ts:31](../src/hooks/useOtpCode.ts#L31)                                                                                       | 2        |
| `get_ai_messages`                             | 服务端入口 | [src/components/panel/ai/AIAssistantPanel.tsx:453](../src/components/panel/ai/AIAssistantPanel.tsx#L453)                                           | 1        |
| `get_ai_sessions`                             | 服务端入口 | [src/components/panel/ai/AIAssistantPanel.tsx:434](../src/components/panel/ai/AIAssistantPanel.tsx#L434)                                           | 2        |
| `get_app_lock_state`                          | 服务端入口 | [src/hooks/useAppLockState.ts:17](../src/hooks/useAppLockState.ts#L17)                                                                             | 1        |
| `get_app_runtime_info`                        | 服务端入口 | [src/context/AppProvider.tsx:451](../src/context/AppProvider.tsx#L451)                                                                             | 1        |
| `get_app_settings`                            | 服务端入口 | [src/pages/SettingsPage.tsx:395](../src/pages/SettingsPage.tsx#L395)                                                                               | 5        |
| `get_claude_code_account_status`              | 未匹配     | [src/components/settings/AiTab.tsx:655](../src/components/settings/AiTab.tsx#L655)                                                                 | 1        |
| `get_cloud_sync_status`                       | 未匹配     | [src/components/settings/SyncBackupTab.tsx:156](../src/components/settings/SyncBackupTab.tsx#L156)                                                 | 2        |
| `get_codex_account_status`                    | 未匹配     | [src/components/settings/AiTab.tsx:635](../src/components/settings/AiTab.tsx#L635)                                                                 | 1        |
| `get_command_history`                         | 服务端入口 | [src/hooks/useCommandHistory.ts:164](../src/hooks/useCommandHistory.ts#L164)                                                                       | 1        |
| `get_connection_custom_icons`                 | 服务端入口 | [src/pages/NewSessionPage.tsx:474](../src/pages/NewSessionPage.tsx#L474)                                                                           | 1        |
| `get_connection_password_value`               | 服务端入口 | [src/components/sessions/VncForm.tsx:137](../src/components/sessions/VncForm.tsx#L137)                                                             | 4        |
| `get_default_local_shell`                     | 前端适配   | [src/pages/NewSessionPage.tsx:316](../src/pages/NewSessionPage.tsx#L316)                                                                           | 1        |
| `get_docker_compose_services`                 | 未匹配     | [src/components/panel/DockerManager.tsx:452](../src/components/panel/DockerManager.tsx#L452)                                                       | 1        |
| `get_docker_container_details`                | 未匹配     | [src/components/dialog/docker/DockerContainerDetailsDialog.tsx:54](../src/components/dialog/docker/DockerContainerDetailsDialog.tsx#L54)           | 1        |
| `get_docker_container_stats`                  | 未匹配     | [src/components/dialog/docker/DockerContainerDetailsDialog.tsx:75](../src/components/dialog/docker/DockerContainerDetailsDialog.tsx#L75)           | 1        |
| `get_external_mcp_client_configs`             | 未匹配     | [src/components/settings/AiTab.tsx:563](../src/components/settings/AiTab.tsx#L563)                                                                 | 1        |
| `get_external_mcp_status`                     | 未匹配     | [src/components/settings/AiTab.tsx:543](../src/components/settings/AiTab.tsx#L543)                                                                 | 1        |
| `get_file_properties`                         | 服务端入口 | [src/lib/transferDuplicateResolution.ts:23](../src/lib/transferDuplicateResolution.ts#L23)                                                         | 3        |
| `get_groups`                                  | 服务端入口 | [src/pages/TunnelPage.tsx:43](../src/pages/TunnelPage.tsx#L43)                                                                                     | 3        |
| `get_known_hosts`                             | 服务端入口 | [src/components/panel/security-auth/KnownHostsManagementTab.tsx:34](../src/components/panel/security-auth/KnownHostsManagementTab.tsx#L34)         | 1        |
| `get_local_file_properties`                   | 未匹配     | [src/components/dialog/file-explorer/PropertiesDialog.tsx:124](../src/components/dialog/file-explorer/PropertiesDialog.tsx#L124)                   | 1        |
| `get_note`                                    | 未匹配     | [src/components/note-editor/NoteEditor.tsx:83](../src/components/note-editor/NoteEditor.tsx#L83)                                                   | 1        |
| `get_otp_entries`                             | 前端适配   | [src/pages/NewSessionPage.tsx:335](../src/pages/NewSessionPage.tsx#L335)                                                                           | 3        |
| `get_otp_secret_value`                        | 未匹配     | [src/components/panel/security-auth/OtpManagementTab.tsx:133](../src/components/panel/security-auth/OtpManagementTab.tsx#L133)                     | 1        |
| `get_plugin_diagnostics`                      | 未匹配     | [src/lib/plugins.ts:33](../src/lib/plugins.ts#L33)                                                                                                 | 1        |
| `get_plugin_marketplace`                      | 服务端入口 | [src/lib/plugins.ts:13](../src/lib/plugins.ts#L13)                                                                                                 | 1        |
| `get_plugin_probe_scripts`                    | 未匹配     | [src/lib/plugins.ts:21](../src/lib/plugins.ts#L21)                                                                                                 | 1        |
| `get_proxies`                                 | 服务端入口 | [src/pages/ProxyPage.tsx:38](../src/pages/ProxyPage.tsx#L38)                                                                                       | 3        |
| `get_proxy_groups`                            | 服务端入口 | [src/pages/ProxyPage.tsx:37](../src/pages/ProxyPage.tsx#L37)                                                                                       | 2        |
| `get_quick_commands`                          | 服务端入口 | [src/pages/QuickCommandPage.tsx:96](../src/pages/QuickCommandPage.tsx#L96)                                                                         | 4        |
| `get_remote_ascend_npu_overview`              | 未匹配     | [src/hooks/useRemoteNpuOverview.ts:58](../src/hooks/useRemoteNpuOverview.ts#L58)                                                                   | 1        |
| `get_remote_docker_compose_projects`          | 未匹配     | [src/components/panel/DockerManager.tsx:187](../src/components/panel/DockerManager.tsx#L187)                                                       | 1        |
| `get_remote_docker_images`                    | 未匹配     | [src/components/panel/DockerManager.tsx:168](../src/components/panel/DockerManager.tsx#L168)                                                       | 1        |
| `get_remote_docker_networks`                  | 未匹配     | [src/components/panel/DockerManager.tsx:180](../src/components/panel/DockerManager.tsx#L180)                                                       | 1        |
| `get_remote_docker_overview`                  | 未匹配     | [src/components/panel/DockerManager.tsx:129](../src/components/panel/DockerManager.tsx#L129)                                                       | 1        |
| `get_remote_docker_volumes`                   | 未匹配     | [src/components/panel/DockerManager.tsx:173](../src/components/panel/DockerManager.tsx#L173)                                                       | 1        |
| `get_remote_gpu_overview`                     | 未匹配     | [src/hooks/useRemoteGpuOverview.ts:59](../src/hooks/useRemoteGpuOverview.ts#L59)                                                                   | 1        |
| `get_remote_processes`                        | 未匹配     | [src/components/panel/ProcessManager.tsx:196](../src/components/panel/ProcessManager.tsx#L196)                                                     | 1        |
| `get_remote_stats`                            | 未匹配     | [src/lib/connectionAutoIcon.ts:45](../src/lib/connectionAutoIcon.ts#L45)                                                                           | 2        |
| `get_saved_connections`                       | 服务端入口 | [src/pages/TunnelPage.tsx:42](../src/pages/TunnelPage.tsx#L42)                                                                                     | 8        |
| `get_saved_credential_password`               | 服务端入口 | [src/hooks/useCredentialAutofill.ts:194](../src/hooks/useCredentialAutofill.ts#L194)                                                               | 3        |
| `get_saved_credentials`                       | 服务端入口 | [src/hooks/useCredentialAutofill.ts:105](../src/hooks/useCredentialAutofill.ts#L105)                                                               | 2        |
| `get_saved_password_value`                    | 服务端入口 | [src/components/panel/security-auth/PasswordManagementTab.tsx:216](../src/components/panel/security-auth/PasswordManagementTab.tsx#L216)           | 1        |
| `get_saved_passwords`                         | 服务端入口 | [src/pages/NewSessionPage.tsx:338](../src/pages/NewSessionPage.tsx#L338)                                                                           | 7        |
| `get_session_cwd_presentation`                | 未匹配     | [src/lib/dynamicTabTitles.ts:611](../src/lib/dynamicTabTitles.ts#L611)                                                                             | 1        |
| `get_session_info`                            | 服务端入口 | [src/context/AppProvider.tsx:1136](../src/context/AppProvider.tsx#L1136)                                                                           | 1        |
| `get_ssh_agent_forwarding_identities`         | 未匹配     | [src/components/sessions/SshForm.tsx:586](../src/components/sessions/SshForm.tsx#L586)                                                             | 1        |
| `get_ssh_key_passphrase`                      | 服务端入口 | [src/components/panel/security-auth/KeyManagementTab.tsx:132](../src/components/panel/security-auth/KeyManagementTab.tsx#L132)                     | 1        |
| `get_ssh_key_private_key`                     | 服务端入口 | [src/components/panel/security-auth/KeyManagementTab.tsx:261](../src/components/panel/security-auth/KeyManagementTab.tsx#L261)                     | 1        |
| `get_ssh_key_public_key`                      | 服务端入口 | [src/components/panel/security-auth/KeyManagementTab.tsx:274](../src/components/panel/security-auth/KeyManagementTab.tsx#L274)                     | 1        |
| `get_ssh_keys`                                | 服务端入口 | [src/components/sessions/SshForm.tsx:551](../src/components/sessions/SshForm.tsx#L551)                                                             | 3        |
| `get_support_info`                            | 未匹配     | [src/components/dialog/app/AboutDialog.tsx:61](../src/components/dialog/app/AboutDialog.tsx#L61)                                                   | 1        |
| `get_supported_ssh_algorithms`                | 服务端入口 | [src/components/sessions/SshForm.tsx:643](../src/components/sessions/SshForm.tsx#L643)                                                             | 1        |
| `get_system_font_infos`                       | 服务端入口 | [src/components/settings/AppearanceTab.tsx:146](../src/components/settings/AppearanceTab.tsx#L146)                                                 | 1        |
| `get_terminal_cwd`                            | 未匹配     | [src/lib/terminalContext.ts:64](../src/lib/terminalContext.ts#L64)                                                                                 | 2        |
| `get_tunnel_groups`                           | 未匹配     | [src/pages/TunnelPage.tsx:41](../src/pages/TunnelPage.tsx#L41)                                                                                     | 2        |
| `get_tunnel_runtime_states`                   | 未匹配     | [src/components/panel/NetworkPanel.tsx:678](../src/components/panel/NetworkPanel.tsx#L678)                                                         | 1        |
| `get_tunnels`                                 | 前端适配   | [src/pages/TunnelPage.tsx:44](../src/pages/TunnelPage.tsx#L44)                                                                                     | 2        |
| `hide_main_window`                            | 未匹配     | [src/App.tsx:1682](../src/App.tsx#L1682)                                                                                                           | 1        |
| `import_ai_provider_icon`                     | 未匹配     | [src/components/settings/AiTab.tsx:1683](../src/components/settings/AiTab.tsx#L1683)                                                               | 1        |
| `import_config`                               | 未匹配     | [src/hooks/useConfigTransfer.tsx:70](../src/hooks/useConfigTransfer.tsx#L70)                                                                       | 1        |
| `import_connection_icon`                      | 未匹配     | [src/pages/NewSessionPage.tsx:657](../src/pages/NewSessionPage.tsx#L657)                                                                           | 1        |
| `import_keyword_highlight_rules`              | 未匹配     | [src/components/dialog/terminal/KeywordHighlightImportDialog.tsx:56](../src/components/dialog/terminal/KeywordHighlightImportDialog.tsx#L56)       | 1        |
| `import_otp_from_qr`                          | 未匹配     | [src/components/panel/security-auth/OtpManagementTab.tsx:101](../src/components/panel/security-auth/OtpManagementTab.tsx#L101)                     | 1        |
| `import_quick_commands`                       | 未匹配     | [src/components/dialog/quick-commands/QuickCommandsImportDialog.tsx:84](../src/components/dialog/quick-commands/QuickCommandsImportDialog.tsx#L84) | 1        |
| `import_sessions`                             | 未匹配     | [src/components/dialog/connections/ImportDialog.tsx:167](../src/components/dialog/connections/ImportDialog.tsx#L167)                               | 1        |
| `import_ssh_config_hosts`                     | 未匹配     | [src/components/dialog/connections/ImportDialog.tsx:179](../src/components/dialog/connections/ImportDialog.tsx#L179)                               | 1        |
| `import_termius_sessions`                     | 未匹配     | [src/components/dialog/connections/ImportDialog.tsx:166](../src/components/dialog/connections/ImportDialog.tsx#L166)                               | 2        |
| `increment_quick_command_use_count`           | 未匹配     | [src/components/panel/QuickCommands.tsx:483](../src/components/panel/QuickCommands.tsx#L483)                                                       | 1        |
| `inspect_marketplace_plugin`                  | 未匹配     | [src/lib/plugins.ts:15](../src/lib/plugins.ts#L15)                                                                                                 | 1        |
| `inspect_plugin_package`                      | 未匹配     | [src/lib/plugins.ts:37](../src/lib/plugins.ts#L37)                                                                                                 | 1        |
| `install_marketplace_plugin`                  | 未匹配     | [src/lib/plugins.ts:17](../src/lib/plugins.ts#L17)                                                                                                 | 1        |
| `install_plugin_package`                      | 未匹配     | [src/lib/plugins.ts:39](../src/lib/plugins.ts#L39)                                                                                                 | 1        |
| `list_cloud_sync_history`                     | 未匹配     | [src/components/panel/SyncBackupHistoryPanel.tsx:181](../src/components/panel/SyncBackupHistoryPanel.tsx#L181)                                     | 1        |
| `list_local_child_directories`                | 未匹配     | [src/components/panel/file-explorer/FileExplorer.tsx:2146](../src/components/panel/file-explorer/FileExplorer.tsx#L2146)                           | 1        |
| `list_local_dir`                              | 未匹配     | [src/components/panel/file-explorer/FileExplorer.tsx:1246](../src/components/panel/file-explorer/FileExplorer.tsx#L1246)                           | 2        |
| `list_note_tree`                              | 未匹配     | [src/hooks/useNotesTree.ts:67](../src/hooks/useNotesTree.ts#L67)                                                                                   | 1        |
| `list_plugins`                                | 服务端入口 | [src/lib/plugins.ts:36](../src/lib/plugins.ts#L36)                                                                                                 | 1        |
| `list_recording_statuses`                     | 未匹配     | [src/hooks/useSessionRuntimeState.ts:23](../src/hooks/useSessionRuntimeState.ts#L23)                                                               | 1        |
| `list_remote_child_directories`               | 服务端入口 | [src/components/panel/file-explorer/FileExplorer.tsx:2151](../src/components/panel/file-explorer/FileExplorer.tsx#L2151)                           | 1        |
| `list_remote_dir`                             | 服务端入口 | [src/lib/transferDuplicateResolution.ts:36](../src/lib/transferDuplicateResolution.ts#L36)                                                         | 4        |
| `list_serial_ports`                           | 前端适配   | [src/pages/NewSessionPage.tsx:490](../src/pages/NewSessionPage.tsx#L490)                                                                           | 2        |
| `list_sessions`                               | 服务端入口 | [src/context/AppProvider.tsx:1121](../src/context/AppProvider.tsx#L1121)                                                                           | 11       |
| `logout_codex`                                | 未匹配     | [src/components/settings/AiTab.tsx:695](../src/components/settings/AiTab.tsx#L695)                                                                 | 1        |
| `mark_tunnels_disconnected_for_connection`    | 未匹配     | [src/App.tsx:2099](../src/App.tsx#L2099)                                                                                                           | 3        |
| `mark_tunnels_reconnecting_for_connection`    | 未匹配     | [src/App.tsx:2065](../src/App.tsx#L2065)                                                                                                           | 3        |
| `move_note_node`                              | 未匹配     | [src/hooks/useNotesTree.ts:185](../src/hooks/useNotesTree.ts#L185)                                                                                 | 1        |
| `notify_mcp_session_restore_complete`         | 服务端入口 | [src/context/AppProvider.tsx:1265](../src/context/AppProvider.tsx#L1265)                                                                           | 3        |
| `open_child_window`                           | 未匹配     | [src/lib/windowManager.ts:759](../src/lib/windowManager.ts#L759)                                                                                   | 1        |
| `open_download_dir`                           | 未匹配     | [src/components/panel/file-explorer/FileTransfer.tsx:591](../src/components/panel/file-explorer/FileTransfer.tsx#L591)                             | 1        |
| `open_log_dir`                                | 未匹配     | [src/components/layout/Header.tsx:1290](../src/components/layout/Header.tsx#L1290)                                                                 | 2        |
| `open_transfer_target_directory`              | 未匹配     | [src/components/panel/file-explorer/FileTransfer.tsx:599](../src/components/panel/file-explorer/FileTransfer.tsx#L599)                             | 1        |
| `pause_transfer`                              | 未匹配     | [src/context/TransferContext.tsx:791](../src/context/TransferContext.tsx#L791)                                                                     | 1        |
| `plugin_backend_call`                         | 未匹配     | [src/lib/plugins.ts:61](../src/lib/plugins.ts#L61)                                                                                                 | 1        |
| `plugin_host_call`                            | 未匹配     | [src/lib/plugins.ts:59](../src/lib/plugins.ts#L59)                                                                                                 | 1        |
| `poll_github_gist_device_flow`                | 未匹配     | [src/components/settings/SyncBackupTab.tsx:300](../src/components/settings/SyncBackupTab.tsx#L300)                                                 | 1        |
| `prepare_docker_compose_service_logs_command` | 未匹配     | [src/components/panel/DockerManager.tsx:517](../src/components/panel/DockerManager.tsx#L517)                                                       | 1        |
| `prepare_docker_container_logs_command`       | 未匹配     | [src/components/panel/DockerManager.tsx:414](../src/components/panel/DockerManager.tsx#L414)                                                       | 1        |
| `prepare_docker_container_shell_command`      | 未匹配     | [src/components/panel/DockerManager.tsx:430](../src/components/panel/DockerManager.tsx#L430)                                                       | 1        |
| `quit_application`                            | 前端适配   | [src/App.tsx:1656](../src/App.tsx#L1656)                                                                                                           | 1        |
| `rdp_attach_frame_channel`                    | 未匹配     | [src/components/rdp/RdpPaneHost.tsx:238](../src/components/rdp/RdpPaneHost.tsx#L238)                                                               | 1        |
| `rdp_input_batch`                             | 未匹配     | [src/components/rdp/RdpPaneHost.tsx:169](../src/components/rdp/RdpPaneHost.tsx#L169)                                                               | 1        |
| `rdp_resize`                                  | 未匹配     | [src/components/rdp/RdpPaneHost.tsx:389](../src/components/rdp/RdpPaneHost.tsx#L389)                                                               | 1        |
| `rdp_set_keyboard_capture`                    | 未匹配     | [src/components/rdp/RdpPaneHost.tsx:427](../src/components/rdp/RdpPaneHost.tsx#L427)                                                               | 5        |
| `read_background_image_data_url`              | 未匹配     | [src/lib/backgroundImage.ts:78](../src/lib/backgroundImage.ts#L78)                                                                                 | 1        |
| `read_clipboard_file_paths`                   | 前端适配   | [src/lib/clipboard.ts:44](../src/lib/clipboard.ts#L44)                                                                                             | 1        |
| `read_clipboard_path_payload`                 | 前端适配   | [src/lib/clipboard.ts:40](../src/lib/clipboard.ts#L40)                                                                                             | 1        |
| `read_clipboard_text`                         | 前端适配   | [src/lib/clipboard.ts:13](../src/lib/clipboard.ts#L13)                                                                                             | 1        |
| `read_theme_file`                             | 未匹配     | [src/components/dialog/theme/ThemeDesignerDialog.tsx:224](../src/components/dialog/theme/ThemeDesignerDialog.tsx#L224)                             | 1        |
| `rebind_ai_session`                           | 服务端入口 | [src/components/panel/ai/AIAssistantPanel.tsx:1273](../src/components/panel/ai/AIAssistantPanel.tsx#L1273)                                         | 1        |
| `refresh_plugin_monitor`                      | 未匹配     | [src/lib/plugins.ts:31](../src/lib/plugins.ts#L31)                                                                                                 | 1        |
| `register_command_confirmation_candidate`     | 服务端入口 | [src/lib/sessionInput.ts:214](../src/lib/sessionInput.ts#L214)                                                                                     | 1        |
| `register_command_submission`                 | 服务端入口 | [src/lib/sessionInput.ts:164](../src/lib/sessionInput.ts#L164)                                                                                     | 1        |
| `rename_local_file`                           | 未匹配     | [src/components/dialog/file-explorer/MoveDialog.tsx:71](../src/components/dialog/file-explorer/MoveDialog.tsx#L71)                                 | 2        |
| `rename_note_node`                            | 未匹配     | [src/hooks/useNotesTree.ts:178](../src/hooks/useNotesTree.ts#L178)                                                                                 | 1        |
| `rename_remote_file`                          | 服务端入口 | [src/components/dialog/file-explorer/MoveDialog.tsx:76](../src/components/dialog/file-explorer/MoveDialog.tsx#L76)                                 | 2        |
| `reorder_credentials`                         | 未匹配     | [src/components/panel/security-auth/CredentialManagementTab.tsx:335](../src/components/panel/security-auth/CredentialManagementTab.tsx#L335)       | 1        |
| `reorder_items`                               | 未匹配     | [src/components/panel/saved-connections/index.tsx:427](../src/components/panel/saved-connections/index.tsx#L427)                                   | 6        |
| `report_mcp_active_session`                   | 未匹配     | [src/hooks/useMcpActiveSession.ts:8](../src/hooks/useMcpActiveSession.ts#L8)                                                                       | 1        |
| `resize_session`                              | 前端适配   | [src/components/terminal/XTerminal.tsx:1877](../src/components/terminal/XTerminal.tsx#L1877)                                                       | 1        |
| `resolve_cloud_sync_conflict`                 | 未匹配     | [src/components/settings/SyncBackupTab.tsx:1161](../src/components/settings/SyncBackupTab.tsx#L1161)                                               | 4        |
| `resolve_local_directory_children`            | 未匹配     | [src/components/panel/file-explorer/FileExplorer.tsx:3330](../src/components/panel/file-explorer/FileExplorer.tsx#L3330)                           | 1        |
| `resolve_local_drop_paths`                    | 未匹配     | [src/components/terminal/useTerminalExternalDrop.ts:41](../src/components/terminal/useTerminalExternalDrop.ts#L41)                                 | 3        |
| `respond_agent_step`                          | 未匹配     | [src/components/panel/ai/AgentStepView.tsx:237](../src/components/panel/ai/AgentStepView.tsx#L237)                                                 | 2        |
| `respond_external_mcp_approval`               | 未匹配     | [src/components/dialog/app/McpApprovalHost.tsx:54](../src/components/dialog/app/McpApprovalHost.tsx#L54)                                           | 1        |
| `respond_host_key_verify`                     | 服务端入口 | [src/components/dialog/connections/HostKeyVerifyDialog.tsx:40](../src/components/dialog/connections/HostKeyVerifyDialog.tsx#L40)                   | 2        |
| `respond_mcp_session_open`                    | 未匹配     | [src/App.tsx:753](../src/App.tsx#L753)                                                                                                             | 3        |
| `respond_plugin_approval`                     | 未匹配     | [src/lib/plugins.ts:63](../src/lib/plugins.ts#L63)                                                                                                 | 1        |
| `respond_rdp_certificate`                     | 未匹配     | [src/components/dialog/connections/RdpCertificateVerifyDialog.tsx:44](../src/components/dialog/connections/RdpCertificateVerifyDialog.tsx#L44)     | 1        |
| `respond_ssh_agent_auth`                      | 未匹配     | [src/components/dialog/connections/SshAgentAuthDialog.tsx:53](../src/components/dialog/connections/SshAgentAuthDialog.tsx#L53)                     | 1        |
| `respond_transfer_duplicate`                  | 未匹配     | [src/components/dialog/file-explorer/TransferDuplicateDialog.tsx:50](../src/components/dialog/file-explorer/TransferDuplicateDialog.tsx#L50)       | 2        |
| `respond_vnc_server_key`                      | 服务端入口 | [src/components/dialog/connections/VncServerKeyVerifyDialog.tsx:44](../src/components/dialog/connections/VncServerKeyVerifyDialog.tsx#L44)         | 1        |
| `resume_transfer`                             | 未匹配     | [src/context/TransferContext.tsx:544](../src/context/TransferContext.tsx#L544)                                                                     | 2        |
| `reveal_ai_provider_api_key`                  | 未匹配     | [src/components/settings/AiTab.tsx:1302](../src/components/settings/AiTab.tsx#L1302)                                                               | 1        |
| `sanitize_download_file_name`                 | 未匹配     | [src/pages/RemoteFileEditorPage.tsx:306](../src/pages/RemoteFileEditorPage.tsx#L306)                                                               | 3        |
| `save_app_language`                           | 未匹配     | [src/components/layout/Header.tsx:881](../src/components/layout/Header.tsx#L881)                                                                   | 1        |
| `save_app_settings`                           | 服务端入口 | [src/pages/SettingsPage.tsx:390](../src/pages/SettingsPage.tsx#L390)                                                                               | 3        |
| `save_app_ui_settings`                        | 服务端入口 | [src/context/AppProvider.tsx:562](../src/context/AppProvider.tsx#L562)                                                                             | 3        |
| `save_connection`                             | 服务端入口 | [src/pages/NewSessionPage.tsx:1239](../src/pages/NewSessionPage.tsx#L1239)                                                                         | 8        |
| `save_credential`                             | 服务端入口 | [src/components/panel/security-auth/CredentialManagementTab.tsx:217](../src/components/panel/security-auth/CredentialManagementTab.tsx#L217)       | 1        |
| `save_group`                                  | 服务端入口 | [src/pages/NewSessionPage.tsx:973](../src/pages/NewSessionPage.tsx#L973)                                                                           | 6        |
| `save_otp_entry`                              | 未匹配     | [src/components/panel/security-auth/OtpManagementTab.tsx:157](../src/components/panel/security-auth/OtpManagementTab.tsx#L157)                     | 1        |
| `save_password`                               | 服务端入口 | [src/components/panel/security-auth/PasswordManagementTab.tsx:279](../src/components/panel/security-auth/PasswordManagementTab.tsx#L279)           | 1        |
| `save_proxy`                                  | 服务端入口 | [src/pages/ProxyPage.tsx:76](../src/pages/ProxyPage.tsx#L76)                                                                                       | 1        |
| `save_quick_commands`                         | 未匹配     | [src/components/panel/QuickCommands.tsx:364](../src/components/panel/QuickCommands.tsx#L364)                                                       | 1        |
| `save_session_transcript`                     | 未匹配     | [src/hooks/useSessionRecordingActions.ts:102](../src/hooks/useSessionRecordingActions.ts#L102)                                                     | 1        |
| `save_ssh_key`                                | 服务端入口 | [src/components/panel/security-auth/KeyManagementTab.tsx:186](../src/components/panel/security-auth/KeyManagementTab.tsx#L186)                     | 1        |
| `save_tunnel`                                 | 未匹配     | [src/pages/TunnelPage.tsx:116](../src/pages/TunnelPage.tsx#L116)                                                                                   | 1        |
| `serial_modem_upload`                         | 未匹配     | [src/lib/terminalFileDrop.ts:85](../src/lib/terminalFileDrop.ts#L85)                                                                               | 1        |
| `set_app_lock_state`                          | 未匹配     | [src/hooks/useAppLockState.ts:55](../src/hooks/useAppLockState.ts#L55)                                                                             | 1        |
| `set_external_mcp_enabled`                    | 未匹配     | [src/components/settings/AiTab.tsx:529](../src/components/settings/AiTab.tsx#L529)                                                                 | 1        |
| `set_macos_app_menu`                          | 未匹配     | [src/components/layout/Header.tsx:1448](../src/components/layout/Header.tsx#L1448)                                                                 | 1        |
| `set_proxy_group`                             | 服务端入口 | [src/components/panel/NetworkPanel.tsx:750](../src/components/panel/NetworkPanel.tsx#L750)                                                         | 1        |
| `set_recording_memory_limit`                  | 未匹配     | [src/hooks/useSessionRuntimeState.ts:82](../src/hooks/useSessionRuntimeState.ts#L82)                                                               | 1        |
| `set_tunnel_group`                            | 未匹配     | [src/components/panel/NetworkPanel.tsx:762](../src/components/panel/NetworkPanel.tsx#L762)                                                         | 1        |
| `signal_remote_process`                       | 未匹配     | [src/components/panel/ProcessManager.tsx:331](../src/components/panel/ProcessManager.tsx#L331)                                                     | 1        |
| `start_ai_chat_stream`                        | 服务端入口 | [src/components/panel/ai/AIAssistantPanel.tsx:959](../src/components/panel/ai/AIAssistantPanel.tsx#L959)                                           | 1        |
| `start_codex_login`                           | 未匹配     | [src/components/settings/AiTab.tsx:675](../src/components/settings/AiTab.tsx#L675)                                                                 | 1        |
| `start_file_watch`                            | 未匹配     | [src/pages/RemoteFileEditorPage.tsx:319](../src/pages/RemoteFileEditorPage.tsx#L319)                                                               | 3        |
| `start_recording`                             | 未匹配     | [src/hooks/useSessionRecordingActions.ts:68](../src/hooks/useSessionRecordingActions.ts#L68)                                                       | 1        |
| `stop_plugin_backend`                         | 未匹配     | [src/lib/plugins.ts:35](../src/lib/plugins.ts#L35)                                                                                                 | 1        |
| `stop_recording`                              | 未匹配     | [src/hooks/useSessionRecordingActions.ts:49](../src/hooks/useSessionRecordingActions.ts#L49)                                                       | 1        |
| `submit_docker_sudo_password`                 | 未匹配     | [src/components/dialog/docker/DockerSudoPasswordDialog.tsx:53](../src/components/dialog/docker/DockerSudoPasswordDialog.tsx#L53)                   | 1        |
| `submit_otp_response`                         | 服务端入口 | [src/components/dialog/connections/OtpDialog.tsx:68](../src/components/dialog/connections/OtpDialog.tsx#L68)                                       | 1        |
| `submit_ssh_auth_response`                    | 服务端入口 | [src/components/dialog/connections/SshAuthDialog.tsx:245](../src/components/dialog/connections/SshAuthDialog.tsx#L245)                             | 1        |
| `subscribe_plugin_monitor`                    | 未匹配     | [src/lib/plugins.ts:23](../src/lib/plugins.ts#L23)                                                                                                 | 1        |
| `sync_pull_now`                               | 未匹配     | [src/components/settings/SyncBackupTab.tsx:1074](../src/components/settings/SyncBackupTab.tsx#L1074)                                               | 2        |
| `sync_push_now`                               | 未匹配     | [src/components/settings/SyncBackupTab.tsx:1065](../src/components/settings/SyncBackupTab.tsx#L1065)                                               | 3        |
| `terminal_history_search`                     | 未匹配     | [src/hooks/useTerminalSearch.ts:198](../src/hooks/useTerminalSearch.ts#L198)                                                                       | 1        |
| `test_ai_model_connection`                    | 服务端入口 | [src/components/settings/AiTab.tsx:1461](../src/components/settings/AiTab.tsx#L1461)                                                               | 1        |
| `test_ai_provider_connection`                 | 服务端入口 | [src/components/settings/AiTab.tsx:1404](../src/components/settings/AiTab.tsx#L1404)                                                               | 2        |
| `test_cloud_sync_connection`                  | 未匹配     | [src/components/settings/SyncBackupTab.tsx:1054](../src/components/settings/SyncBackupTab.tsx#L1054)                                               | 1        |
| `translate_text`                              | 未匹配     | [src/components/dialog/terminal/TranslationDialog.tsx:44](../src/components/dialog/terminal/TranslationDialog.tsx#L44)                             | 1        |
| `try_get_terminal_cwd`                        | 服务端入口 | [src/lib/terminalZmodemUpload.ts:179](../src/lib/terminalZmodemUpload.ts#L179)                                                                     | 5        |
| `uninstall_plugin`                            | 未匹配     | [src/lib/plugins.ts:54](../src/lib/plugins.ts#L54)                                                                                                 | 1        |
| `unsubscribe_plugin_monitor`                  | 未匹配     | [src/lib/plugins.ts:29](../src/lib/plugins.ts#L29)                                                                                                 | 1        |
| `update_connection_asset_from_monitoring`     | 未匹配     | [src/hooks/useAssetMonitoringCache.ts:83](../src/hooks/useAssetMonitoringCache.ts#L83)                                                             | 1        |
| `update_connection_icon`                      | 未匹配     | [src/lib/connectionAutoIcon.ts:60](../src/lib/connectionAutoIcon.ts#L60)                                                                           | 1        |
| `update_note`                                 | 未匹配     | [src/components/note-editor/NoteEditor.tsx:138](../src/components/note-editor/NoteEditor.tsx#L138)                                                 | 1        |
| `update_remote_file_attributes`               | 未匹配     | [src/components/dialog/file-explorer/PropertiesDialog.tsx:196](../src/components/dialog/file-explorer/PropertiesDialog.tsx#L196)                   | 1        |
| `update_remote_symlink_target`                | 未匹配     | [src/components/dialog/file-explorer/PropertiesDialog.tsx:188](../src/components/dialog/file-explorer/PropertiesDialog.tsx#L188)                   | 1        |
| `upload_clipboard_image_to_ssh`               | 前端适配   | [src/lib/clipboard.ts:50](../src/lib/clipboard.ts#L50)                                                                                             | 1        |
| `upload_local_directory`                      | 未匹配     | [src/context/TransferContext.tsx:644](../src/context/TransferContext.tsx#L644)                                                                     | 1        |
| `upload_local_file`                           | 未匹配     | [src/pages/FileUploadPage.tsx:37](../src/pages/FileUploadPage.tsx#L37)                                                                             | 3        |
| `upsert_quick_command`                        | 未匹配     | [src/pages/QuickCommandPage.tsx:234](../src/pages/QuickCommandPage.tsx#L234)                                                                       | 2        |
| `verify_master_password`                      | 未匹配     | [src/components/panel/security-auth/SecretUnlockFooter.tsx:89](../src/components/panel/security-auth/SecretUnlockFooter.tsx#L89)                   | 2        |
| `vnc_attach_frame_channel`                    | 未匹配     | [src/lib/vncFrames.ts:53](../src/lib/vncFrames.ts#L53)                                                                                             | 1        |
| `vnc_detach_frame_channel`                    | 未匹配     | [src/lib/vncFrames.ts:42](../src/lib/vncFrames.ts#L42)                                                                                             | 1        |
| `vnc_input_batch`                             | 服务端入口 | [src/components/vnc/VncPaneHost.tsx:144](../src/components/vnc/VncPaneHost.tsx#L144)                                                               | 2        |
| `vnc_reconnect`                               | 服务端入口 | [src/components/vnc/VncPaneHost.tsx:408](../src/components/vnc/VncPaneHost.tsx#L408)                                                               | 1        |
| `vnc_set_clipboard_text`                      | 服务端入口 | [src/components/vnc/VncPaneHost.tsx:220](../src/components/vnc/VncPaneHost.tsx#L220)                                                               | 1        |
| `write_bytes_to_session`                      | 前端适配   | [src/lib/sessionInput.ts:179](../src/lib/sessionInput.ts#L179)                                                                                     | 1        |
| `write_clipboard_text`                        | 前端适配   | [src/lib/clipboard.ts:20](../src/lib/clipboard.ts#L20)                                                                                             | 2        |
| `write_theme_file`                            | 未匹配     | [src/components/dialog/theme/ThemeDesignerDialog.tsx:203](../src/components/dialog/theme/ThemeDesignerDialog.tsx#L203)                             | 1        |
| `write_to_session`                            | 前端适配   | [src/lib/sessionInput.ts:170](../src/lib/sessionInput.ts#L170)                                                                                     | 3        |
| `zmodem_accept_download`                      | 未匹配     | [src/components/terminal/zmodemTerminalEvents.ts:297](../src/components/terminal/zmodemTerminalEvents.ts#L297)                                     | 1        |
| `zmodem_accept_upload`                        | 未匹配     | [src/components/terminal/zmodemTerminalEvents.ts:322](../src/components/terminal/zmodemTerminalEvents.ts#L322)                                     | 2        |
| `zmodem_cancel`                               | 未匹配     | [src/lib/terminalZmodemUpload.ts:91](../src/lib/terminalZmodemUpload.ts#L91)                                                                       | 6        |
| `zmodem_pick_download_dir`                    | 未匹配     | [src/components/terminal/zmodemTerminalEvents.ts:294](../src/components/terminal/zmodemTerminalEvents.ts#L294)                                     | 1        |
| `zmodem_pick_upload_files`                    | 未匹配     | [src/components/terminal/zmodemTerminalEvents.ts:330](../src/components/terminal/zmodemTerminalEvents.ts#L330)                                     | 1        |

### 动态命令表达式

这些表达式不包括在 276 个直接字符串命令统计中。通用转发的 `cmd` / `command` 也被记录，不能视为额外业务命令。

| 调用位置                                                                                                                           | 表达式                                                                               |
| ---------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------ |
| [src/pages/RemoteFileEditorPage.tsx:336](../src/pages/RemoteFileEditorPage.tsx#L336)                                               | `tab.backend === "local" ? "open_local_file_text" : "open_remote_file_text"`         |
| [src/pages/RemoteFileEditorPage.tsx:508](../src/pages/RemoteFileEditorPage.tsx#L508)                                               | `tab.backend === "local" ? "write_local_file_text" : "write_remote_file_text"`       |
| [src/context/TransferContext.tsx:602](../src/context/TransferContext.tsx#L602)                                                     | `request.moveSource ? "move_file_entry" : "copy_file_entry"`                         |
| [src/lib/invoke.ts:165](../src/lib/invoke.ts#L165)                                                                                 | `cmd`                                                                                |
| [src/lib/backend/tauri.ts:6](../src/lib/backend/tauri.ts#L6)                                                                       | `...args`                                                                            |
| [src/lib/backend/api.ts:13](../src/lib/backend/api.ts#L13)                                                                         | `command`                                                                            |
| [src/components/terminal/TerminalContextMenu.tsx:240](../src/components/terminal/TerminalContextMenu.tsx#L240)                     | `command`                                                                            |
| [src/components/panel/RecordingPanel.tsx:132](../src/components/panel/RecordingPanel.tsx#L132)                                     | `command`                                                                            |
| [src/components/panel/NetworkPanel.tsx:774](../src/components/panel/NetworkPanel.tsx#L774)                                         | `open ? "open_tunnel" : "close_tunnel"`                                              |
| [src/components/panel/NetworkPanel.tsx:793](../src/components/panel/NetworkPanel.tsx#L793)                                         | `groupDialog.tab === "proxy" ? "save_proxy_group" : "save_tunnel_group"`             |
| [src/components/panel/NetworkPanel.tsx:811](../src/components/panel/NetworkPanel.tsx#L811)                                         | `deleteGroupState.tab === "proxy" ? "delete_proxy_group" : "delete_tunnel_group"`    |
| [src/components/dialog/file-explorer/NewItemDialog.tsx:89](../src/components/dialog/file-explorer/NewItemDialog.tsx#L89)           | `command`                                                                            |
| [src/components/panel/file-explorer/FilePreviewContent.tsx:166](../src/components/panel/file-explorer/FilePreviewContent.tsx#L166) | `command`                                                                            |
| [src/components/panel/file-explorer/FileExplorer.tsx:1693](../src/components/panel/file-explorer/FileExplorer.tsx#L1693)           | `backend === "local" ? "get_local_home_dir" : "get_home_dir"`                        |
| [src/components/panel/file-explorer/FileExplorer.tsx:2906](../src/components/panel/file-explorer/FileExplorer.tsx#L2906)           | `backend === "local" ? "read_local_file_text" : "read_remote_file_text"`             |
| [src/components/panel/file-explorer/FileExplorer.tsx:3456](../src/components/panel/file-explorer/FileExplorer.tsx#L3456)           | `backend === "local" ? "open_local_file_text" : "open_remote_file_text"`             |
| [src/components/panel/file-explorer/FileDocumentEditor.tsx:94](../src/components/panel/file-explorer/FileDocumentEditor.tsx#L94)   | `pane.file.backend === "local" ? "write_local_file_text" : "write_remote_file_text"` |
| [src/components/panel/file-explorer/FileDocumentEditor.tsx:148](../src/components/panel/file-explorer/FileDocumentEditor.tsx#L148) | `pane.file.backend === "local" ? "open_local_file_text" : "open_remote_file_text"`   |
