import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  fireEvent,
  render,
  renderHook,
  screen,
  waitFor,
} from "@testing-library/react";
import type { Terminal } from "@xterm/xterm";
import type { TerminalFitScheduler } from "@/components/terminal/terminalFitScheduler";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));
vi.mock("@/i18n", () => ({ default: { t: (key: string) => key } }));
class FakeSocket {
  static OPEN = 1;
  static instances: FakeSocket[] = [];
  static failedOpenings = 0;
  readyState = 1;
  bufferedAmount = 0;
  binaryType = "";
  onopen?: () => void;
  onerror?: () => void;
  onmessage?: (event: { data: unknown }) => void;
  onclose?: () => void;
  sent: unknown[] = [];
  constructor(public url: URL) {
    FakeSocket.instances.push(this);
    if (FakeSocket.failedOpenings > 0) {
      FakeSocket.failedOpenings--;
      queueMicrotask(() => {
        this.onerror?.();
        this.close();
      });
    } else queueMicrotask(() => this.onopen?.());
  }
  send(value: unknown) {
    this.sent.push(value);
  }
  close() {
    this.readyState = 3;
    this.onclose?.();
  }
}
class FakeEvents {
  onmessage?: (event: { data: string }) => void;
  constructor() {
    queueMicrotask(() =>
      this.onmessage?.({ data: JSON.stringify({ event: "ready" }) }),
    );
  }
  addEventListener() {}
  close() {}
}
const fetchMock = vi.fn();
beforeEach(() => {
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
  vi.resetModules();
  vi.stubEnv("BASE_URL", "/nyaterm/");
  vi.stubGlobal("WebSocket", FakeSocket);
  vi.stubGlobal("EventSource", FakeEvents);
  vi.stubGlobal("BroadcastChannel", undefined);
  vi.stubGlobal("fetch", fetchMock);
  FakeSocket.instances = [];
  FakeSocket.failedOpenings = 0;
  fetchMock
    .mockReset()
    .mockImplementation(
      async (url: URL) =>
        new Response(
          JSON.stringify(
            url.pathname.includes("auth/")
              ? { csrf: "csrf-test" }
              : url.pathname.endsWith("/sessions")
                ? { session_id: "session-1" }
                : url.pathname.endsWith("/upload")
                  ? {
                      bytes: 4,
                      status: "completed",
                      path: url.searchParams.get("path"),
                    }
                  : null,
          ),
          { headers: { "Content-Type": "application/json" } },
        ),
    );
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.unstubAllEnvs();
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    configurable: true,
    value: {},
  });
});

describe("browser backend boundary", () => {
  it("uploads browser clipboard PNG bytes with session ownership and CSRF", async () => {
    const image = new Blob([new Uint8Array([137, 80, 78, 71])], {
      type: "image/png",
    });
    const clipboard = {
      read: vi
        .fn()
        .mockResolvedValue([
          { types: ["image/png"], getType: async () => image },
        ]),
    };
    const previous = Object.getOwnPropertyDescriptor(navigator, "clipboard");
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: clipboard,
    });
    try {
      const { authenticate, httpInvoke } = await import("./http");
      await authenticate();
      const result = await httpInvoke<{ remote_path: string }>(
        "upload_clipboard_image_to_ssh",
        {
          sessionId: "owned-session",
          remoteDir: "/home/test",
        },
      );
      const [url, options] =
        fetchMock.mock.calls[fetchMock.mock.calls.length - 1]!;
      expect(url.pathname).toBe("/nyaterm/api/sessions/owned-session/upload");
      expect(url.searchParams.get("path")).toBe(result.remote_path);
      expect(result.remote_path).toMatch(
        /^\/home\/test\/nyaterm-clipboard-.*\.png$/,
      );
      expect(options).toMatchObject({
        method: "POST",
        credentials: "same-origin",
        body: image,
        headers: { "X-Nyaterm-Csrf": "csrf-test" },
      });
    } finally {
      if (previous) Object.defineProperty(navigator, "clipboard", previous);
      else Reflect.deleteProperty(navigator, "clipboard");
    }
  });

  it.each(["completed", "skipped"])(
    "uses final clipboard upload path and handles %s",
    async (status) => {
      const image = new Blob(["PNG"], { type: "image/png" });
      const previous = Object.getOwnPropertyDescriptor(navigator, "clipboard");
      Object.defineProperty(navigator, "clipboard", {
        configurable: true,
        value: {
          read: async () => [
            { types: ["image/png"], getType: async () => image },
          ],
        },
      });
      fetchMock.mockImplementation(
        async (url: URL) =>
          new Response(
            JSON.stringify(
              url.pathname.endsWith("/upload")
                ? {
                    bytes: status === "skipped" ? 0 : 3,
                    status,
                    path: "/renamed.png.uuid",
                  }
                : { csrf: "csrf-test" },
            ),
          ),
      );
      try {
        const { authenticate, httpInvoke } = await import("./http");
        await authenticate();
        expect(
          await httpInvoke("upload_clipboard_image_to_ssh", {
            sessionId: "owned-session",
            remoteDir: "/",
          }),
        ).toEqual(
          status === "skipped" ? null : { remote_path: "/renamed.png.uuid" },
        );
      } finally {
        if (previous) Object.defineProperty(navigator, "clipboard", previous);
        else Reflect.deleteProperty(navigator, "clipboard");
      }
    },
  );

  it("returns browser upload results and rejects failed downloads", async () => {
    const { uploadBrowserFile, downloadBrowserFile } = await import("./files");
    fetchMock.mockResolvedValueOnce(
      new Response(
        JSON.stringify({ bytes: 0, status: "skipped", path: "/same.txt" }),
      ),
    );
    expect(
      await uploadBrowserFile("s", "/same.txt", new Blob(["data"])),
    ).toEqual({ bytes: 0, status: "skipped", path: "/same.txt" });
    fetchMock.mockResolvedValueOnce(
      new Response(
        JSON.stringify({
          bytes: 4,
          status: "completed",
          path: "/same.txt.uuid",
        }),
      ),
    );
    expect(
      (await uploadBrowserFile("s", "/same.txt", new Blob(["data"]))).path,
    ).toBe("/same.txt.uuid");
    fetchMock.mockResolvedValueOnce(
      new Response(JSON.stringify({ error: "Permission denied" }), {
        status: 400,
      }),
    );
    await expect(downloadBrowserFile("s", "/denied.txt")).rejects.toThrow(
      "Permission denied",
    );
  });

  it("uses Web background data URLs without sending desktop paths to the server", async () => {
    const { loadBackgroundImageDataUrl } =
      await import("@/lib/backgroundImage");
    expect(
      await loadBackgroundImageDataUrl("C:\\Users\\test\\background.png"),
    ).toBe("");
    const image = "data:image/png;base64,iVBORw0KGgo=";
    expect(await loadBackgroundImageDataUrl(image)).toBe(image);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("mounts a Web terminal and cleans up browser resize listeners", async () => {
    const { useTerminalRefreshEffects } = await import(
      "@/components/terminal/useTerminalRefreshEffects"
    );
    const schedule = vi.fn();
    const terminal = {
      buffer: { active: { baseY: 0, viewportY: 0 } },
    } as unknown as Terminal;
    const { unmount } = renderHook(() =>
      useTerminalRefreshEffects({
        terminalRef: { current: terminal },
        fitSchedulerRef: {
          current: { schedule } as unknown as TerminalFitScheduler,
        },
        active: true,
        visible: true,
        appLocked: false,
        terminalReady: true,
        performanceMode: "normal",
        sessionId: "web-ssh",
        showGutter: false,
        showContentPadding: false,
      }),
    );
    // Window subscriptions resolve asynchronously, like the desktop API.
    await Promise.resolve();
    schedule.mockClear();
    window.dispatchEvent(new Event("resize"));
    expect(schedule).toHaveBeenCalledWith(
      expect.objectContaining({ reason: "window-resized", refresh: true }),
    );

    unmount();
    schedule.mockClear();
    window.dispatchEvent(new Event("resize"));
    expect(schedule).not.toHaveBeenCalled();
  });

  it("does not report Web active session changes to the desktop MCP host", async () => {
    const { useMcpActiveSession } = await import("@/hooks/useMcpActiveSession");
    const { rerender } = renderHook(
      ({ sessionId }) => useMcpActiveSession(sessionId),
      {
        initialProps: { sessionId: "web-ssh" as string | null },
      },
    );
    rerender({ sessionId: "web-telnet" });
    rerender({ sessionId: null });
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("completes Web output ACK batches without HTTP requests or retries", async () => {
    const { outputAckCoordinator } = await import("@/components/terminal/outputAckCoordinator");
    const lease = outputAckCoordinator.acquire("web-ssh", 1);
    lease.ack(128);
    lease.ack(201071);
    await waitFor(() =>
      expect(outputAckCoordinator.snapshot("web-ssh")).toEqual({
        leases: 1,
        pendingBytes: 0,
        inFlight: 0,
        retryTimers: 0,
      }),
    );
    lease.dispose();
    expect(outputAckCoordinator.snapshot("web-ssh").leases).toBe(0);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("loads command suggestions and the quick-command object in Web mode", async () => {
    fetchMock.mockImplementation(
      async (url: URL) =>
        new Response(
          JSON.stringify(
            url.pathname.endsWith("get_quick_commands")
              ? {
                  commands: [
                    { id: "q1", label: "List files", command: "ls -la" },
                  ],
                  categories: [],
                }
              : url.pathname.endsWith("fuzzy_search_commands")
                ? [
                    {
                      command: "ls -la",
                      display: "List files",
                      indices: [],
                      score: 10,
                      source: "quickCommand",
                    },
                  ]
                : [],
          ),
        ),
    );
    const { useCommandHistory } = await import("@/hooks/useCommandHistory");
    const { createTerminalInputState } = await import("@/lib/terminalInputTracker");
    const inputStateRef = {
      current: { ...createTerminalInputState(), value: "ls", cursor: 2 },
    };
    const terminalRef = { current: null };
    const apply = vi.fn();
    const canShow = () => true;
    const { result, unmount } = renderHook(() =>
      useCommandHistory(
        terminalRef,
        inputStateRef,
        apply,
        canShow,
        true,
        1,
        100,
      ),
    );
    act(() => {
      result.current.triggerSearch({ manual: true });
    });
    await waitFor(() => expect(result.current.showSuggestions).toBe(true));
    expect(result.current.suggestions[0].command).toBe("ls -la");
    act(() => {
      inputStateRef.current = createTerminalInputState();
      result.current.triggerSearch({ manual: true });
    });
    // Automatic searches normally debounce for 80ms.
    await new Promise((resolve) => setTimeout(resolve, 100));
    await waitFor(() =>
      expect(result.current.suggestions[0]?.command).toBe("ls -la"),
    );
    expect(
      fetchMock.mock.calls.some(([url]) =>
        url.pathname.endsWith("get_quick_commands"),
      ),
    ).toBe(true);
    unmount();
  });

  it("uses same-origin base paths, cookie credentials and CSRF without URL secrets", async () => {
    const { authenticate, httpInvoke } = await import("./http");
    await authenticate("test-login");
    expect(
      await httpInvoke("create_temporary_ssh_session", {
        config: { host: "example.test" },
      }),
    ).toBe("session-1");
    const [url, options] = fetchMock.mock.calls[fetchMock.mock.calls.length - 1];
    expect(url.pathname).toBe("/nyaterm/api/sessions");
    expect(url.search).toBe("");
    expect(options.credentials).toBe("same-origin");
    expect(options.headers["X-Nyaterm-Csrf"]).toBe("csrf-test");
  });
  it("keeps UTF-8 packet boundaries and queued output before ordered closure", async () => {
    const { httpInvoke } = await import("./http");
    const { browserListen } = await import("./events");
    await httpInvoke("attach_session", { sessionId: "terminal" });
    const socket = FakeSocket.instances[0];
    const bytes = new TextEncoder().encode("你好");
    socket.onmessage?.({ data: bytes.slice(0, 2).buffer });
    socket.onmessage?.({ data: bytes.slice(2).buffer });
    const output: string[] = [];
    const stop = await browserListen<{ data: string }>("terminal-output-terminal", ({ payload }) =>
      output.push(payload.data),
    );
    expect(output.join("")).toBe("你好");
    await httpInvoke("resize_session", {
      sessionId: "terminal",
      cols: 132,
      rows: 43,
    });
    expect(JSON.parse(socket.sent[0] as string)).toEqual({
      type: "resize",
      cols: 132,
      rows: 43,
    });
    await httpInvoke("write_bytes_to_session", {
      sessionId: "terminal",
      data: [0, 255],
    });
    expect(Array.from(socket.sent[1] as Uint8Array)).toEqual([0, 255]);
    const closed = vi.fn();
    await browserListen("session-closed-terminal", closed);
    socket.onmessage?.({ data: JSON.stringify({ type: "closed" }) });
    expect(closed).toHaveBeenCalledTimes(1);
    expect(socket.readyState).toBe(3);
    stop();
  });
  it("denies native local execution and bounds pending output", async () => {
    const { httpInvoke } = await import("./http");
    const { deliver } = await import("./events");
    await expect(httpInvoke("create_local_session")).rejects.toThrow(
      "Capability unavailable",
    );
    await expect(httpInvoke("get_default_local_shell")).rejects.toThrow(
      "Capability unavailable in Web mode: localShell",
    );
    expect(fetchMock).not.toHaveBeenCalled();
    deliver("terminal-output-absent", { data: "", bytes: 2 * 1024 * 1024 });
    expect(() =>
      deliver("terminal-output-absent", { data: "x", bytes: 1 }),
    ).toThrow("listener");
  });
  it("opens existing settings pages in a browser dialog and closes them", async () => {
    const { WebviewWindow } = await import("./platform/webviewWindow");
    const child = new WebviewWindow("settings", {
      url: "index.html?window=settings",
      width: 800,
    });
    const frame = document.querySelector("iframe")!;
    expect(new URL(frame.src).pathname).toBe("/nyaterm/index.html");
    expect(new URL(frame.src).searchParams.get("window")).toBe("settings");
    expect(await WebviewWindow.getByLabel("settings")).toBe(child);
    await child.close();
    expect(document.querySelector("iframe")).toBeNull();
  });
  it("opens settings through the window manager, waits for readiness, reuses and closes the dialog", async () => {
    vi.spyOn(window, "focus").mockImplementation(() => {});
    const { openSettings } = await import("../windowManager");
    const { browserEmit, browserListen } = await import("./events");
    const { CHILD_WINDOW_LIFECYCLE_EVENT } = await import("../childWindowProtocol");
    const command = vi.fn();
    const stop = await browserListen("settings-open-tab", command);
    const opening = openSettings("appearance");
    await waitFor(() => expect(document.querySelector("iframe")).toBeTruthy());
    const frame = document.querySelector("iframe")!;
    const url = new URL(frame.src);
    expect(url.pathname).toBe("/nyaterm/index.html");
    expect(url.searchParams.get("webWindowLabel")).toBe("settings");
    expect(frame.parentElement?.style.display).toBe("none");
    const identity = {
      label: "settings",
      token: url.searchParams.get("readyToken")!,
    };
    expect(identity.token).toBeTruthy();
    await browserEmit(CHILD_WINDOW_LIFECYCLE_EVENT, {
      ...identity,
      phase: "shell-ready",
    });
    const child = await opening;
    expect(await child.isVisible()).toBe(true);
    expect(command).not.toHaveBeenCalled();
    await browserEmit(CHILD_WINDOW_LIFECYCLE_EVENT, {
      ...identity,
      phase: "command-ready",
      command: "settings-open-tab",
    });
    expect(command).toHaveBeenCalledWith(
      expect.objectContaining({
        payload: { tab: "appearance", targetWindowLabel: "main" },
      }),
    );
    expect(await openSettings("general")).toBe(child);
    expect(document.querySelectorAll("iframe")).toHaveLength(1);
    await child.close();
    expect(document.querySelector("iframe")).toBeNull();
    expect(fetchMock).not.toHaveBeenCalled();
    stop();
  });
  it("removes a browser child when bootstrap fails before readiness", async () => {
    vi.spyOn(window, "focus").mockImplementation(() => {});
    const { openChildWindow } = await import("../windowManager");
    const { browserEmit } = await import("./events");
    const { CHILD_WINDOW_LIFECYCLE_EVENT } = await import("../childWindowProtocol");
    const opening = openChildWindow({
      label: "new-session",
      title: "New session",
      url: "index.html?window=new-session&owner=main",
    });
    const failure = expect(opening).rejects.toThrow(
      "Child window did not finish rendering",
    );
    await waitFor(() => expect(document.querySelector("iframe")).toBeTruthy());
    const url = new URL(document.querySelector("iframe")!.src);
    await browserEmit(CHILD_WINDOW_LIFECYCLE_EVENT, {
      label: "new-session",
      token: url.searchParams.get("readyToken"),
      phase: "load-failed",
      stage: "bootstrap-import",
    });
    await failure;
    expect(document.querySelector("iframe")).toBeNull();
    expect(fetchMock).not.toHaveBeenCalled();
  });
  it("keeps browser logs in the console without sending unsupported persistence requests", async () => {
    const info = vi.spyOn(console, "info").mockImplementation(() => {});
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    const { logger } = await import("../logger");
    for (let index = 0; index < 60; index += 1) {
      logger.info({
        domain: "app.lifecycle",
        event: "test.entry",
        message: "Browser log",
      });
    }
    logger.error({
      domain: "ui.error",
      event: "test.error",
      message: "Browser error",
    });
    await logger.flush();
    window.dispatchEvent(new Event("pagehide"));
    expect(info).toHaveBeenCalledTimes(60);
    expect(error).toHaveBeenCalledTimes(1);
    expect(fetchMock).not.toHaveBeenCalled();
    info.mockRestore();
    error.mockRestore();
  });
  it("shows the login gate and mounts the existing UI only after authentication", async () => {
    fetchMock.mockResolvedValueOnce(
      new Response(JSON.stringify({ error: "Sign in required" }), {
        status: 401,
      }),
    );
    const { BrowserGate } = await import("./BrowserGate");
    render(
      <BrowserGate>
        <div>Existing NyaTerm application</div>
      </BrowserGate>,
    );
    await waitFor(() => expect(fetchMock).toHaveBeenCalled());
    expect(screen.queryByText("Existing NyaTerm application")).toBeNull();
    fireEvent.change(screen.getByLabelText("web.password"), {
      target: { value: "test-login" },
    });
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "web.signIn" }).getAttribute("disabled"),
      ).toBeNull(),
    );
    fireEvent.click(screen.getByRole("button", { name: "web.signIn" }));
    expect(
      await screen.findByText("Existing NyaTerm application"),
    ).toBeTruthy();
  });
});

describe("Web Telnet and VNC", () => {
  it("restores VNC frames and input after a failed reconnect handshake", async () => {
    const { subscribeBrowserVnc, sendBrowserVnc } = await import("./vnc");
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    const frame = vi.fn();
    const onError = vi.fn();
    const stop = await subscribeBrowserVnc("retry", frame, onError);
    try {
      FakeSocket.failedOpenings = 1;
      FakeSocket.instances[0].close();
      await vi.advanceTimersByTimeAsync(1000);
      expect(onError).toHaveBeenCalledOnce();
      await vi.advanceTimersByTimeAsync(1000);
      const recovered = FakeSocket.instances[2];
      const bytes = new ArrayBuffer(44);
      recovered.onmessage?.({ data: bytes });
      expect(frame).toHaveBeenCalledWith(bytes);
      await sendBrowserVnc("retry", { type: "input", events: [] });
      expect(recovered.sent).toEqual([
        JSON.stringify({ type: "input", events: [] }),
      ]);
      stop();
      await vi.advanceTimersByTimeAsync(3000);
      expect(FakeSocket.instances).toHaveLength(3);
    } finally {
      stop();
      vi.useRealTimers();
    }
  });
  it("enables the supported protocol and proxy capabilities only", async () => {
    const { supports } = await import("./runtime");
    for (const name of ["ssh", "telnet", "vnc", "networkProxy"] as const)
      expect(supports(name)).toBe(true);
    for (const name of ["remoteDesktop", "localShell", "serial"] as const)
      expect(supports(name)).toBe(false);
  });
  it("maps Telnet and VNC creation and loads real proxy configuration", async () => {
    const { httpInvoke } = await import("./http");
    await httpInvoke("create_telnet_session", {
      host: "telnet.test",
      port: 23,
    });
    expect(
      JSON.parse(
        fetchMock.mock.calls[fetchMock.mock.calls.length - 1]?.[1].body,
      ),
    ).toEqual({
      host: "telnet.test",
      port: 23,
      type: "telnet",
    });
    await httpInvoke("create_vnc_session", { connectionId: "vnc-saved" });
    expect(
      JSON.parse(
        fetchMock.mock.calls[fetchMock.mock.calls.length - 1]?.[1].body,
      ),
    ).toEqual({
      connectionId: "vnc-saved",
      type: "vnc",
    });
    await httpInvoke("get_proxies");
    expect(
      fetchMock.mock.calls[fetchMock.mock.calls.length - 1]?.[0].pathname,
    ).toBe("/nyaterm/api/commands/get_proxies");
  });
  it("replays VNC messages and keeps a replacement safe from stale cleanup", async () => {
    const { subscribeBrowserVnc, sendBrowserVnc, closeAllBrowserVnc } = await import("./vnc");
    const { browserListen } = await import("./events");
    const oldController = new AbortController();
    const old = subscribeBrowserVnc(
      "vnc",
      vi.fn(),
      vi.fn(),
      oldController.signal,
    ).catch(() => {});
    const frame = vi.fn();
    const current = subscribeBrowserVnc("vnc", frame, vi.fn());
    await old;
    const stop = await current;
    const socket = FakeSocket.instances[FakeSocket.instances.length - 1]!;
    const state = vi.fn();
    const unlisten = await browserListen("vnc-state-vnc", state);
    const bytes = new ArrayBuffer(44);
    socket.onmessage?.({ data: bytes });
    socket.onmessage?.({
      data: JSON.stringify({
        type: "event",
        event: "vnc-state-vnc",
        payload: { state: "active" },
      }),
    });
    expect(frame).toHaveBeenCalledWith(bytes);
    expect(state).toHaveBeenCalledWith(
      expect.objectContaining({ payload: { state: "active" } }),
    );
    oldController.abort();
    await sendBrowserVnc("vnc", { type: "input", events: [] });
    expect(socket.sent).toContain(
      JSON.stringify({ type: "input", events: [] }),
    );
    expect(socket.url.search).toBe("");
    stop();
    expect(socket.readyState).toBe(3);
    unlisten();
    closeAllBrowserVnc();
  });
  it("cancels a VNC attachment before its socket opens", async () => {
    const { subscribeBrowserVnc } = await import("./vnc");
    const controller = new AbortController();
    const result = subscribeBrowserVnc(
      "cancelled",
      vi.fn(),
      vi.fn(),
      controller.signal,
    );
    controller.abort();
    await expect(result).rejects.toThrow();
    expect(FakeSocket.instances[0].readyState).toBe(3);
  });
});

it("delivers Web connection failures to the existing terminal error listener", async () => {
  const { httpInvoke } = await import("./http");
  const { browserListen } = await import("./events");
  await httpInvoke("attach_session", { sessionId: "failure" });
  const socket = FakeSocket.instances[0];
  const failed = vi.fn();
  const unlisten = await browserListen("session-error-failure", failed);
  socket.onmessage?.({
    data: JSON.stringify({
      type: "error",
      error: "Proxy credentials could not be decrypted",
    }),
  });
  expect(failed).toHaveBeenCalledWith(
    expect.objectContaining({
      payload: "Proxy credentials could not be decrypted",
    }),
  );
  unlisten();
});
