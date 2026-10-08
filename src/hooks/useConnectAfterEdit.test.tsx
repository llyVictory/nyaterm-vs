import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createTerminalWindowLeaf, type TerminalWindowNode } from "@/lib/tabWindows";
import { createSessionPane, createWorkspaceTab } from "@/lib/workspaceTabs";
import type { SavedConnection } from "@/types/global";
import { useConnectAfterEdit } from "./useConnectAfterEdit";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  closeStaleCreatedSession: vi.fn(),
  focusTerminalSession: vi.fn(),
  launchSavedRdpWithSystemClient: vi.fn(),
  openNewSession: vi.fn(),
}));
vi.mock("@/lib/invoke", () => ({ invoke: mocks.invoke }));
vi.mock("@/lib/windowManager", () => ({ openNewSession: mocks.openNewSession }));
vi.mock("@/lib/appSessionFactory", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/appSessionFactory")>()),
  closeStaleCreatedSession: mocks.closeStaleCreatedSession,
  focusTerminalSession: mocks.focusTerminalSession,
  launchSavedRdpWithSystemClient: mocks.launchSavedRdpWithSystemClient,
}));

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

function renderConnect(overrides: Partial<Parameters<typeof useConnectAfterEdit>[0]> = {}) {
  const options: Parameters<typeof useConnectAfterEdit>[0] = {
    tabs: [],
    addPendingTab: vi.fn().mockReturnValue({
      tabId: "pending-tab",
      paneId: "pending-pane",
      createRequestId: "pending-request",
    }),
    hasPane: vi.fn().mockReturnValue(true),
    hasTab: vi.fn().mockReturnValue(true),
    markPaneConnecting: vi.fn().mockReturnValue("reuse-request"),
    markPaneConnectionFailed: vi.fn(),
    markTabConnectionFailed: vi.fn(),
    recordRecentConnection: vi.fn(),
    setActivePane: vi.fn(),
    setActiveTabId: vi.fn(),
    updatePaneSession: vi.fn(),
    updateTabSession: vi.fn(),
    rdpClientMode: "builtin",
    setTerminalWindows: vi.fn(),
    updateAutoIconForSessionStart: vi.fn(),
    ...overrides,
  };
  return { ...renderHook(useConnectAfterEdit, { initialProps: options }), options };
}

const connection: SavedConnection = { id: "conn-1", name: "Edited Host", type: "ssh" };

beforeEach(() => {
  vi.clearAllMocks();
  mocks.invoke
    .mockReset()
    .mockImplementation(async (command: string) =>
      command === "get_saved_connections" ? [connection] : "new-session",
    );
  mocks.closeStaleCreatedSession.mockResolvedValue(undefined);
  mocks.launchSavedRdpWithSystemClient.mockResolvedValue(false);
});

describe("useConnectAfterEdit", () => {
  it("reuses the specified pane and records the successfully created session", async () => {
    const pane = createSessionPane("Old Host", "SSH", "conn-1");
    const tab = createWorkspaceTab(pane, 0);
    const { result, options } = renderConnect({ tabs: [tab] });
    await act(() =>
      result.current({ connectionId: "conn-1", sourceTabId: tab.id, sourcePaneId: pane.id }),
    );
    expect(options.addPendingTab).not.toHaveBeenCalled();
    expect(options.setActiveTabId).toHaveBeenCalledWith(tab.id);
    expect(options.setActivePane).toHaveBeenCalledWith(tab.id, pane.id);
    expect(options.markPaneConnecting).toHaveBeenCalledWith(tab.id, pane.id, {
      name: "Edited Host",
      type: "SSH",
      connectionId: "conn-1",
      display: undefined,
    });
    expect(mocks.invoke).toHaveBeenLastCalledWith("create_ssh_session", {
      connectionId: "conn-1",
      createRequestId: "reuse-request",
      recordingScopeId: pane.id,
    });
    expect(options.updatePaneSession).toHaveBeenCalledWith(tab.id, pane.id, "new-session");
    expect(options.updateTabSession).not.toHaveBeenCalled();
    expect(mocks.focusTerminalSession).toHaveBeenCalledWith("new-session");
    expect(options.recordRecentConnection).toHaveBeenCalledWith("conn-1");
    expect(options.updateAutoIconForSessionStart).toHaveBeenCalledWith("conn-1", "new-session");
  });

  it("falls back to the active pane when the requested source pane is gone", async () => {
    const pane = createSessionPane("Host", "SSH");
    const tab = createWorkspaceTab(pane, 0);
    const { result, options } = renderConnect({ tabs: [tab] });
    await act(() =>
      result.current({ connectionId: "conn-1", sourceTabId: tab.id, sourcePaneId: "closed-pane" }),
    );
    expect(options.setActivePane).toHaveBeenCalledWith(tab.id, pane.id);
    expect(options.addPendingTab).not.toHaveBeenCalled();
  });

  it("creates a tab after the anchor and inserts it into the requested split", async () => {
    let layout: TerminalWindowNode | null = createTerminalWindowLeaf(["anchor-tab"], "anchor-tab");
    const leafId = layout.id;
    const { result, options } = renderConnect({
      setTerminalWindows: vi.fn((update) => {
        layout = typeof update === "function" ? update(layout) : update;
      }),
    });
    await act(() =>
      result.current({
        connectionId: "conn-1",
        sourceTabId: "closed-tab",
        targetLeafId: leafId,
        anchorTabId: "anchor-tab",
      }),
    );
    expect(options.addPendingTab).toHaveBeenCalledWith(
      "Edited Host",
      "SSH",
      "conn-1",
      undefined,
      { afterTabId: "anchor-tab" },
      { display: undefined },
    );
    expect(layout).toMatchObject({
      tabIds: ["anchor-tab", "pending-tab"],
      activeTabId: "pending-tab",
    });
    expect(options.updatePaneSession).toHaveBeenCalledWith(
      "pending-tab",
      "pending-pane",
      "new-session",
    );
  });

  it("queries the latest tabs after waiting for saved connections", async () => {
    const lookup = deferred<SavedConnection[]>();
    mocks.invoke.mockReturnValueOnce(lookup.promise);
    const pane = createSessionPane("Host", "SSH");
    const tab = createWorkspaceTab(pane, 0);
    const { result, rerender, options } = renderConnect({ tabs: [tab] });
    const pending = result.current({
      connectionId: "conn-1",
      sourceTabId: tab.id,
      sourcePaneId: pane.id,
    });
    rerender({ ...options, tabs: [] });
    await act(async () => {
      lookup.resolve([connection]);
      await pending;
    });
    expect(options.markPaneConnecting).not.toHaveBeenCalled();
    expect(options.addPendingTab).toHaveBeenCalledOnce();
    expect(options.updatePaneSession).toHaveBeenCalledWith(
      "pending-tab",
      "pending-pane",
      "new-session",
    );
  });

  it.each([
    ["ssh", "create_ssh_session", true],
    ["local_terminal", "create_local_session", true],
    ["telnet", "create_telnet_session", true],
    ["serial", "create_serial_session", true],
    ["rdp", "create_rdp_session", false],
    // Preserve the current fallback branch during this structural refactor.
    ["vnc", "create_ssh_session", true],
  ] as const)("preserves the %s creation command and recording scope", async (type, command, hasScope) => {
    mocks.invoke.mockResolvedValueOnce([{ ...connection, type }]);
    const { result } = renderConnect();
    await act(() => result.current({ connectionId: "conn-1" }));
    expect(mocks.invoke).toHaveBeenLastCalledWith(command, {
      connectionId: "conn-1",
      createRequestId: "pending-request",
      ...(hasScope ? { recordingScopeId: "pending-pane" } : {}),
    });
  });

  it("launches a system RDP connection before creating a pending pane", async () => {
    const rdpConnection = { ...connection, type: "rdp" as const };
    mocks.invoke.mockResolvedValueOnce([rdpConnection]);
    mocks.launchSavedRdpWithSystemClient.mockResolvedValueOnce(true);
    const { result, options } = renderConnect({ rdpClientMode: "windows" });

    await act(() => result.current({ connectionId: "conn-1" }));

    expect(mocks.launchSavedRdpWithSystemClient).toHaveBeenCalledWith(
      rdpConnection,
      "windows",
    );
    expect(options.addPendingTab).not.toHaveBeenCalled();
    expect(options.markPaneConnecting).not.toHaveBeenCalled();
    expect(options.updatePaneSession).not.toHaveBeenCalled();
    expect(options.updateTabSession).not.toHaveBeenCalled();
    expect(options.updateAutoIconForSessionStart).not.toHaveBeenCalled();
    expect(options.recordRecentConnection).toHaveBeenCalledWith("conn-1");
    expect(mocks.invoke).toHaveBeenCalledTimes(1);
    expect(mocks.invoke).not.toHaveBeenCalledWith(
      "create_rdp_session",
      expect.anything(),
    );
  });

  it("stops before creating a pane when the external RDP launch fails", async () => {
    mocks.invoke.mockResolvedValueOnce([
      { ...connection, type: "rdp", rdp_client_mode: "windows" },
    ]);
    mocks.launchSavedRdpWithSystemClient.mockRejectedValueOnce(
      new Error("Launch failed"),
    );
    const { result, options } = renderConnect();
    await act(() => result.current({ connectionId: "conn-1" }));
    expect(options.addPendingTab).not.toHaveBeenCalled();
    expect(options.markPaneConnecting).not.toHaveBeenCalled();
    expect(options.recordRecentConnection).not.toHaveBeenCalled();
    expect(mocks.invoke).toHaveBeenCalledTimes(1);
  });

  it("preserves the ID fallback when the saved connection is missing", async () => {
    mocks.invoke.mockResolvedValueOnce([]);
    const { result, options } = renderConnect();
    await act(() => result.current({ connectionId: "missing-connection" }));
    expect(options.addPendingTab).toHaveBeenCalledWith(
      "missing-connection",
      "SSH",
      "missing-connection",
      undefined,
      undefined,
      { display: undefined },
    );
    expect(mocks.invoke).toHaveBeenLastCalledWith("create_ssh_session", {
      connectionId: "missing-connection",
      createRequestId: "pending-request",
      recordingScopeId: "pending-pane",
    });
  });

  it("closes a session that completes after its pane was removed", async () => {
    const creation = deferred<string>();
    mocks.invoke.mockImplementation(async (command: string) =>
      command === "get_saved_connections" ? [connection] : creation.promise,
    );
    const { result, options } = renderConnect();
    let pending!: Promise<void>;
    await act(async () => {
      pending = result.current({ connectionId: "conn-1" });
    });
    vi.mocked(options.hasPane).mockReturnValue(false);
    await act(async () => {
      creation.resolve("orphan-session");
      await pending;
    });
    expect(mocks.closeStaleCreatedSession).toHaveBeenCalledWith("orphan-session");
    expect(options.updatePaneSession).not.toHaveBeenCalled();
    expect(mocks.focusTerminalSession).not.toHaveBeenCalled();
    expect(options.recordRecentConnection).not.toHaveBeenCalled();
  });

  it.each([
    "session creation cancelled",
    "pane removed",
  ])("suppresses recovery after %s", async (reason) => {
    mocks.invoke.mockResolvedValueOnce([connection]).mockRejectedValueOnce(new Error(reason));
    const { result, options } = renderConnect({
      hasPane: vi.fn().mockReturnValue(reason !== "pane removed"),
    });
    await act(() => result.current({ connectionId: "conn-1" }));
    expect(options.markPaneConnectionFailed).not.toHaveBeenCalled();
    expect(mocks.openNewSession).not.toHaveBeenCalled();
  });

  it("marks the reused pane failed and opens edit recovery for a missing SSH key", async () => {
    mocks.invoke
      .mockResolvedValueOnce([connection])
      .mockRejectedValueOnce(new Error("No SSH key for this connection"));
    const pane = createSessionPane("Host", "SSH");
    const tab = createWorkspaceTab(pane, 0);
    const { result, options } = renderConnect({ tabs: [tab] });
    await act(() => result.current({ connectionId: "conn-1", sourceTabId: tab.id }));
    expect(options.markPaneConnectionFailed).toHaveBeenCalledWith(
      tab.id,
      pane.id,
      "No SSH key for this connection",
    );
    expect(mocks.openNewSession).toHaveBeenCalledWith("conn-1", true, {
      sourceTabId: tab.id,
      sourcePaneId: pane.id,
    });
  });

  it("marks a normal connection failure without opening the editor", async () => {
    mocks.invoke
      .mockResolvedValueOnce([connection])
      .mockRejectedValueOnce(new Error("Connection refused"));
    const { result, options } = renderConnect();
    await act(() => result.current({ connectionId: "conn-1" }));
    expect(options.markPaneConnectionFailed).toHaveBeenCalledWith(
      "pending-tab",
      "pending-pane",
      "Connection refused",
    );
    expect(mocks.openNewSession).not.toHaveBeenCalled();
  });

  it("leaves the workspace unchanged when saved connection loading fails", async () => {
    mocks.invoke.mockRejectedValueOnce(new Error("Storage unavailable"));
    const { result, options } = renderConnect();
    await act(() => result.current({ connectionId: "conn-1" }));
    expect(options.addPendingTab).not.toHaveBeenCalled();
    expect(options.markPaneConnecting).not.toHaveBeenCalled();
    expect(options.markPaneConnectionFailed).not.toHaveBeenCalled();
    expect(mocks.invoke).toHaveBeenCalledTimes(1);
  });
});
