# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Development commands

### Root app

- `pnpm install` — install JS dependencies
- `pnpm dev` — run the Vite frontend only
- `pnpm tauri dev` — run the full desktop app in Tauri dev mode
- `pnpm build` — build the desktop frontend
- `pnpm build:web` — type-check and build the frontend for Web deployment
- `pnpm tauri build` — build the production desktop bundle
- `pnpm test` — run frontend Vitest tests
- `pnpm lint` — run frontend lint checks
- `pnpm format` — apply frontend formatting
- `pnpm format:check` — check frontend formatting
- `pnpm i18n:check` — check locale JSON formatting
- `pnpm i18n:fix` — rewrite locale JSON formatting
- `pnpm i18n:keys` — check locale key consistency
- `pnpm version-sync` — sync version numbers across app files
- `pnpm release` — version sync + production Tauri build

### Rust / Tauri backend

- `cargo fmt --manifest-path src-tauri/Cargo.toml` — format Rust code
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets` — lint Rust code
- `cargo test --manifest-path src-tauri/Cargo.toml` — run desktop backend tests
- `cargo test --manifest-path src-tauri/Cargo.toml <test_name>` — run a single backend test
- `cargo test --manifest-path src-tauri/crates/otp/Cargo.toml` — run OTP crate tests

### Web deployment

- `pnpm web:build:server` — build the Web Rust server
- `pnpm web:serve` — run the Web server locally
- `pnpm test:web` — run Web backend tests
- `pnpm test:web:e2e` — run Web deployment E2E tests

Web deployment details are documented in:

- `docs/web-deployment.md`
- `deploy/web/README.md`

### Docs site

- `pnpm --dir docs-site start` — serve all locales
- `pnpm --dir docs-site start:zh` — run zh-CN docs dev server
- `pnpm --dir docs-site start:en` — run English docs dev server
- `pnpm --dir docs-site start:ko` — run Korean docs dev server
- `pnpm --dir docs-site build` — build the docs site

## Big-picture architecture

NyaTerm is primarily a Tauri 2 desktop application built with React, TypeScript, and Rust.

The repository also supports a Web deployment runtime. The React frontend is shared, while backend access is adapted per runtime.

```text
React / TypeScript
        |
src/lib/backend/api.ts
        |
   +----+----------------+
   |                     |
Desktop                Web
   |                     |
Tauri IPC          HTTP / SSE / WS
   |                     |
src-tauri/src/      nyaterm-web
   |                     |
   +------ shared -------+
          nyaterm-core
```

For ordinary Desktop-specific work, keep the existing Tauri architecture and do not over-generalize solely for Web.

When changing shared frontend backend abstractions or shared Rust core logic, consider both runtimes.

### Frontend backend abstraction

Shared frontend code should normally access backend functionality through `src/lib/backend/`.

Important files:

- `src/lib/backend/api.ts` — common `invoke`, `listen`, and `emit` entry point
- `src/lib/backend/runtime.ts` — runtime detection and capability checks
- `src/lib/backend/tauri.ts` — Desktop/Tauri adapter
- `src/lib/backend/http.ts` — Web HTTP/SSE/WebSocket adapter
- `src/lib/backend/platform/` — runtime-specific platform API wrappers

Avoid adding scattered direct `@tauri-apps/api` calls where an existing shared adapter should be used.

Do not assume a shared React component implies identical Desktop and Web capabilities. Check `runtime.ts` before changing capability gating.

### Desktop backend

The main Desktop backend lives under:

```text
src-tauri/src/
```

Tauri commands are registered centrally in `src-tauri/src/lib.rs` and grouped by concern under `src-tauri/src/cmd/`.

Desktop-specific functionality includes areas such as:

- native windows
- Local Shell
- Serial
- RDP
- tray
- native file integration
- global shortcuts
- updater
- OS-specific integrations

### Shared Rust core

Reusable backend/domain logic shared by Desktop and Web is located under:

```text
src-tauri/crates/nyaterm-core/
```

When backend behavior genuinely applies to both runtimes, prefer sharing it through `nyaterm-core` instead of duplicating implementations.

Do not move Desktop-only behavior into the shared core merely for architectural symmetry.

### Web backend

The Web server lives under:

```text
src-tauri/crates/nyaterm-web/
```

It provides browser-compatible backend access using HTTP, SSE, and WebSocket transports.

Web support is intentionally a subset of the Desktop application. Do not assume every Desktop capability has or needs an equivalent Web implementation.

## Window model

`src/main.tsx` decides between the main application and child-page/window flows.

The main application uses the normal app provider and `App.tsx`.

Child flows use `ChildAppProvider` and `ChildWindowRouter`.

Desktop child windows are coordinated through `src/lib/windowManager.ts`.

Native Tauri window APIs are Desktop-specific. When changing shared window-related code, ensure browser mode does not accidentally call native window APIs.

## Frontend state model

`src/context/AppContext.tsx` and `src/context/AppProvider.tsx` are central to application state.

They cover areas such as:

- tab/session workspace state
- UI and application settings
- saved connections and groups
- startup workspace restoration

`src/context/ChildAppProvider.tsx` is the lighter provider used by child-page/window flows.

`src/context/TransferContext.tsx` tracks file transfer state and backend transfer notifications.

## Workspace / terminal model

The terminal workspace has two distinct layers:

- `src/lib/workspaceTabs.ts` manages the persistent logical tab and pane model
- `src/lib/tabWindows.ts` manages the runtime multi-tab / split-window layout

Do not confuse persistent workspace state with runtime terminal layout.

`src/App.tsx` is the main application shell.

`src/components/terminal/XTerminal.tsx` is the central xterm.js integration point.

Terminal issues may involve:

```text
transport
-> backend session I/O
-> IPC / Web transport
-> frontend event handling
-> buffering / backpressure
-> xterm rendering
-> user interaction
```

Do not attribute terminal performance or correctness problems to the network, Rust, WebView, or xterm without tracing the actual path.

## Backend runtime model

Desktop backend state and managers are constructed in `src-tauri/src/lib.rs`.

Backend functionality is grouped under `src-tauri/src/core/` and related command modules.

Relevant areas include:

- SSH
- SFTP
- Local PTY
- Telnet
- Serial
- RDP
- VNC
- tunnels
- recording
- AI
- monitoring
- sync / backup
- plugins
- storage

When investigating async Rust behavior, check:

- task ownership
- channels
- cancellation
- locks
- reconnect
- shutdown
- session lifecycle

## SSH / auth / transfer details

SSH-related work should consider:

- connection lifecycle
- authentication
- saved credentials
- keyboard-interactive / OTP
- host-key verification
- known hosts
- proxies
- jump hosts
- tunnels
- SFTP
- reconnect
- session ownership

Do not bypass host-key verification or credential encryption.

File-transfer behavior differs between Desktop and Web. Desktop may use native paths, file watchers, native dialogs, and child windows, while Web uses browser-compatible upload/download and internal editing flows.

## AI Assistant

The AI Assistant includes Ask and Agent-style workflows.

Relevant areas include:

- streaming
- provider compatibility
- terminal context
- session mentions
- command cards
- command risk levels
- approvals
- approved execution
- cancellation
- history
- audit
- prompt redaction

High-risk commands must continue to use the existing approval mechanism.

Web currently supports a more limited AI capability set than Desktop. Do not expose Desktop Agent/MCP behavior in Web merely because shared UI is available.

## Persistence model

Desktop data normally lives under:

```text
~/.nyaterm/
```

The primary store is `nyaterm.redb`.

Sensitive values are encrypted before storage.

When changing persistence, consider:

- backward compatibility
- migrations
- backup/import/export
- cloud-sync snapshots
- existing legacy migration paths

Web uses its own data directory configured through `NYATERM_WEB_DATA_DIR`. Do not assume Web data lives under the Desktop user's `~/.nyaterm/`.

Security-sensitive code must not:

- store passwords or secrets in plaintext
- log credentials or private keys
- bypass master-password or server-key protections
- weaken host-key verification
- break compatibility with existing encrypted data

## Plugins

Plugin-related code spans:

```text
plugins/
src/components/plugins/
src/lib/plugins.ts
src/lib/pluginBridge.ts
src/lib/pluginIpc.ts
src-tauri/crates/nyaterm-plugin-runtime/
```

Web currently supports only a subset of plugin functionality.

Do not assume native plugin installation or execution is browser-compatible.

## Project-specific guidance

### UI

Prefer existing shadcn/ui patterns and components.

Shared UI components live under:

```text
src/components/ui/
```

### Internationalization

When changing user-facing text, update all supported locale files:

- `src/i18n/locales/en.json`
- `src/i18n/locales/zh-CN.json`
- `src/i18n/locales/zh-TW.json`
- `src/i18n/locales/ko.json`

### Frontend tests

The root frontend uses Vitest.

Tests are colocated throughout `src/` using `*.test.ts` and `*.test.tsx`.

Add or update focused tests when changing reusable frontend logic.

### Frontend linting

`pnpm lint` includes the repository no-console check.

Prefer the existing logger infrastructure over direct `console.*` calls.

### Vite

Vite uses the `@` alias for `src/`.

### Docs

There is a separate Docusaurus app under `docs-site/`.

Useful implementation documentation is under `docs-site/docs/development/`.

For Web deployment behavior, prefer the current implementation and `docs/web-deployment.md` over assumptions based on older architecture notes.

## Code modification principles

This is a long-lived project.

When modifying code:

- understand the current implementation before changing it
- prefer small, targeted changes
- avoid unrelated refactors
- preserve existing module boundaries
- reuse existing abstractions
- avoid introducing complex architecture merely to remove minor duplication
- consider backward compatibility for config, databases, backups, and persisted state
- consider Windows, macOS, and Linux differences
- preserve Desktop behavior when changing shared Web-facing code
- consider both runtimes when modifying shared backend abstractions or `nyaterm-core`
- consider cancellation, teardown, and ownership in async Rust
- do not remove compatibility logic without understanding why it exists

## Validation principles

Choose the smallest meaningful validation set.

Frontend changes may use:

```sh
pnpm test
pnpm lint
pnpm build:web
```

Desktop Rust changes may use:

```sh
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml <relevant_test>
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets
```

Web backend changes may use:

```sh
pnpm test:web
pnpm build:web
```

Use `pnpm test:web:e2e` when the change affects actual Web deployment, transport, or browser integration.

Do not run the heaviest full build by default for every small change.

If validation fails because of the environment or an unrelated existing issue, report what was run, what passed, what failed, and whether the failure appears related to the change.

Do not claim tests or builds passed unless they were actually executed successfully.

## Source analysis principles

For implementation questions:

1. identify whether the problem belongs to React UI, frontend state, backend adapter, Tauri IPC, Web transport, Rust backend, transport, or persistence
2. find the actual entry point and call sites
3. read the real implementation before drawing conclusions
4. follow the actual call chain
5. inspect Git history when compatibility logic is involved
6. for async code, inspect task ownership, channels, locks, cancellation, and lifecycle
7. do not invent files, functions, types, commands, or behavior from names alone

Typical Desktop flow:

```text
User action
-> React component / hook
-> backend adapter
-> Tauri command / event
-> Rust backend
-> transport / storage
-> frontend state update
-> UI
```

Typical Web flow:

```text
User action
-> React component / hook
-> backend adapter
-> HTTP / SSE / WebSocket
-> nyaterm-web
-> shared core / service
-> frontend state update
-> UI
```

Always verify the actual implementation in the current source tree.

## Git conventions

Repository history generally follows Conventional Commit-style subjects such as:

```text
feat:
fix:
perf:
refactor:
test:
chore:
docs:
```

Keep commits focused and consistent with recent repository history.