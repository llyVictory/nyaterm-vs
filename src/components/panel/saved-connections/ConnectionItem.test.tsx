import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { SavedConnection } from "@/types/global";
import ConnectionItem from "./ConnectionItem";
import { SavedConnectionsContext, type SavedConnectionsContextValue } from "./context";

const platformState = vi.hoisted(() => ({ windowsDesktop: true }));
vi.mock("@/lib/backend/runtime", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/backend/runtime")>()),
  canUseWindowsRdpClient: () => platformState.windowsDesktop,
}));

vi.mock("@/components/ui/context-menu", () => ({
  ContextMenu: ({ children }: { children: ReactNode }) => <>{children}</>,
  ContextMenuTrigger: ({ children }: { children: ReactNode }) => <>{children}</>,
  ContextMenuContent: ({ children }: { children: ReactNode }) => <div>{children}</div>,
  ContextMenuItem: ({ children, onClick }: { children: ReactNode; onClick?: () => void }) => (
    <button type="button" onClick={onClick}>
      {children}
    </button>
  ),
  ContextMenuSeparator: () => null,
}));

vi.mock("@/components/ui/tooltip", () => ({
  Tooltip: ({ children }: { children: ReactNode }) => <>{children}</>,
  TooltipTrigger: ({ children }: { children: ReactNode }) => <>{children}</>,
  TooltipContent: ({ children }: { children: ReactNode }) => <div>{children}</div>,
}));

vi.mock("./MoveToGroupMenu", () => ({ MoveToGroupContextMenu: () => null }));

const connection: SavedConnection = {
  id: "ssh-1",
  name: "Production",
  type: "ssh",
  host: "prod.example.com",
  port: 22,
  username: "root",
  tags: ["production", "gpu", "database", "critical"],
};

const handleConnectOnlyMock = vi.fn();
const onEditConnectionMock = vi.fn();

function createContextValue(conn = connection): SavedConnectionsContextValue {
  return {
    isDragEnabled: false,
    isPointerDragEnabled: false,
    dragTarget: null,
    expandedGroups: new Set(),
    selectedConnectionIds: new Set([conn.id]),
    keyboardActiveConnectionId: null,
    savedConnections: [conn],
    savedGroups: [],
    toggleGroup: vi.fn(),
    handleConnect: vi.fn(),
    handleConnectOnly: handleConnectOnlyMock,
    handleOpenSftp: vi.fn(),
    handleConnectSelected: vi.fn(),
    handleCopyConnection: vi.fn(),
    requestMoveConnectionToGroup: vi.fn(),
    requestMoveSelectedConnectionsToGroup: vi.fn(),
    handleConnectionSelectionStart: vi.fn(),
    handleConnectionContextMenu: vi.fn(),
    registerConnectionElement: vi.fn(),
    onEditConnection: onEditConnectionMock,
    onNewConnection: vi.fn(),
    requestDeleteConnection: vi.fn(),
    setRenamingConn: vi.fn(),
    setRenameValue: vi.fn(),
    setDeleteFolderTarget: vi.fn(),
    openNewFolderDialog: vi.fn(),
    openRenameFolderDialog: vi.fn(),
    requestOpenGroupConnections: vi.fn(),
    handleDragStart: vi.fn(),
    handleDragEnd: vi.fn(),
    handleDragEnterItem: vi.fn(),
    handleDragOverItem: vi.fn(),
    handleDragLeaveItem: vi.fn(),
    handleDropItem: vi.fn(),
    handlePointerDragStart: vi.fn(),
    handlePointerDragMove: vi.fn(),
    handlePointerDragEnd: vi.fn(),
    handlePointerDragCancel: vi.fn(),
    t: ((key: string) => key) as SavedConnectionsContextValue["t"],
  };
}

function renderConnectionItem(conn = connection, context = createContextValue(conn)) {
  return render(
    <SavedConnectionsContext.Provider value={context}>
      <ConnectionItem conn={conn} indented={false} />
    </SavedConnectionsContext.Provider>,
  );
}

function getConnectionTrigger(container: HTMLElement) {
  const trigger = container.querySelector<HTMLElement>(`[data-saved-drop-id="${connection.id}"]`);
  if (!trigger) throw new Error("Connection trigger not found");
  return trigger;
}

describe("ConnectionItem", () => {
  beforeEach(() => {
    platformState.windowsDesktop = true;
    handleConnectOnlyMock.mockReset();
    onEditConnectionMock.mockReset();
  });

  it.each(["builtin", "windows"] as const)(
    "temporarily opens only the right-clicked RDP with %s despite a multi-selection",
    (mode) => {
      const rdp = {
        ...connection,
        type: "rdp" as const,
        rdp_client_mode: "windows" as const,
      };
      const context = createContextValue(rdp);
      context.selectedConnectionIds.add("other-connection");
      renderConnectionItem(rdp, context);
      fireEvent.click(
        screen.getByRole("button", {
          name:
            mode === "builtin"
              ? "savedConnections.openWithBuiltinRdp"
              : "savedConnections.openWithWindowsRdp",
        }),
      );
      expect(handleConnectOnlyMock).toHaveBeenCalledExactlyOnceWith(rdp, {
        rdpClientModeOverride: mode,
      });
      expect(context.handleConnectSelected).not.toHaveBeenCalled();
      expect(rdp.rdp_client_mode).toBe("windows");
    },
  );

  it("hides temporary RDP actions on non-Windows desktops and Web", () => {
    platformState.windowsDesktop = false;
    renderConnectionItem({ ...connection, type: "rdp" });
    expect(
      screen.queryByText("savedConnections.openWithBuiltinRdp"),
    ).toBeNull();
    expect(
      screen.queryByText("savedConnections.openWithWindowsRdp"),
    ).toBeNull();
  });

  it("hides temporary RDP actions for other protocols", () => {
    renderConnectionItem();
    expect(
      screen.queryByText("savedConnections.openWithBuiltinRdp"),
    ).toBeNull();
  });

  it("connects exactly once when the focused connection receives Enter", async () => {
    const user = userEvent.setup();
    const { container } = renderConnectionItem();
    const trigger = getConnectionTrigger(container);

    trigger.focus();
    await user.keyboard("{Enter}");

    expect(handleConnectOnlyMock).toHaveBeenCalledTimes(1);
    expect(handleConnectOnlyMock).toHaveBeenCalledWith(connection);
  });

  it("keeps nested action buttons independent from the parent Enter handler", async () => {
    const user = userEvent.setup();
    const { container } = renderConnectionItem();
    const trigger = getConnectionTrigger(container);
    const connectButton = screen.getAllByRole("button", {
      name: "savedConnections.connect",
    })[0];
    const editButton = screen.getAllByRole("button", {
      name: "savedConnections.edit",
    })[0];

    connectButton.focus();
    await user.keyboard("{Enter}");
    expect(handleConnectOnlyMock).toHaveBeenCalledTimes(1);

    handleConnectOnlyMock.mockClear();
    editButton.focus();
    await user.keyboard("{Enter}");

    expect(handleConnectOnlyMock).not.toHaveBeenCalled();
    expect(onEditConnectionMock).toHaveBeenCalledTimes(1);
    expect(onEditConnectionMock).toHaveBeenCalledWith(connection);
    expect(document.activeElement).toBe(trigger);
  });

  it("preserves double-click connect and focuses the item before context-menu edit", () => {
    const { container } = renderConnectionItem();
    const trigger = getConnectionTrigger(container);
    const row = trigger.firstElementChild as HTMLElement;

    fireEvent.doubleClick(row);
    expect(handleConnectOnlyMock).toHaveBeenCalledTimes(1);

    const editButtons = screen.getAllByRole("button", {
      name: "savedConnections.edit",
    });
    fireEvent.click(editButtons[1]);

    expect(onEditConnectionMock).toHaveBeenCalledTimes(1);
    expect(onEditConnectionMock).toHaveBeenCalledWith(connection);
    expect(document.activeElement).toBe(trigger);
  });

  it("shows two compact tags, an overflow count, and all tags in details", () => {
    const { container } = renderConnectionItem();

    expect(container.querySelector('[data-connection-tag="production"]')).not.toBeNull();
    expect(container.querySelector('[data-connection-tag="gpu"]')).not.toBeNull();
    expect(container.querySelector('[data-connection-tag="database"]')).toBeNull();
    expect(container.querySelector("[data-connection-tag-overflow]")?.textContent).toBe("+2");
    expect(screen.queryByText("savedConnections.tags")).not.toBeNull();
    expect(screen.queryByText("production · gpu · database · critical")).not.toBeNull();
  });

  it("renders normally without tags", () => {
    const withoutTags = { ...connection, id: "ssh-2", tags: undefined };
    const { container } = renderConnectionItem(withoutTags);

    expect(container.querySelector("[data-connection-tag]")).toBeNull();
    expect(container.querySelector("[data-connection-tag-overflow]")).toBeNull();
  });
});
