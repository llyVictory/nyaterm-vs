import { beforeEach, describe, expect, it, vi } from "vitest";
import { setOwnerMainWindowLabel } from "./windowManager";
import type { SavedConnection, TerminalSessionPane } from "@/types/global";
import {
  createTemporarySession,
  createSessionForConnection,
  createSessionForPane,
  launchSavedRdpWithSystemClient,
  shouldLaunchSavedRdpWithSystemClient,
  resolveRdpClientMode,
} from "./appSessionFactory";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("@/lib/invoke", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn() }));

describe("SSH runtime mode creation", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue("session-1");
  });

  it("sends the explicit SFTP runtime through the existing create command", async () => {
    const connection = { id: "ssh-1", type: "ssh" } as SavedConnection;

    await createSessionForConnection(connection, "request-1", undefined, "sftp");

    expect(invokeMock).toHaveBeenCalledWith("create_ssh_session", {
      connectionId: "ssh-1",
      createRequestId: "request-1",
      startupCommand: null,
      runtimeMode: "sftp",
      recordingScopeId: undefined,
    });
  });

  it("reuses the pane runtime for reconnect and startup restoration", async () => {
    const pane = {
      id: "pane-1",
      type: "SSH",
      connectionId: "ssh-1",
      sshRuntimeMode: "sftp",
    } as TerminalSessionPane;

    await createSessionForPane(pane, "request-2");

    expect(invokeMock).toHaveBeenCalledWith("create_ssh_session", {
      connectionId: "ssh-1",
      createRequestId: "request-2",
      startupCommand: null,
      runtimeMode: "sftp",
      recordingScopeId: "pane-1",
    });
  });

  it("passes an explicit directory to a duplicated saved local terminal", async () => {
    const pane = {
      id: "pane-local",
      type: "Local",
      connectionId: "local-1",
    } as TerminalSessionPane;
    await createSessionForPane(pane, "request-local", undefined, "D:\\My Files");
    expect(invokeMock).toHaveBeenCalledWith("create_local_session", {
      connectionId: "local-1",
      createRequestId: "request-local",
      workingDir: "D:\\My Files",
      recordingScopeId: "pane-local",
    });
  });
});

describe("VNC owner window routing", () => {
  beforeEach(() => {
    invokeMock.mockReset().mockResolvedValue("vnc-session");
  });

  it.each(["main", "main-second"])(
    "passes owner %s through creation and pane recreation",
    async (owner) => {
      setOwnerMainWindowLabel(owner);
      await createSessionForConnection(
        { id: "pi", type: "vnc" },
        "create-request",
      );
      await createSessionForPane(
        { id: "pane", connectionId: "pi", type: "VNC" },
        "recreate-request",
      );
      expect(invokeMock).toHaveBeenNthCalledWith(1, "create_vnc_session", {
        connectionId: "pi",
        createRequestId: "create-request",
        recordingScopeId: undefined,
        ownerWindowLabel: owner,
      });
      expect(invokeMock).toHaveBeenNthCalledWith(2, "create_vnc_session", {
        connectionId: "pi",
        createRequestId: "recreate-request",
        recordingScopeId: "pane",
        ownerWindowLabel: owner,
      });
    },
  );
});


it("retains the temporary Telnet route and encoding when connecting and reconnecting", async () => {
  const temporary = { protocol: "telnet" as const, name: "temporary", host: "host", port: 23, network: { proxy_id: "proxy", proxy_jump_id: "jump" }, encoding: "GBK" };
  await createTemporarySession(temporary, "temporary-create");
  await createSessionForPane({ id: "pane-telnet", type: "Telnet", temporaryConfig: temporary }, "temporary-reconnect");
  expect(invokeMock).toHaveBeenCalledWith("create_telnet_session", expect.objectContaining({ network: temporary.network, encoding: "GBK", createRequestId: "temporary-create" }));
  expect(invokeMock).toHaveBeenCalledWith("create_telnet_session", expect.objectContaining({ network: temporary.network, encoding: "GBK", createRequestId: "temporary-reconnect", recordingScopeId: "pane-telnet" }));
});

describe("Windows system RDP routing", () => {
  beforeEach(() => {
    invokeMock.mockReset().mockResolvedValue(undefined);
  });

  it("recreates an existing RDP pane internally for reconnect and split operations", async () => {
    await createSessionForPane({ id: "rdp-pane", type: "RDP", connectionId: "rdp-1" }, "recreate-rdp");
    expect(invokeMock).toHaveBeenCalledExactlyOnceWith("create_rdp_session", {
      connectionId: "rdp-1",
      createRequestId: "recreate-rdp",
    });
  });

  it.each([
    [undefined, undefined, undefined, "builtin"],
    [undefined, "windows", undefined, "windows"],
    ["builtin", "windows", undefined, "builtin"],
    ["windows", "builtin", undefined, "windows"],
    ["windows", "windows", "builtin", "builtin"],
    ["builtin", "builtin", "windows", "windows"],
  ] as const)(
    "resolves connection %s, default %s, override %s to %s",
    (connectionMode, globalMode, override, expected) => {
      const connection = {
        type: "rdp" as const,
        rdp_client_mode: connectionMode,
      };
      expect(
        resolveRdpClientMode(connection, globalMode, {
          windows: true,
          desktop: true,
          rdpClientModeOverride: override,
        }),
      ).toBe(expected);
      expect(connection.rdp_client_mode).toBe(connectionMode);
    },
  );

  it("does not invoke a native client in Web even on Windows", async () => {
    expect(
      await launchSavedRdpWithSystemClient(
        { id: "rdp-web", type: "rdp", rdp_client_mode: "windows" },
        "windows",
        { windows: true, desktop: false, rdpClientModeOverride: "windows" },
      ),
    ).toBe(false);
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("routes a mixed batch according to each connection's preference", async () => {
    const connections: SavedConnection[] = [
      { id: "inherited", name: "Inherited", type: "rdp" },
      {
        id: "builtin",
        name: "Built-in",
        type: "rdp",
        rdp_client_mode: "builtin",
      },
      { id: "ssh", name: "SSH", type: "ssh" },
    ];
    const routed = await Promise.all(
      connections.map((connection) =>
        launchSavedRdpWithSystemClient(connection, "windows", {
          windows: true,
          desktop: true,
        }),
      ),
    );
    expect(routed).toEqual([true, false, false]);
    expect(invokeMock).toHaveBeenCalledExactlyOnceWith("launch_windows_rdp", {
      connectionId: "inherited",
    });
  });

  it("propagates a launch failure without falling back to session creation", async () => {
    invokeMock.mockRejectedValueOnce(new Error("mstsc unavailable"));
    await expect(
      launchSavedRdpWithSystemClient(
        { id: "rdp-failed", type: "rdp" },
        "windows",
        { windows: true, desktop: true },
      ),
    ).rejects.toThrow("mstsc unavailable");
    expect(invokeMock).toHaveBeenCalledTimes(1);
  });

  it("launches a saved RDP connection externally only on Windows system mode", async () => {
    const connection = { id: "rdp-1", type: "rdp" } as SavedConnection;

    expect(
      shouldLaunchSavedRdpWithSystemClient(connection, "windows", { windows: true, desktop: true }),
    ).toBe(true);
    expect(
      await launchSavedRdpWithSystemClient(connection, "windows", { windows: true, desktop: true }),
    ).toBe(true);
    expect(invokeMock).toHaveBeenCalledExactlyOnceWith("launch_windows_rdp", {
      connectionId: "rdp-1",
    });
  });

  it("keeps built-in and non-Windows RDP on the existing session path", async () => {
    const connection = { id: "rdp-1", type: "rdp" } as SavedConnection;

    expect(
      shouldLaunchSavedRdpWithSystemClient(connection, "builtin", { windows: true, desktop: true }),
    ).toBe(false);
    expect(
      shouldLaunchSavedRdpWithSystemClient(connection, "windows", { windows: false, desktop: true }),
    ).toBe(false);
    expect(
      await launchSavedRdpWithSystemClient(connection, "builtin", { windows: true, desktop: true }),
    ).toBe(false);
    expect(
      await launchSavedRdpWithSystemClient(connection, "windows", { windows: false, desktop: true }),
    ).toBe(false);
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("does not route non-RDP saved connections to Windows Remote Desktop", async () => {
    const connection = { id: "ssh-1", type: "ssh" } as SavedConnection;

    expect(
      await launchSavedRdpWithSystemClient(connection, "windows", { windows: true, desktop: true }),
    ).toBe(false);
    expect(invokeMock).not.toHaveBeenCalled();
  });
});
