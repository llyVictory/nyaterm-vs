import { backendURL, requireCapability } from "./runtime";
import { deliver } from "./events";
import { closeBrowserVnc, closeAllBrowserVnc, sendBrowserVnc } from "./vnc";

let csrf: string | undefined;
let source: EventSource | undefined;
let eventReady: Promise<void> | undefined;
const sockets = new Map<
  string,
  { socket: WebSocket; ready: Promise<void>; closed: boolean }
>();
const nativeCommands =
  /^(create_local_session|create_serial_session|create_rdp_session|.*zmodem.*|.*serial_modem.*|.*local_file.*|.*watcher.*|open_local.*|install_plugin|import_plugin)$/;

export async function request<T>(
  path: string,
  body?: unknown,
  method = body === undefined ? "GET" : "POST",
): Promise<T> {
  const response = await fetch(backendURL(path), {
    method,
    credentials: "same-origin",
    headers: {
      "Content-Type": "application/json",
      "X-Nyaterm-Request": "1",
      ...(csrf ? { "X-Nyaterm-Csrf": csrf } : {}),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const value = await response.json();
  if (!response.ok)
    throw new Error(
      value.error ?? `Backend request failed (${response.status})`,
    );
  return value as T;
}
export async function authenticate(password?: string): Promise<void> {
  const value = await request<{ csrf: string }>(
    password === undefined ? "api/auth/session" : "api/auth/login",
    password === undefined ? undefined : { password },
  );
  csrf = value.csrf;
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
      } else deliver(event.event, event.payload);
    };
    source.addEventListener("expired", () => window.location.reload());
  });
  return eventReady;
}
async function attach(id: string): Promise<void> {
  const previous = sockets.get(id);
  if (previous && !previous.closed) return previous.ready;
  const url = backendURL(`api/sessions/${encodeURIComponent(id)}/terminal`);
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  const socket = new WebSocket(url);
  socket.binaryType = "arraybuffer";
  const decoder = new TextDecoder();
  const state = { socket, ready: Promise.resolve(), closed: false };
  let opened = false;
  let rejectReady: (reason: Error) => void = () => {};
  state.ready = new Promise<void>((resolve, reject) => {
    rejectReady = reject;
    socket.onopen = () => {
      opened = true;
      resolve();
    };
    socket.onerror = () => reject(new Error("Terminal WebSocket unavailable"));
  });
  socket.onmessage = ({ data }) => {
    if (typeof data !== "string") {
      // Stream decoding preserves UTF-8 characters split across SSH packets.
      try {
        deliver(`terminal-output-${id}`, {
          data: decoder.decode(data, { stream: true }),
          bytes: data.byteLength,
        });
      } catch {
        state.closed = true;
        socket.close();
        void request("api/commands/close_session", { sessionId: id }).catch(
          () => {},
        );
      }
    } else {
      const event = JSON.parse(data);
      if (event.type === "error") deliver(`session-error-${id}`, event.error);
      if (event.type === "closed" || event.type === "error") {
        state.closed = true;
        deliver(`session-closed-${id}`, null);
        socket.close();
      }
    }
  };
  socket.onclose = () => {
    if (!opened) {
      state.closed = true;
      rejectReady(new Error("Terminal WebSocket closed"));
      return;
    }
    if (state.closed) {
      if (sockets.get(id) === state) sockets.delete(id);
      return;
    }
    state.closed = true;
    // A transport interruption has a bounded server lease. Attempt a same-session
    // attachment; an explicit reconnect in the UI still creates a new SSH session.
    window.setTimeout(() => {
      if (sockets.get(id) !== state) return;
      void attach(id).catch(() => deliver(`session-closed-${id}`, null));
    }, 1000);
  };
  sockets.set(id, state);
  return state.ready;
}
async function terminalSend(
  id: string,
  message: string | Uint8Array,
): Promise<void> {
  await attach(id);
  const socket = sockets.get(id)?.socket;
  if (!socket || socket.readyState !== WebSocket.OPEN)
    throw new Error("Terminal is disconnected");
  if (socket.bufferedAmount > 1024 * 1024)
    throw new Error("Terminal input queue is full");
  socket.send(message);
}
export async function httpInvoke<T>(
  command: string,
  args: Record<string, unknown> = {},
): Promise<T> {
  if (command === "get_default_local_shell") requireCapability("localShell");
  if (nativeCommands.test(command)) requireCapability("nativeFiles");
  if (command === "read_clipboard_text")
    return (await navigator.clipboard.readText()) as T;
  if (command === "write_clipboard_text") {
    await navigator.clipboard.writeText(String(args.text));
    return undefined as T;
  }
  if (
    command === "read_clipboard_path_payload" ||
    command === "upload_clipboard_image_to_ssh"
  )
    return null as T;
  if (command === "read_clipboard_file_paths") return [] as T;
  if (
    [
      "claim_external_open_requests",
      "get_tunnels",
      "get_otp_entries",
      "get_local_shells",
      "list_serial_ports",
    ].includes(command)
  )
    return [] as T;
  if (command === "log_frontend_batch") return undefined as T;
  const id = String(args.sessionId ?? "");
  switch (command) {
    case "quit_application": {
      await request("api/auth/logout", {});
      source?.close();
      for (const active of sockets.values()) {
        active.closed = true;
        active.socket.close();
      }
      sockets.clear();
      closeAllBrowserVnc();
      window.location.reload();
      return undefined as T;
    }
    case "create_ssh_session":
    case "create_temporary_ssh_session":
    case "create_telnet_session":
    case "create_vnc_session": {
      await startEvents();
      const result = await request<{ session_id: string }>("api/sessions", {
        ...args,
        type:
          command === "create_telnet_session"
            ? "telnet"
            : command === "create_vnc_session"
              ? "vnc"
              : "ssh",
      });
      return result.session_id as T;
    }
    case "attach_session":
      await attach(id);
      return undefined as T;
    case "write_to_session":
      await terminalSend(
        id,
        JSON.stringify({ type: "input", data: args.data }),
      );
      return undefined as T;
    case "write_bytes_to_session":
      await terminalSend(id, new Uint8Array(args.data as number[]));
      return undefined as T;
    case "resize_session":
      await terminalSend(
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
      const active = sockets.get(id);
      if (active) {
        active.closed = true;
        active.socket.close();
        sockets.delete(id);
      }
      break;
    }
  }
  return request<T>(`api/commands/${encodeURIComponent(command)}`, args);
}
export function csrfHeader(): Record<string, string> {
  return csrf ? { "X-Nyaterm-Csrf": csrf } : {};
}
