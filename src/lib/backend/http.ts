import { configureBrowserLogs, createBrowserRequestId } from "./browserDiagnostics";
import { deliver } from "./events";
import { uploadBrowserFile } from "./files";
import { backendURL, requireCapability } from "./runtime";
import { BrowserTerminals } from "./terminal";
import { closeAllBrowserVnc, closeBrowserVnc, sendBrowserVnc } from "./vnc";

let csrf: string | undefined;
let source: EventSource | undefined;
let eventReady: Promise<void> | undefined;
const terminals = new BrowserTerminals(
  (id, signal) => request("api/commands/get_session_info", { sessionId: id }, "POST", signal),
  (id) => request("api/commands/close_session", { sessionId: id }),
);
export class BackendRequestError extends Error {
  constructor(
    message: string,
    public status: number,
    public requestId?: string,
  ) {
    super(message);
  }
}
const nativeCommands =
  /^(create_local_session|create_serial_session|create_rdp_session|.*zmodem.*|.*serial_modem.*|.*local_file.*|.*watcher.*|open_local.*|install_plugin|import_plugin)$/;

export async function request<T>(
  path: string,
  body?: unknown,
  method = body === undefined ? "GET" : "POST",
  signal?: AbortSignal,
  requestId = createBrowserRequestId(),
): Promise<T> {
  const response = await fetch(backendURL(path), {
    method,
    signal,
    credentials: "same-origin",
    headers: {
      "Content-Type": "application/json",
      "X-Nyaterm-Request": "1",
      "X-Nyaterm-Request-Id": requestId,
      ...(csrf ? { "X-Nyaterm-Csrf": csrf } : {}),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const value = await response.json();
  if (!response.ok)
    throw new BackendRequestError(
      value.error ?? `Backend request failed (${response.status})`,
      response.status,
      response.headers.get("X-Nyaterm-Request-Id") ?? requestId,
    );
  return value as T;
}
export async function authenticate(password?: string): Promise<void> {
  const value = await request<{ csrf: string }>(
    password === undefined ? "api/auth/session" : "api/auth/login",
    password === undefined ? undefined : { password },
  );
  csrf = value.csrf;
  configureBrowserLogs(async (entries, keepalive) => {
    const response = await fetch(backendURL("api/logs/frontend"), {
      method: "POST",
      credentials: "same-origin",
      keepalive,
      headers: {
        "Content-Type": "application/json",
        "X-Nyaterm-Request": "1",
        "X-Nyaterm-Request-Id": createBrowserRequestId(),
        ...csrfHeader(),
      },
      body: JSON.stringify({ entries }),
    });
    if (response.status === 401) configureBrowserLogs(undefined);
    if (!response.ok) throw new BackendRequestError("Log upload failed", response.status);
  });
}
export function startEvents(): Promise<void> {
  if (eventReady) return eventReady;
  eventReady = new Promise<void>((resolve, reject) => {
    source = new EventSource(backendURL("api/events"), {
      withCredentials: true,
    });
    const timer = window.setTimeout(() => {
      reject(new Error("Backend event stream unavailable"));
      eventReady = undefined;
      source?.close();
      source = undefined;
    }, 10_000);
    source.onmessage = ({ data }) => {
      const event = JSON.parse(data) as { event: string; payload: unknown };
      if (event.event === "ready") {
        clearTimeout(timer);
        resolve();
      } else if (event.event.startsWith("connection-error-")) {
        const payload = event.payload as { error?: string } | string;
        deliver(
          event.event.replace("connection-error-", "session-error-"),
          typeof payload === "string" ? payload : payload.error,
        );
      } else if (event.event.startsWith("session-closed-")) {
        terminals.stop(event.event.slice("session-closed-".length), true);
      } else deliver(event.event, event.payload);
    };
    source.addEventListener("expired", () => {
      terminals.stopAll();
      closeAllBrowserVnc();
      source?.close();
      window.location.reload();
    });
  });
  return eventReady;
}
export async function httpInvoke<T>(
  command: string,
  args: Record<string, unknown> = {},
  context?: { requestId?: string },
): Promise<T> {
  if (command === "get_default_local_shell") requireCapability("localShell");
  if (nativeCommands.test(command)) requireCapability("nativeFiles");
  if (command === "read_clipboard_text") return (await navigator.clipboard.readText()) as T;
  if (command === "write_clipboard_text") {
    await navigator.clipboard.writeText(String(args.text));
    return undefined as T;
  }
  if (command === "read_clipboard_path_payload") return null as T;
  if (command === "upload_clipboard_image_to_ssh") {
    if (!navigator.clipboard?.read) return null as T;
    const items = await navigator.clipboard.read();
    const item = items.find((entry) => entry.types.includes("image/png"));
    if (!item) return null as T;
    const image = await item.getType("image/png");
    if (image.size > 10 * 1024 * 1024) throw new Error("Clipboard image exceeds 10 MiB");
    const sessionId = String(args.sessionId);
    const directory =
      typeof args.remoteDir === "string"
        ? args.remoteDir
        : await request<string>("api/commands/get_home_dir", { sessionId });
    const path = `${directory.replace(/\/$/, "")}/nyaterm-clipboard-${crypto.randomUUID()}.png`;
    const result = await uploadBrowserFile(sessionId, path, image);
    return (result.status === "skipped" ? null : { remote_path: result.path }) as T;
  }
  if (command === "read_clipboard_file_paths") return [] as T;
  if (
    [
      "claim_external_open_requests",
      "get_tunnels",
      "get_local_shells",
      "list_serial_ports",
    ].includes(command)
  )
    return [] as T;
  if (command === "log_frontend_batch") return undefined as T;
  // Web terminal streams use bounded WebSocket queues, not desktop byte credits.
  if (command === "ack_session_output") return undefined as T;
  const id = String(args.sessionId ?? "");
  switch (command) {
    case "quit_application": {
      terminals.stopAll();
      await request("api/auth/logout", {});
      configureBrowserLogs(undefined);
      source?.close();
      closeAllBrowserVnc();
      window.location.reload();
      return undefined as T;
    }
    case "create_ssh_session":
    case "create_temporary_ssh_session":
    case "create_telnet_session":
    case "create_vnc_session": {
      await startEvents();
      const result = await request<{ session_id: string }>(
        "api/sessions",
        {
          ...args,
          type:
            command === "create_telnet_session"
              ? "telnet"
              : command === "create_vnc_session"
                ? "vnc"
                : "ssh",
        },
        "POST",
        undefined,
        context?.requestId,
      );
      return result.session_id as T;
    }
    case "attach_session":
      await terminals.attach(id);
      return undefined as T;
    case "write_to_session":
      await terminals.send(id, JSON.stringify({ type: "input", data: args.data }));
      return undefined as T;
    case "write_bytes_to_session":
      await terminals.send(id, new Uint8Array(args.data as number[]));
      return undefined as T;
    case "resize_session":
      await terminals.send(
        id,
        JSON.stringify({ type: "resize", cols: args.cols, rows: args.rows }),
      );
      return undefined as T;
    case "vnc_input_batch":
      await sendBrowserVnc(id, { type: "input", events: args.events });
      return undefined as T;
    case "vnc_set_clipboard_text":
      await sendBrowserVnc(id, { type: "clipboard", text: args.text });
      return undefined as T;
    case "close_vnc_session":
      closeBrowserVnc(id);
      break;
    case "close_session": {
      terminals.stop(id);
      break;
    }
  }
  return request<T>(
    `api/commands/${encodeURIComponent(command)}`,
    args,
    "POST",
    undefined,
    context?.requestId,
  );
}
export function csrfHeader(): Record<string, string> {
  return csrf ? { "X-Nyaterm-Csrf": csrf } : {};
}
