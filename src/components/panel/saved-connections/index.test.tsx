import { act, render } from "@testing-library/react";
import type { MouseEvent } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { SavedConnection } from "@/types/global";
import type { SavedConnectionsContextValue } from "./context";
import SavedConnections from "./index";

const state = vi.hoisted(() => ({
  context: null as SavedConnectionsContextValue | null,
  connections: [] as SavedConnection[],
  invoke: vi.fn(),
  updateUi: vi.fn(),
  refresh: vi.fn(),
}));
vi.mock("@/lib/invoke", () => ({ invoke: state.invoke }));
vi.mock("@/context/AppContext", () => ({
  useApp: () => ({
    savedConnections: state.connections,
    savedGroups: [],
    refreshConnections: state.refresh,
    appSettings: { ui: {}, keybindings: {} },
    updateUi: state.updateUi,
  }),
}));
vi.mock("@/hooks/useConfigTransfer", () => ({
  useConfigTransfer: () => ({ handleExport: vi.fn(), passwordAlert: null }),
}));
vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));
vi.mock("./ConnectionItem", async () => {
  const { useSavedConnectionsContext } = await import("./context");
  return {
    default: () => {
      state.context = useSavedConnectionsContext();
      return null;
    },
  };
});
vi.mock("@/components/dialog/connections/ClearAllDialog", () => ({
  default: () => null,
}));
vi.mock("@/components/dialog/connections/DeleteConnectionDialog", () => ({
  default: () => null,
}));
vi.mock("@/components/dialog/connections/DeleteFolderDialog", () => ({
  default: () => null,
}));
vi.mock("@/components/dialog/connections/FolderDialog", () => ({
  default: () => null,
}));
vi.mock("@/components/dialog/connections/ImportDialog", () => ({
  default: () => null,
}));
vi.mock("@/components/dialog/connections/OpenGroupConnectionsDialog", () => ({
  default: () => null,
}));
vi.mock("@/components/dialog/connections/RenameConnectionDialog", () => ({
  default: () => null,
}));

function renderPanel(connect = vi.fn().mockResolvedValue(undefined)) {
  render(
    <SavedConnections
      onTemporarySshLink={vi.fn()}
      onNewConnection={vi.fn()}
      onEditConnection={vi.fn()}
      onConnectConnection={connect}
      onOpenSftpConnection={vi.fn()}
    />,
  );
  return connect;
}

describe("saved connection client overrides", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    state.invoke.mockResolvedValue("copy-id");
    state.connections = [
      {
        id: "rdp-1",
        name: "RDP",
        type: "rdp",
        host: "host",
        rdp_client_mode: "windows",
      },
      {
        id: "rdp-2",
        name: "Other",
        type: "rdp",
        host: "other",
        rdp_client_mode: "builtin",
      },
    ];
  });

  it("forwards a temporary override and prevents concurrent duplicate opens", async () => {
    let finish!: () => void;
    const connect = renderPanel(
      vi.fn(
        () =>
          new Promise<void>((resolve) => {
            finish = resolve;
          }),
      ),
    );
    act(() => {
      state.context?.handleConnectOnly(state.connections[0], {
        rdpClientModeOverride: "builtin",
      });
      state.context?.handleConnectOnly(state.connections[0], {
        rdpClientModeOverride: "windows",
      });
    });
    expect(connect).toHaveBeenCalledExactlyOnceWith(state.connections[0], {
      rdpClientModeOverride: "builtin",
    });
    expect(state.invoke).not.toHaveBeenCalled();
    expect(state.connections[0].rdp_client_mode).toBe("windows");
    await act(async () => finish());
    act(() =>
      state.context?.handleConnectOnly(state.connections[0], {
        rdpClientModeOverride: "windows",
      }),
    );
    expect(connect).toHaveBeenCalledTimes(2);
    await act(async () => finish());
  });

  it("passes each selected connection's preference without a temporary override", async () => {
    const connect = renderPanel();
    for (const connection of state.connections) {
      act(() =>
        state.context?.handleConnectionSelectionStart(connection, {
          button: 0,
          ctrlKey: true,
        } as MouseEvent),
      );
    }
    await act(async () => state.context?.handleConnectSelected());
    expect(connect).toHaveBeenNthCalledWith(1, state.connections[0], undefined);
    expect(connect).toHaveBeenNthCalledWith(2, state.connections[1], undefined);
  });

  it("preserves the client preference when copying a saved connection", async () => {
    renderPanel();
    await act(async () =>
      state.context?.handleCopyConnection(state.connections[0]),
    );
    expect(state.invoke).toHaveBeenCalledWith("save_connection", {
      connection: {
        ...state.connections[0],
        id: "",
        name: "RDP (copy)",
        password: undefined,
      },
    });
    expect(state.refresh).toHaveBeenCalledOnce();
  });
});
