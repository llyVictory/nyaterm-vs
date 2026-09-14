import { beforeEach, describe, expect, it, vi } from "vitest";
import { CHILD_WINDOW_LIFECYCLE_EVENT, type ChildWindowLifecyclePayload } from "./childWindowProtocol";

const mocks = vi.hoisted(() => {
  type MockWindow = {
    label: string;
    close: () => Promise<void>;
    hide: () => Promise<void>;
    isVisible: () => Promise<boolean>;
    once: (event: string, handler: () => void) => Promise<void>;
    outerPosition: () => Promise<{ x: number; y: number }>;
    outerSize: () => Promise<{ width: number; height: number }>;
    requestUserAttention: () => Promise<void>;
    setAlwaysOnTop: () => Promise<void>;
    setEnabled: () => Promise<void>;
    setFocus: () => Promise<void>;
    setFocusable: () => Promise<void>;
    setPosition: () => Promise<void>;
    setTitle: () => Promise<void>;
    show: () => Promise<void>;
  };

  const listeners = new Map<string, (event: { payload: unknown }) => void>();
  const windows = new Map<string, MockWindow>();
  const currentWindow = createMockWindow("main", windows);

  function createMockWindow(label: string, registry: Map<string, MockWindow>): MockWindow {
    const destroyedHandlers: Array<() => void> = [];
    const win = {
      label,
      close: vi.fn(async () => {
        registry.delete(label);
        for (const handler of destroyedHandlers) handler();
      }),
      hide: vi.fn(async () => {}),
      isVisible: vi.fn(async () => true),
      once: vi.fn(async (event: string, handler: () => void) => {
        if (event === "tauri://destroyed") destroyedHandlers.push(handler);
      }),
      outerPosition: vi.fn(async () => ({ x: 0, y: 0 })),
      outerSize: vi.fn(async () => ({ width: 800, height: 560 })),
      requestUserAttention: vi.fn(async () => {}),
      setAlwaysOnTop: vi.fn(async () => {}),
      setEnabled: vi.fn(async () => {}),
      setFocus: vi.fn(async () => {}),
      setFocusable: vi.fn(async () => {}),
      setPosition: vi.fn(async () => {}),
      setTitle: vi.fn(async () => {}),
      show: vi.fn(async () => {}),
    };
    return win;
  }

  return {
    availableMonitors: vi.fn(async () => [
      {
        workArea: {
          position: { x: 0, y: 0 },
          size: { width: 1920, height: 1040 },
        },
      },
    ]),
    createMockWindow,
    currentWindow,
    isLinux: false,
    isMacOS: false,
    emit: vi.fn(async () => {}),
    getAll: vi.fn(async () => Array.from(windows.values())),
    getByLabel: vi.fn(async (label: string) => windows.get(label) ?? null),
    getCurrentWindow: vi.fn(() => currentWindow),
    invoke: vi.fn(async (_command: string, args?: { options?: { label?: string } }) => {
      const label = args?.options?.label;
      if (label) windows.set(label, createMockWindow(label, windows));
    }),
    listen: vi.fn(async (event: string, handler: (event: { payload: unknown }) => void) => {
      listeners.set(event, handler);
      return () => listeners.delete(event);
    }),
    listeners,
    primaryMonitor: vi.fn(async () => ({
      workArea: {
        position: { x: 0, y: 0 },
        size: { width: 1920, height: 1040 },
      },
    })),
    windows,
  };
});

vi.mock("@tauri-apps/api/event", () => ({
  emit: mocks.emit,
  listen: mocks.listen,
}));

vi.mock("@tauri-apps/api/webviewWindow", () => ({
  WebviewWindow: {
    getAll: mocks.getAll,
    getByLabel: mocks.getByLabel,
  },
}));

vi.mock("@tauri-apps/api/window", () => ({
  availableMonitors: mocks.availableMonitors,
  getCurrentWindow: mocks.getCurrentWindow,
  PhysicalPosition: class PhysicalPosition {
    constructor(
      public x: number,
      public y: number,
    ) {}
  },
  primaryMonitor: mocks.primaryMonitor,
  UserAttentionType: { Critical: 1 },
}));

vi.mock("./invoke", () => ({ invoke: mocks.invoke }));
vi.mock("../i18n", () => ({ default: { t: (key: string) => key } }));
vi.mock("./logger", () => ({
  logger: {
    info: vi.fn(),
    warn: vi.fn(),
  },
}));
vi.mock("./platform", () => ({
  get isLinux() {
    return mocks.isLinux;
  },
  get isMacOS() {
    return mocks.isMacOS;
  },
}));

beforeEach(() => {
  vi.clearAllMocks();
  vi.resetModules();
  mocks.isLinux = false;
  mocks.isMacOS = false;
  mocks.listeners.clear();
  mocks.windows.clear();
});

async function importWindowManager() {
  return import("./windowManager");
}

function emitLifecycle(payload: ChildWindowLifecyclePayload) {
  mocks.listeners.get(CHILD_WINDOW_LIFECYCLE_EVENT)?.({ payload });
}

async function waitForInvoke() {
  await vi.waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith("open_child_window", expect.anything()));
}

function createdToken(callIndex = 0) {
  const args = mocks.invoke.mock.calls[callIndex][1] as { options: { url: string } };
  const params = new URLSearchParams(args.options.url.slice(args.options.url.indexOf("?") + 1));
  const token = params.get("readyToken");
  expect(token).toBeTruthy();
  return token as string;
}

describe("child window command mapping", () => {
  it.each([
    ["index.html?window=settings", "settings-open-tab"],
    ["index.html?window=file-editor", "remote-file-editor-open"],
    ["index.html?window=file-preview", "file-preview-open"],
    ["index.html?window=new-session", undefined],
  ])("maps %s to %s", async (url, expected) => {
    const { childWindowCommandForUrl } = await importWindowManager();
    expect(childWindowCommandForUrl(url)).toBe(expected);
  });
});

describe("child window work-area helpers", () => {
  const primaryWorkArea = {
    position: { x: 0, y: 0 },
    size: { width: 1920, height: 1040 },
  };

  it("detects a child window completely outside disconnected monitor bounds", async () => {
    const { rectOverlapsWorkArea } = await importWindowManager();
    expect(rectOverlapsWorkArea({ x: 2500, y: 100, width: 800, height: 560 }, primaryWorkArea)).toBe(
      false,
    );
  });

  it("keeps a child window that still intersects the visible work area", async () => {
    const { rectOverlapsWorkArea } = await importWindowManager();
    expect(rectOverlapsWorkArea({ x: 1800, y: 100, width: 800, height: 560 }, primaryWorkArea)).toBe(
      true,
    );
  });

  it("centers an off-screen child window in the selected work area", async () => {
    const { centerWindowRectInWorkArea } = await importWindowManager();
    expect(centerWindowRectInWorkArea({ width: 800, height: 560 }, primaryWorkArea)).toEqual({
      x: 560,
      y: 240,
    });
  });
});

describe("modal window raising", () => {
  it("does not pulse always-on-top for ordinary modal windows on Linux", async () => {
    mocks.isLinux = true;
    const settingsWindow = mocks.createMockWindow("settings", mocks.windows);
    mocks.windows.set("settings", settingsWindow);
    const { raiseModalChildWindowGroup } = await importWindowManager();

    await raiseModalChildWindowGroup({ reason: "open" });

    expect(settingsWindow.show).toHaveBeenCalledOnce();
    expect(settingsWindow.setFocus).toHaveBeenCalledOnce();
    expect(settingsWindow.setAlwaysOnTop).not.toHaveBeenCalled();
  });

  it("preserves required always-on-top state for auto-upload windows on Linux", async () => {
    mocks.isLinux = true;
    const autoUploadWindow = mocks.createMockWindow(
      "auto-upload-bWFpbg--tmp-upload",
      mocks.windows,
    );
    mocks.windows.set(autoUploadWindow.label, autoUploadWindow);
    const { raiseModalChildWindowGroup } = await importWindowManager();

    await raiseModalChildWindowGroup({ reason: "open" });

    expect(autoUploadWindow.setAlwaysOnTop).toHaveBeenCalledOnce();
    expect(autoUploadWindow.setAlwaysOnTop).toHaveBeenCalledWith(true);
  });

  it("retains the topmost pulse for modal windows on non-Linux platforms", async () => {
    const settingsWindow = mocks.createMockWindow("settings", mocks.windows);
    mocks.windows.set("settings", settingsWindow);
    const { raiseModalChildWindowGroup } = await importWindowManager();

    await raiseModalChildWindowGroup({ reason: "open" });

    expect(settingsWindow.setAlwaysOnTop).toHaveBeenCalledWith(true);
  });
});

describe("child window load failure recovery", () => {
  it("closes and clears a revealed window after the command listener fails", async () => {
    const { openSettings } = await importWindowManager();
    const open = openSettings("appearance");
    await waitForInvoke();
    const token = createdToken();
    emitLifecycle({ label: "settings", token, phase: "shell-ready" });
    await open;

    const win = mocks.windows.get("settings");
    expect(win?.show).toHaveBeenCalled();
    emitLifecycle({ label: "settings", token, phase: "load-failed", stage: "command-listener" });

    await vi.waitFor(() => expect(win?.close).toHaveBeenCalledOnce());
    expect(mocks.windows.has("settings")).toBe(false);
  });

  it("recreates a failed existing window on the next open", async () => {
    const { openSettings } = await importWindowManager();
    const firstOpen = openSettings("appearance");
    await waitForInvoke();
    const firstToken = createdToken();
    emitLifecycle({ label: "settings", token: firstToken, phase: "shell-ready" });
    await firstOpen;

    const firstWindow = mocks.windows.get("settings");
    emitLifecycle({
      label: "settings",
      token: firstToken,
      phase: "load-failed",
      stage: "command-listener",
    });
    await vi.waitFor(() => expect(firstWindow?.close).toHaveBeenCalledOnce());

    const secondOpen = openSettings("general");
    await vi.waitFor(() => expect(mocks.invoke).toHaveBeenCalledTimes(2));
    const secondToken = createdToken(1);
    emitLifecycle({ label: "settings", token: secondToken, phase: "shell-ready" });
    await secondOpen;

    const secondWindow = mocks.windows.get("settings");
    expect(secondWindow).toBeTruthy();
    expect(secondWindow).not.toBe(firstWindow);
  });

  it("fails first open promptly and closes the orphan when bootstrap fails before shell ready", async () => {
    const { openSettings } = await importWindowManager();
    const open = openSettings("appearance");
    await waitForInvoke();
    const token = createdToken();
    const win = mocks.windows.get("settings");

    emitLifecycle({ label: "settings", token, phase: "load-failed", stage: "bootstrap-import" });

    await expect(open).rejects.toThrow("Child window did not finish rendering: settings");
    await vi.waitFor(() => expect(win?.close).toHaveBeenCalled());
  });

  it("ignores stale load-failed events from an old token", async () => {
    const { openSettings } = await importWindowManager();
    const open = openSettings("appearance");
    await waitForInvoke();
    const token = createdToken();
    emitLifecycle({ label: "settings", token, phase: "shell-ready" });
    await open;

    const win = mocks.windows.get("settings");
    emitLifecycle({
      label: "settings",
      token: "stale-token",
      phase: "load-failed",
      stage: "command-listener",
    });

    await Promise.resolve();
    expect(win?.close).not.toHaveBeenCalled();
    expect(mocks.windows.get("settings")).toBe(win);
  });
});
