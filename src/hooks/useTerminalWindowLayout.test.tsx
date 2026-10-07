import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { findTerminalWindowLeafByTabId } from "@/lib/tabWindows";
import { createFileDocumentPane, createSessionPane, createWorkspaceTab } from "@/lib/workspaceTabs";
import type { RestorableTerminalWindowNode, Tab } from "@/types/global";
import { useTerminalWindowLayout } from "./useTerminalWindowLayout";

function renderLayout(tabs: Tab[], savedLayout: RestorableTerminalWindowNode | null = null) {
  const options: Parameters<typeof useTerminalWindowLayout>[0] = {
    tabs,
    activeTabId: tabs[0]?.id ?? null,
    setActiveTabId: vi.fn(),
    setActivePane: vi.fn(),
    updateSplitRatio: vi.fn(),
    updateUi: vi.fn(),
    persistTabsNow: vi.fn().mockResolvedValue(undefined),
    settingsLoaded: false,
    startupRestoreComplete: false,
    general: {
      startup_restore: true,
      startup_restore_window_layout: true,
      minimize_to_tray: false,
      boss_key: null,
      confirm_on_close: true,
      rdp_client_mode: "builtin",
    },
    savedLayout,
  };
  return { ...renderHook(useTerminalWindowLayout, { initialProps: options }), options };
}

function terminalTab(index: number) {
  return createWorkspaceTab(createSessionPane(`Host ${index}`, "SSH"), index);
}

describe("useTerminalWindowLayout", () => {
  it("waits for startup restoration and preserves each restored leaf's active tab", () => {
    const tabs = [terminalTab(0), terminalTab(1), terminalTab(2)];
    const savedLayout: RestorableTerminalWindowNode = {
      kind: "split",
      direction: "vertical",
      ratio: 0.4,
      first: { kind: "leaf", tab_indexes: [0, 1], active_tab_index: 1 },
      second: { kind: "leaf", tab_indexes: [2], active_tab_index: 2 },
    };
    const { result, rerender, options } = renderLayout(tabs, savedLayout);
    expect(result.current.terminalWindows).toBeNull();
    rerender({ ...options, settingsLoaded: true });
    expect(result.current.terminalWindows).toBeNull();
    expect(options.updateUi).not.toHaveBeenCalled();

    const ready = { ...options, settingsLoaded: true, startupRestoreComplete: true };
    rerender(ready);
    const layout = result.current.terminalWindows;
    expect(layout).toMatchObject({ kind: "split", ratio: 0.4 });
    expect(layout && findTerminalWindowLeafByTabId(layout, tabs[0].id)?.activeTabId).toBe(
      tabs[1].id,
    );
    expect(options.updateUi).not.toHaveBeenCalled();

    // Restored leaf selections survive unrelated renders until the user selects a tab.
    rerender({ ...ready, tabs: [...tabs] });
    const restored = result.current.terminalWindows;
    expect(restored && findTerminalWindowLeafByTabId(restored, tabs[0].id)?.activeTabId).toBe(
      tabs[1].id,
    );
    const leaf = restored && findTerminalWindowLeafByTabId(restored, tabs[0].id);
    if (!leaf) throw new Error("Expected a restored leaf");
    act(() => result.current.handleSelectLeafTab(leaf.id, tabs[0].id));
    expect(options.setActiveTabId).toHaveBeenCalledWith(tabs[0].id);
    expect(options.updateUi).toHaveBeenLastCalledWith({
      terminal_window_layout: {
        ...savedLayout,
        first: { ...savedLayout.first, active_tab_index: 0 },
      },
    });
  });

  it("persists the latest split ratio and excludes file-only tabs when closing", async () => {
    const fileTab = createWorkspaceTab(
      createFileDocumentPane({
        sessionId: "ssh-1",
        name: "notes.md",
        type: "SSH",
        backend: "remote",
        path: "/notes.md",
        file: { content: "notes", size: 5, mtime: 0, contentHash: "hash" },
      }),
      0,
    );
    const tabs = [fileTab, terminalTab(1), terminalTab(2)];
    const { result, rerender, options } = renderLayout(tabs);
    rerender({ ...options, settingsLoaded: true, startupRestoreComplete: true });
    const initialLeaf = result.current.terminalWindows;
    if (initialLeaf?.kind !== "leaf") throw new Error("Expected a leaf");
    act(() => result.current.handleSplitTabToLeaf(tabs[2].id, initialLeaf.id, "right"));
    const split = result.current.terminalWindows;
    if (split?.kind !== "split") throw new Error("Expected a split");
    act(() => result.current.handleUpdateWindowSplitRatio(split.id, 0.7));
    await act(() => result.current.persistWorkspaceLayoutNow());
    expect(options.persistTabsNow).toHaveBeenLastCalledWith({
      terminal_window_layout: {
        kind: "split",
        direction: split.direction,
        ratio: 0.7,
        first: { kind: "leaf", tab_indexes: [0], active_tab_index: 0 },
        second: { kind: "leaf", tab_indexes: [1], active_tab_index: 1 },
      },
    });
  });

  it("uses plain tab persistence before hydration and honors disabled layout restoration", async () => {
    const tabs = [terminalTab(0)];
    const { result, rerender, options } = renderLayout(tabs);
    await act(() => result.current.persistWorkspaceLayoutNow());
    expect(options.persistTabsNow).toHaveBeenLastCalledWith();
    rerender({
      ...options,
      settingsLoaded: true,
      startupRestoreComplete: true,
      general: { ...options.general, startup_restore_window_layout: false },
    });
    await act(() => result.current.persistWorkspaceLayoutNow());
    expect(options.persistTabsNow).toHaveBeenLastCalledWith({ terminal_window_layout: null });
  });
});
