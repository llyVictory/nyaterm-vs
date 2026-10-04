import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));
vi.mock("@/i18n", () => ({ default: { t: (key: string) => key } }));
class FakeSocket {
  static OPEN = 1;
  static instances: FakeSocket[] = [];
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
    queueMicrotask(() => this.onopen?.());
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
  it("uses same-origin base paths, cookie credentials and CSRF without URL secrets", async () => {
    const { authenticate, httpInvoke } = await import("./http");
    await authenticate("test-login");
    expect(
      await httpInvoke("create_temporary_ssh_session", {
        config: { host: "example.test" },
      }),
    ).toBe("session-1");
    const [url, options] =
      fetchMock.mock.calls[fetchMock.mock.calls.length - 1];
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
    const stop = await browserListen<{ data: string }>(
      "terminal-output-terminal",
      ({ payload }) => output.push(payload.data),
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
    const { CHILD_WINDOW_LIFECYCLE_EVENT } =
      await import("../childWindowProtocol");
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
    const { CHILD_WINDOW_LIFECYCLE_EVENT } =
      await import("../childWindowProtocol");
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
        screen
          .getByRole("button", { name: "web.signIn" })
          .getAttribute("disabled"),
      ).toBeNull(),
    );
    fireEvent.click(screen.getByRole("button", { name: "web.signIn" }));
    expect(
      await screen.findByText("Existing NyaTerm application"),
    ).toBeTruthy();
  });
});
