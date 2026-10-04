# NyaTerm plugins v1

NyaTerm supports locally installed tool and panel extensions. A plugin can add
sandboxed HTML panels, terminal/connection context-menu commands, and an optional
persistent native backend. Built-in connection protocols remain managed by
NyaTerm. Connection/filesystem providers are future APIs. Reviewed plugins are
distributed through the official Plugin Store.

## Install and use

1. Open the **Plugins** activity panel, choose **Install local package**, and select a `.nyap` file.
2. Review the plugin identity, publisher label and declared permissions. Installation
   validates the package and leaves it disabled.
3. Choose **Enable**, select the permissions to grant, and save. A native backend
   requires explicit `native` approval.
4. Open its panel or command from the Plugins activity item. Panels also appear in
   the activity bar. Declared menu commands appear in terminal/connection menus.

The terminal selected when a panel opens defines its session scope. Selecting
another terminal recreates that scope. A terminal-menu command is scoped to that
terminal; a saved-connection-menu command has no live terminal scope. It cannot
silently access an unrelated active terminal. Opening a panel from a context menu
keeps that explicit session selection until another open intent replaces it.

Command execution and file reading each require a separate approval dialog, even
after granting their manifest permissions. Closing the panel or command dialog,
closing its session/window, locking NyaTerm, or changing plugin activation revokes
the associated scope and cancels host requests. Cancellation cannot undo a command
that has already run on the terminal.

Install a newer package with the same ID to update. Versions are immutable: the
same version cannot be replaced with different contents. Updates and switching
the version selector leave the selected version disabled for review. The old
versions remain available for rollback. Updates, grant changes and uninstall fail
while a plugin request is active; finish or close it, then retry. Disable stops the
native backend. Native processes are also stopped on lock and application exit.

## Trust and storage

UI panels run in an iframe with `sandbox="allow-scripts"`, an opaque origin and
a restrictive response CSP. Scripts, styles, images and fonts are local package
assets; direct network access, child frames, form submissions and popups are
blocked. The bridge accepts messages only from its own frame and binds requests
to a Rust-issued plugin/version/window/session scope. Plugins do not receive an
arbitrary Tauri command gateway. On Windows, where Wry injects initialization
scripts into subframes, the native IPC transport is explicitly initialized only
in the top-level document.

**Native plugins are trusted executables running with your OS user permissions.**
Host permission checks do not sandbox their filesystem, network or subprocess
access. Only enable native packages from a source you trust. SHA-256 checksums
verify consistency and the reviewed package digest prevents a file swap between
inspection and installation; these are not publisher authentication. Publisher
names are self-declared. A changed publisher label cannot overwrite an existing
ID without uninstalling it first, but that continuity check is not a signature.
Local packages may remain unsigned. The official Plugin Store uses repository
Ed25519 signing: `signature.json` signs the exact bytes of `checksums.json`, which
covers every content file (including `manifest.json`) but excludes `checksums.json`
and `signature.json` themselves. Publisher verification and package signatures are
separate concepts; a signature authenticates the Store's reviewed artifact.

The Marketplace downloads and verifies packages in Rust, checks the catalog's final
SHA-256 and size, and cross-checks ID, version, publisher, permissions and signing
key. Public keys are pinned in the client; catalog metadata cannot add trusted keys.
The review dialog shows the verified package before installation, and installation
leaves the plugin disabled until its runtime permissions are separately granted.
Installed versions persist provenance; switching between local and Store sources
requires uninstalling first. Old registry records default to local provenance.

The Store lives in [`nyakang/nyaterm-plugins`](https://github.com/nyakang/nyaterm-plugins).
Each extension has a separate listing, per-target review submissions and per-version
release records. Publisher and signing configuration lives under `trust/`; the
client downloads the generated `public/v1/plugins.json` catalog. The previous
catalog endpoint has been removed without a compatibility mirror, so older
clients must upgrade to load the Store. Deploy the Store's new endpoint before
releasing the updated client. Catalog v1, package formats and installed provenance
remain unchanged.

Its records and signing workflow are independent of author plugin source. First
rollout requires initializing the official key and configuring the protected
`plugin-signing` environment; an empty client trust store rejects every Store install.

The standalone [`@nyaterm/plugin-cli`](../plugins/cli/README.md) package bundles the
authoring binary, SDK and templates, with a workflow to assemble six platforms:

```sh
npm install -g @nyaterm/plugin-cli
nyaterm-plugin create example.hello ./hello
nyaterm-plugin pack ./hello ./hello-1.0.0.nyap
nyaterm-plugin inspect ./hello-1.0.0.nyap
```

Publication to npm is a separate release step. Distributed Rust templates use a
pinned public Git SDK dependency; source-checkout templates retain local paths.

Plugin files live in the active runtime's `plugins/` directory (normally
`~/.nyaterm/plugins/`):

```text
plugins/
  registry.json
  vendor.plugin/versions/1.0.0/...
  data/vendor.plugin/settings.json
```

`storage` holds plaintext, nonsecret JSON preferences, separately for each ID.
Do not store passwords, tokens, terminal commands or terminal output there.
Uninstall removes plugin code and registration; preferences are retained for a
later reinstall. Plugin packages, grants and preferences are excluded from the
existing portable snapshot/backup/cloud-sync payloads.

## GPU monitor

The [GPU monitor example](../plugins/examples/gpu-monitor/README.md) validates
fixed remote probes, shared collection and GPU status contributions. Build it with
`pnpm plugin:example:gpu`, then package it with
`pnpm plugin:pack plugins/examples/gpu-monitor temp/plugins/gpu-monitor.nyap`.
Enable it after reviewing its fixed scripts and approving `native` and `remote.probe`.
Automatic probe grants are removed on updates and version switches.
An authorized `gpu.v1` provider replaces built-in GPU collection and the existing
GPU workspace; disabling it restores the built-in source. Collection failures remain
visible on the plugin source. See the [validation record](gpu-plugin-validation.md).

## Build the UI example

For project generation, Rust SDK APIs, status and logs, see the
[development guide](plugin-development.md) and
[native SDK reference](../plugins/sdk/rust/nyaterm-plugin-sdk/README.md).

From the NyaTerm repository (Node/pnpm and Rust required):

```sh
pnpm plugin:pack plugins/examples/session-toolbox temp/plugins/session-toolbox.nyap
```

Install the generated file, enable it with the permissions you want to try, select
a live terminal and open **Session Toolbox**. Read output, request a command or
request a UTF-8 file read. The latter two actions show the native host approval UI.
Terminal execution uses the existing execution profile and shell capture engine;
sessions that disable execution retain that restriction.

The packager includes `ui/nyaterm-sdk.js` automatically. It is reserved and cannot
be overwritten with different bytes. Only `manifest.json`, `ui/`, `assets/` and
`bin/` are collected, so source code/build artifacts outside these directories do
not enter the archive. UI entry assets must be at most 4 MiB each.

## Manifest

```json
{
  "contributions": {
    "panels": [
      { "id": "overview", "title": "Overview", "entry": "ui/index.html" }
    ],
    "commands": [
      {
        "id": "open-overview",
        "title": "Open Overview",
        "panel": "overview",
        "menus": ["terminal", "connection"]
      }
    ]
  },
  "description": "Tools for a selected terminal",
  "engine": ">=1.2.12, <2.0.0",
  "id": "example.tools",
  "manifestVersion": 1,
  "name": "Example Tools",
  "permissions": ["session.read", "terminal.read", "storage"],
  "publisher": "Example",
  "version": "1.0.0"
}
```

Unknown fields are rejected. IDs use lowercase ASCII letters, digits, `.`, `_`
and `-`, start with a letter/digit and are at most 128 bytes; `..` is prohibited.
Plugin IDs also contain a dot to separate their namespace.
Versions use canonical SemVer; `engine` uses Rust semver requirements, including
comma-separated comparators. Panel entries are `ui/*.html`. A command references
exactly one panel or a declared `command/*` native method. Panel and command IDs
are unique across the contribution set. Optional menus are `terminal` and
`connection`. Panels use persisted `plugin:<pluginId>:<panelId>` activity IDs and
retain their layout when temporarily disabled or unavailable.

Limits: 16 panels, 64 commands, 24 permissions including at most 8 exact HTTPS
origins. Package limits: 128 MiB compressed, 256 MiB expanded, 64 MiB per file and
1024 archive entries. Paths must be portable relative paths; traversal, symlinks,
special files, Windows aliases and case collisions are rejected. `checksums.json`
maps every other archive filename, including the manifest, to its SHA-256 hash.
Use the packager to produce this file rather than maintaining hashes by hand.

## UI SDK and host API

Load classic scripts from local files. Inline styles are allowed; use a local
bundled JS file rather than a CDN or dynamic imports requiring cross-origin access.

```html
<script src="nyaterm-sdk.js"></script>
<script src="app.js"></script>
```

```js
await NyaTerm.ready;
const session = await NyaTerm.session();
const { output } = await NyaTerm.terminal.read(100);
document.getElementById("output").textContent = output;
```

Use `textContent` when rendering terminal/file/network data. Typings are in
`plugins/sdk/nyaterm.d.ts`. SDK operations return promises and reject on denied
permission, invalid scope, cancellation or runtime errors.

| SDK method                                     | Host method / permission                          | Behavior                                                                            |
| ---------------------------------------------- | ------------------------------------------------- | ----------------------------------------------------------------------------------- |
| `session()`                                    | `host/session` / `session.read`                   | Selected `{id,name,type,connected}` or `null`; no credentials                       |
| `terminal.read(lines = 100)`                   | `host/terminal/read` / `terminal.read`            | `{output}`; 1–500 recent lines                                                      |
| `terminal.execute(command, timeoutMs = 30000)` | `host/terminal/execute` / `terminal.execute`      | Individual approval, audit and existing execution engine; 1–120 s execution timeout |
| `filesystem.read(path)`                        | `host/filesystem/read` / `filesystem.read`        | Individual approval; at most 256 KiB UTF-8 text                                     |
| `storage.get(key)`                             | `host/storage/get` / `storage`                    | JSON value or `null`                                                                |
| `storage.set(key, value)`                      | `host/storage/set` / `storage`                    | `null` deletes; otherwise saves nonsecret JSON                                      |
| `network.request(url, options)`                | `host/network/request` / `network:https://origin` | GET/POST, declared exact origin, no redirects, 30 s timeout                         |
| `backend(method, input)`                       | `ui/*` or declared `command/*` / `native`         | Request to persistent backend                                                       |

Execute results contain `output`, `exitCode`, `durationMs`, `timedOut` and
`sourceTruncated`. File reads expose `content` (SSH may return extra file metadata).
Local file paths are confined to the selected session's canonical working
directory. Remote reads require SSH/SFTP; serial and Telnet do not offer file reads.
Storage allows 128 portable-ID keys and 1 MiB serialized JSON per plugin. Network
options are `{method, body, contentType}`; request bodies are capped at 256 KiB and
responses at 1 MiB. Results are `{status, contentType, body}` with text bodies.
URLs cannot contain credentials. HTTPS origins include their nondefault port and
must match exactly; no wildcards, paths or implicit subdomain grants.

`NyaTerm.context` contains plugin identity/version and CSS theme variables;
`NyaTerm.onContextChange(listener)` returns an unsubscribe function. Theme variables
are applied to the iframe document automatically, including `--font-sans` and
`--font-display` (the configured UI font stack) and `--font-mono`. Font changes
are pushed to open panels without reloading them. The SDK defaults to the UI font
and makes form controls inherit it; plugin styles should use
`font-family: var(--font-sans, system-ui, sans-serif)` instead of a fixed family.
Use `var(--font-mono, monospace)` for code. System-installed fonts are available
inside the iframe, but the host's `@font-face` declarations are not inherited;
package any required web fonts with the plugin and declare them in its CSS.
Existing packages with a fixed font family need to update their CSS and be rebuilt.
This context contains no scope
token. Tokens are managed by the parent bridge. Scopes expire after 30 minutes;
open panels renew at 25 minutes by recreating their document. Preserve needed
nonsecret preferences in plugin storage.

UI requests are limited to 32 outstanding calls and 1 MiB request parameters.
Host calls have a 180 s envelope including approval; SDK calls time out at 185 s.
An approval dialog times out at 120 s. A native backend request times out at 170 s,
including any reverse host calls, so long commands and approvals share that budget.

## Native backend protocol

Build and package a working persistent-process example:

```sh
pnpm plugin:example:native
pnpm plugin:pack temp/plugins/native-counter temp/plugins/native-counter.nyap
```

Install and explicitly grant `native`. Invoke **Increment native counter** from
the Plugins panel twice: its count increases in the same process. Disable/re-enable
and invoke again: activation starts a fresh process. The build script generates
a manifest for the current OS/architecture. Production plugins should bundle the
executables needed for each supported platform:

```json
{
  "executables": {
    "windows-x86_64": "bin/backend.exe",
    "linux-x86_64": "bin/backend"
  },
  "transport": "stdio-jsonl"
}
```

Put that object under `backend`, declare the `native` permission, and declare a
command with `"method": "command/count"`. Accepted platforms are Windows, Linux
and macOS on `x86_64` or `aarch64`. Activation is lazy, per plugin; calls reuse the
process and can run concurrently. stdout is exclusively protocol traffic; write
diagnostics to stderr. Environment inheritance is restricted to PATH, OS/temp and
locale values, plus `NYATERM_PLUGIN_ID`, `NYATERM_PLUGIN_VERSION`, `NYATERM_APP_VERSION`.
The working directory is the immutable installed version directory.

The host first sends a JSON-RPC 2.0 request:

```json
{
  "id": 1,
  "jsonrpc": "2.0",
  "method": "plugin/initialize",
  "params": {
    "pluginId": "example.tools",
    "pluginVersion": "1.0.0",
    "appVersion": "1.2.12",
    "apiVersion": 1,
    "protocolVersion": 1,
    "permissions": ["native"]
  }
}
```

Reply within 10 seconds with matching identity/version/protocol:

```json
{
  "id": 1,
  "jsonrpc": "2.0",
  "result": {
    "pluginId": "example.tools",
    "pluginVersion": "1.0.0",
    "protocolVersion": 1
  }
}
```

Normal host requests have numeric IDs and params `{scopeToken, input}`. Respond
with the same numeric ID and `result` or `error:{code,message}`. Backends may make
reverse host requests using a **string ID**, one of the host methods in the table,
and params `{scopeToken, input}`. They must propagate the token associated with the
specific call, not keep a global "current session". Host enforcement checks the
token's plugin, live session/window, version, grants and lock state. Other plugins'
tokens are rejected. Backends should handle `$/cancelRequest` notifications for
timed-out numeric host requests. Closing a scope stops waiting for its response;
native code must arrange its own cooperative work cancellation.

`stdio-jsonl` is one UTF-8 JSON object per newline. `stdio-framed` uses:

```text
kind: u8 | payload length: u32 little-endian | payload
kind 1: UTF-8 JSON-RPC
kind 2: channel length: u16 little-endian | UTF-8 channel | raw bytes
```

Frames are capped at 8 MiB; JSON at 2 MiB; channel names at 128 bytes. The runtime
supports notification/binary subscriptions for future provider integration, but
v1 browser APIs expose request/response calls only. There is no public UI binary
stream or backend event subscription. Do not use those frames to implement a UI
API yet. The runtime bounds queues and pending requests and terminates the child
on protocol failures, failed handshake, disable/update/uninstall, lock or exit.
A crashed backend is activated again on the next call.

## Maintaining the host

The independent runtime lives in `src-tauri/crates/nyaterm-plugin-runtime`; Tauri
adapters and permission checks live in `src-tauri/src/core/plugins`. Plugin UI
bridge code and the SDK are separately tested.

`ipc_transport.js` and `process_ipc_message.js` preserve Tauri 2.11.5 transport
semantics, with a top-frame guard before initializing the invoke key. When
upgrading Tauri, compare these files with its upstream scripts and rerun the IPC
isolation/serialization tests. Do not remove the guard or add `allow-same-origin`
to plugin iframes to work around compatibility issues.

On Windows, the full Tauri library test executable may need a Common Controls v6
activation manifest to load its native UI dependencies. An entrypoint-not-found
error before any tests run is a loader failure; check the executable manifest
rather than interpreting it as a failed plugin assertion.

```sh
pnpm exec vitest run
pnpm build
pnpm lint
cargo test --manifest-path src-tauri/crates/nyaterm-plugin-runtime/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --lib core::plugins
```

For Store/client interoperability, set `NYATERM_STORE_CATALOGS` to the generated
public catalog and any populated fixture paths (separated by `;` on Windows or
`:` on Unix). The runtime's `store_catalog` test parses each with the client's
catalog validator. See the Store README for exporting the populated fixture.
