import { beforeEach, describe, expect, it, vi } from "vitest";
import { setOwnerMainWindowLabel } from "./windowManager";
import type { SavedConnection, TerminalSessionPane } from "@/types/global";
import { createTemporarySession, createSessionForConnection, createSessionForPane } from "./appSessionFactory";

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
