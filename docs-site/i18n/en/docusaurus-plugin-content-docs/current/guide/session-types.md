---
sidebar_position: 0
---

# Session Types

NyaTerm is not just an SSH client. It is a desktop app that puts multiple terminal and remote-desktop workflows into one workspace. It currently supports six session types:

- **SSH**
- **Local Terminal**
- **Telnet**
- **Serial**
- **RDP**
- **VNC**

Understanding the differences helps explain why some panels or enhancements only appear for certain tabs.

## At a glance

| Session Type | Typical scenario | Key capabilities |
|--------------|------------------|------------------|
| SSH | Remote Linux / Unix administration | SFTP, OTP, resource / GPU / process / Docker monitoring, proxy, jump host, tunnels, algorithm preferences |
| Local Terminal | Local shell work, scripts, builds | Shared terminal UI, command history, split panes |
| Telnet | Legacy devices, lab environments, compatibility troubleshooting | Terminal workspace features with `Backspace Mode`, but not SSH-only features |
| Serial | Routers, switches, boards, embedded debug ports | Serial port settings, `Backspace Mode`, and terminal workspace features |
| RDP | Windows Remote Desktop or graphical administration entry points | Remote desktop display, NLA/CredSSP, certificate verification, proxy / SSH jump host, text clipboard, window fitting, and reconnects |
| VNC | Raw TCP VNC services, VM consoles, lightweight graphical remote desktops | Raw / ZRLE / Tight / Tight JPEG display, None / VNC Auth, proxy / SSH jump host, window scaling, text clipboard, and reconnects |

## SSH

SSH is the most capable session type in NyaTerm. It is the best fit when you need to:

- Log in to remote Linux / Unix hosts
- Browse and transfer remote files
- Use OTP, jump hosts, or proxies
- Watch remote resource, GPU, process, and Docker monitoring
- Configure port tunnels
- Fine-tune negotiated SSH algorithms

If you need any of these, use **SSH** first:

- File explorer
- Auto-upload / round-trip editing
- Remote resource monitoring
- SSH tunnels in the Network panel

## Local Terminal

Local Terminal is useful when you want your local shell workflow inside the same NyaTerm workspace, for example:

- Running frontend or Rust builds locally
- Running scripts, reading logs, or using Git
- Comparing local and remote output side by side

Its value is not remote access. Its value is that it shares the same workspace model as SSH sessions:

- Tabs
- Split panes
- Terminal search
- Command history and suggestions
- Optional line numbers, timestamps, and highlighting

When creating a local terminal, you can also choose:

- The shell path, such as `powershell.exe`, `cmd.exe`, `bash`, or `wsl.exe`
- The working directory

## Telnet

Telnet is useful for:

- Maintaining older equipment
- Lab environments
- Compatibility scenarios where SSH is not available

You still get NyaTerm's terminal workspace model, but not SSH-specific security or file features. In practice, that usually means no:

- SFTP file explorer
- OTP binding
- SSH jump host
- SSH resource monitoring

If your goal is simply to open a traditional remote terminal quickly, Telnet can be the more direct choice.

For devices that expect specific erase behavior, Telnet also exposes `Backspace Mode` so you can choose `Ctrl+H (BS)` or `DEL (0x7F)`.

## Serial

Serial sessions are useful for connecting to:

- Network device console ports
- Routers and switches
- Development boards, embedded devices, and debug ports

When creating a serial session, you can configure:

- Port
- Baud rate
- Data bits
- Parity
- Stop bits
- `Backspace Mode`

Serial sessions still live inside NyaTerm's tabbed and split workspace, so you can watch serial output in one pane while running commands in an SSH or local terminal pane.

## RDP

On Windows desktop, choose the default RDP client in **Settings → Connections → Remote Desktop**. The initial default is NyaTerm built-in RDP. When creating or editing an RDP connection, **Open with** can follow the default or select either built-in RDP or Windows Remote Desktop. Existing connections continue to follow your previous global preference.

The saved RDP connection context menu also offers **Open with built-in RDP** and **Open with Windows Remote Desktop**. These actions apply only to that connection's current launch and do not change saved preferences, even with multiple connections selected. Batch launches use each connection's preference.

Windows Remote Desktop opens in a separate system window and receives only the address and port. Credentials, domain, proxies, SSH jump hosts, and other options are managed by the system client. Parameters saved in NyaTerm remain available for built-in RDP. Existing built-in sessions keep using built-in RDP for reconnects, splits, and startup restoration. Other desktop platforms use built-in RDP; Web does not offer RDP.

RDP sessions are for Windows hosts or other environments that expose a Remote Desktop endpoint. They share NyaTerm's saved-connection, tab, and split-pane workspace model, but the underlying session is a graphical desktop instead of a text terminal.

When creating an RDP session, you can configure:

- Host, port, username, password, and domain
- Network Level Authentication (NLA / CredSSP)
- Certificate policy: ask on unknown certificates, strict rejection, or accept for this session
- Network: a saved proxy or SSH jump host
- Display mode: fit to window or fixed size
- Text clipboard mode
- Automatic reconnect attempts

When connecting to an RDP host with an unknown certificate, NyaTerm opens a certificate verification dialog. You can accept the certificate for the current connection only or accept and remember it. If a remembered certificate changes later, NyaTerm prompts again before connecting.

RDP does not provide terminal command history, the SFTP file explorer, or remote resource monitoring. If you need command-line enhancements, use SSH, Local Terminal, Telnet, or Serial instead.

## VNC

VNC sessions are for traditional RFB / VNC services such as VM consoles, lab environments, and lightweight graphical desktops. Like RDP, they use a remote-desktop pane and share NyaTerm's saved-connection, recent-use, tab, and split-pane workspace model.

When creating a VNC session, you can configure:

- Host and port
- Security mode: automatic, None, or classic VNC Authentication
- Network: a saved proxy or SSH jump host
- Display mode: fit to window, actual size, or stretch
- Text clipboard toggle
- Automatic reconnect attempts
- Shared / view-only behavior

The VNC protocol layer has no TLS / VeNCrypt support, but its underlying TCP connection can be established through a saved SOCKS5 / HTTP / ProxyCommand proxy or SSH jump host. Classic VNC Authentication passwords are limited to 8 bytes; NyaTerm rejects longer passwords instead of truncating them. Framebuffer encodings are advertised by default as `DesktopSizePseudo`, ZRLE, Tight, then Raw; Tight JPEG is decoded in the backend into the same RGBA framebuffer path, and Raw remains the stable fallback. CopyRect, cursor pseudo-encoding, and remote resize are not supported. Text clipboard exchange is limited to Latin-1 text so binary or oversized payloads do not enter the VNC protocol path.

### VNC Interop Matrix

| Scenario | Security | Encoding | Status |
| --- | --- | --- | --- |
| Scripted RFB 3.8 fixture | None | ZRLE / Tight / Tight JPEG -> RGBA RawImage | Automated test passed |
| Scripted RFB 3.8 fixture | classic VNC Auth | ZRLE / Tight / Tight JPEG -> RGBA RawImage | Automated test passed |
| TigerVNC | None / VNC Auth | Raw / ZRLE / Tight / JPEG | Real server untested |
| TightVNC | None / VNC Auth | Raw / Tight / JPEG | Real server untested |
| x11vnc / LibVNCServer | None / VNC Auth | Raw / ZRLE / Tight / JPEG | Real server untested |
| QEMU / KVM VNC | None / VNC Auth | Raw / ZRLE / Tight / JPEG | Real server untested |

## How to choose

A simple rule of thumb:

- Need the full remote workflow? Use **SSH**
- Need a local shell? Use **Local Terminal**
- Need a traditional remote terminal? Use **Telnet**
- Need a device console or debug port? Use **Serial**
- Need a graphical Windows remote desktop? Use **RDP**
- Need a VNC / VM console graphical desktop? Use **VNC**

## Mix them in one workspace

One of NyaTerm's strengths is that you can mix these session types in the same workspace, for example:

- SSH on the left to watch remote logs
- Local Terminal on the right to run packaging or Git commands
- A Serial tab open to watch device boot output
- An RDP pane open to inspect a Windows remote desktop
- A VNC pane open to operate a VM console

That is why some features are documented as session-specific. The workspace is shared, but the capability boundary still depends on the underlying session type.

:::tip Screenshot suggestion
- Suggested image path: `/img/docs/session-types/new-session-tabs.png`
- Show the SSH / Local Terminal / Telnet / Serial tabs in the new-session window
- Keeping the default field areas visible helps readers understand the differences
:::
