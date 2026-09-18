# Agent-execution-policy-dev

## 目标

Agent 负责：
- 阅读代码
- 修改代码
- 静态分析
- 低副作用检查

Agent 不负责：
- 启动桌面应用
- 构建安装包
- 用户环境验证


## 默认禁止执行

未经用户当前请求明确授权，不得执行明确UI功能测试、生产产物的命令：

### Node / pnpm
- pnpm install
- pnpm update
- pnpm add/remove
- pnpm build
- pnpm release
- pnpm tauri dev
  ...

### Tauri
- pnpm tauri dev
- pnpm tauri build
  ...

### Rust
- cargo build
- cargo check
- cargo test
- cargo clippy
  ...


规则：
- 不间接触发上述命令
- 不自动重试
- 失败后停止并报告


## 回复要求

完成修改后：
- 描述修改内容
- 列出建议验证命令
- 不声称 build/test 已通过
