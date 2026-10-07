import { invoke as tauriInvoke } from "@/lib/backend/api";
import { emit } from "@/lib/backend/api";
import { assertMatchingTemporaryConfig } from "@/lib/appWorkspace";
import { getErrorMessage } from "@/lib/errors";
import { invoke } from "@/lib/invoke";
import { getOwnerMainWindowLabel } from "@/lib/windowManager";
import { logger } from "@/lib/logger";
import { isWindows } from "@/lib/platform";
import {
  buildTerminalCommandInput,
  clearSessionCommandHistory,
  sendSessionInput,
} from "@/lib/sessionInput";
import type { TemporaryLinkConfig } from "@/lib/temporaryLink";
import { captureTerminalReconnectContent } from "@/lib/terminalReconnectHistory";
import type {
  GeneralSettings,
  SavedConnection,
  SessionPane,
  SessionType,
  SshRuntimeMode,
  WorkspaceSessionType,
} from "@/types/global";

export type StartupCommandRequest = {
  command: string;
  delayMs: number;
};

const CONNECTION_SESSION_TYPES: Record<
  SavedConnection["type"],
  WorkspaceSessionType
> = {
  ssh: "SSH",
  local_terminal: "Local",
  telnet: "Telnet",
  serial: "Serial",
  rdp: "RDP",
  vnc: "VNC",
};

export function getConnectionSessionType(
  connection: Pick<SavedConnection, "type"> | null | undefined,
): WorkspaceSessionType {
  return connection ? CONNECTION_SESSION_TYPES[connection.type] : "SSH";
}

export function getRemoteDesktopPaneDisplay(
  connection: SavedConnection | null | undefined,
) {
  if (connection?.type === "rdp") {
    return {
      remoteWidth: connection.display?.width ?? 1920,
      remoteHeight: connection.display?.height ?? 1080,
      scaleMode: connection.display?.mode === "fit-window" ? "fit" : "actual",
    } as const;
  }
  if (connection?.type === "vnc") {
    return {
      scaleMode: connection.display?.scale_mode ?? "fit",
      viewOnly: connection.view_only ?? false,
      clipboardEnabled: connection.clipboard?.enabled ?? true,
    } as const;
  }
  return undefined;
}

export function shouldLaunchSavedRdpWithSystemClient(
  connection: Pick<SavedConnection, "type"> | null | undefined,
  mode: GeneralSettings["rdp_client_mode"] | undefined,
  windows = isWindows,
) {
  return windows && mode === "windows" && connection?.type === "rdp";
}

export async function launchSavedRdpWithSystemClient(
  connection: Pick<SavedConnection, "id" | "type">,
  mode: GeneralSettings["rdp_client_mode"] | undefined,
  windows = isWindows,
) {
  if (!shouldLaunchSavedRdpWithSystemClient(connection, mode, windows)) {
    return false;
  }
  await invoke("launch_windows_rdp", { connectionId: connection.id });
  return true;
}

export function isSessionCreationCancelled(error: unknown) {
  return getErrorMessage(error)
    .toLowerCase()
    .includes("session creation cancelled");
}

export async function attachSessionBeforeClose(sessionId: string) {
  await tauriInvoke("attach_session", { sessionId }).catch((error) => {
    logger.debug({
      domain: "session.lifecycle",
      event: "session.attach_before_close_failed",
      message: "Best-effort attach before close failed",
      ids: { session_id: sessionId },
      error,
    });
  });
}

export async function closeStaleCreatedSession(sessionId: string) {
  try {
    await attachSessionBeforeClose(sessionId);
    await invoke("close_session", { sessionId });
    await invoke("finish_recording_scope", { scopeId: sessionId });
    clearSessionCommandHistory(sessionId);
  } catch (error) {
    logger.error({
      domain: "session.lifecycle",
      event: "session.stale_close_failed",
      message: "Failed to close stale created session",
      ids: { session_id: sessionId },
      error,
    });
  }
}

export async function createSessionForConnection(
  connection: Pick<SavedConnection, "id" | "type">,
  createRequestId?: string,
  startupCommand?: StartupCommandRequest,
  runtimeModeOverride?: SshRuntimeMode,
  recordingScopeId?: string,
) {
  switch (connection.type) {
    case "local_terminal":
      return invoke<string>("create_local_session", {
        connectionId: connection.id,
        createRequestId,
        recordingScopeId,
      });
    case "telnet":
      return invoke<string>("create_telnet_session", {
        connectionId: connection.id,
        createRequestId,
        recordingScopeId,
        startupCommand: buildStartupCommandPayload(startupCommand),
      });
    case "serial":
      return invoke<string>("create_serial_session", {
        connectionId: connection.id,
        createRequestId,
        recordingScopeId,
      });
    case "vnc":
      return invoke<string>("create_vnc_session", {
        ownerWindowLabel: getOwnerMainWindowLabel(),
        connectionId: connection.id,
        createRequestId,
        recordingScopeId,
      });
    case "rdp":
      return invoke<string>("create_rdp_session", {
        connectionId: connection.id,
        createRequestId,
      });
    default:
      return invoke<string>("create_ssh_session", {
        connectionId: connection.id,
        createRequestId,
        startupCommand: buildStartupCommandPayload(startupCommand),
        runtimeMode: runtimeModeOverride,
        recordingScopeId,
      });
  }
}

export async function createTemporarySession(
  config: TemporaryLinkConfig,
  createRequestId?: string,
  startupCommand?: StartupCommandRequest,
  recordingScopeId?: string,
) {
  switch (config.protocol) {
    case "telnet":
      return invoke<string>("create_telnet_session", {
        connectionId: null,
        host: config.host,
        port: config.port,
        name: config.name,
        ...(config.network ? { network: config.network } : {}),
        ...(config.encoding ? { encoding: config.encoding } : {}),
        createRequestId,
        recordingScopeId,
        startupCommand: buildStartupCommandPayload(startupCommand),
      });
    case "serial":
      return invoke<string>("create_serial_session", {
        connectionId: null,
        portName: config.portName,
        baudRate: config.baudRate,
        name: config.name,
        createRequestId,
        recordingScopeId,
      });
    default: {
      const { protocol: _protocol, ...sshConfig } = config;
      return invoke<string>("create_temporary_ssh_session", {
        config: sshConfig,
        createRequestId,
        recordingScopeId,
        startupCommand: buildStartupCommandPayload(startupCommand),
      });
    }
  }
}

export async function createExternalLocalSession(
  workingDir: string | null,
  createRequestId?: string,
  recordingScopeId?: string,
) {
  return invoke<string>("create_local_session", {
    connectionId: null,
    createRequestId,
    recordingScopeId,
    workingDir,
  });
}

export function createSessionForPane(
  pane: Pick<
    SessionPane,
    "id" | "type" | "connectionId" | "temporaryConfig" | "sshRuntimeMode"
  >,
  createRequestId?: string,
  startupCommand?: StartupCommandRequest,
  workingDir?: string,
) {
  switch (pane.type) {
    case "Local":
      return invoke<string>("create_local_session", {
        connectionId: pane.connectionId || null,
        createRequestId,
        recordingScopeId: pane.id,
        ...(workingDir === undefined ? {} : { workingDir }),
      });
    case "Telnet":
      if (pane.connectionId) {
        return invoke<string>("create_telnet_session", {
          connectionId: pane.connectionId,
          createRequestId,
          recordingScopeId: pane.id,
          startupCommand: buildStartupCommandPayload(startupCommand),
          runtimeMode: pane.sshRuntimeMode,
        });
      }
      assertMatchingTemporaryConfig(pane);
      if (pane.temporaryConfig?.protocol === "telnet") {
        return invoke<string>("create_telnet_session", {
          connectionId: null,
          host: pane.temporaryConfig.host,
          port: pane.temporaryConfig.port,
          name: pane.temporaryConfig.name,
          ...(pane.temporaryConfig.network ? { network: pane.temporaryConfig.network } : {}),
          ...(pane.temporaryConfig.encoding ? { encoding: pane.temporaryConfig.encoding } : {}),
          createRequestId,
          recordingScopeId: pane.id,
          startupCommand: buildStartupCommandPayload(startupCommand),
        });
      }
      throw new Error("Missing Telnet connection id");
    case "Serial":
      if (pane.connectionId) {
        return invoke<string>("create_serial_session", {
          connectionId: pane.connectionId,
          createRequestId,
          recordingScopeId: pane.id,
        });
      }
      assertMatchingTemporaryConfig(pane);
      if (pane.temporaryConfig?.protocol === "serial") {
        return invoke<string>("create_serial_session", {
          connectionId: null,
          portName: pane.temporaryConfig.portName,
          baudRate: pane.temporaryConfig.baudRate,
          name: pane.temporaryConfig.name,
          createRequestId,
          recordingScopeId: pane.id,
        });
      }
      throw new Error("Missing Serial connection id");
    case "VNC":
      if (!pane.connectionId) throw new Error("Missing VNC connection id");
      return invoke<string>("create_vnc_session", {
        ownerWindowLabel: getOwnerMainWindowLabel(),
        connectionId: pane.connectionId,
        createRequestId,
        recordingScopeId: pane.id,
      });
    case "RDP":
      if (!pane.connectionId) throw new Error("Missing RDP connection id");
      return invoke<string>("create_rdp_session", {
        connectionId: pane.connectionId,
        createRequestId,
      });
    default:
      if (pane.connectionId) {
        return invoke<string>("create_ssh_session", {
          connectionId: pane.connectionId,
          createRequestId,
          recordingScopeId: pane.id,
          startupCommand: buildStartupCommandPayload(startupCommand),
          runtimeMode: pane.sshRuntimeMode,
        });
      }
      assertMatchingTemporaryConfig(pane);
      if (pane.temporaryConfig?.protocol === "ssh") {
        const { protocol: _protocol, ...sshConfig } = pane.temporaryConfig;
        return invoke<string>("create_temporary_ssh_session", {
          config: sshConfig,
          createRequestId,
          recordingScopeId: pane.id,
          startupCommand: buildStartupCommandPayload(startupCommand),
        });
      }
      throw new Error("Missing SSH connection id");
  }
}

export function getTemporaryLinkSessionType(
  config: TemporaryLinkConfig,
): SessionType {
  switch (config.protocol) {
    case "telnet":
      return "Telnet";
    case "serial":
      return "Serial";
    default:
      return "SSH";
  }
}

export function buildStartupCommandPayload(
  startupCommand?: StartupCommandRequest,
) {
  if (!startupCommand || !startupCommand.command.trim()) return null;
  return {
    command: startupCommand.command,
    delayMs: Math.max(0, Math.min(60000, Math.round(startupCommand.delayMs))),
  };
}

export async function sendStartupCommandToSession(
  sessionId: string,
  startupCommand: StartupCommandRequest,
) {
  const delayMs = Math.max(
    0,
    Math.min(60000, Math.round(startupCommand.delayMs)),
  );
  if (delayMs > 0) {
    await new Promise((resolve) => window.setTimeout(resolve, delayMs));
  }
  await sendSessionInput(
    sessionId,
    buildTerminalCommandInput(startupCommand.command),
    {
      preview: { kind: "reset" },
      registerSubmission: startupCommand.command,
      origin: "startup_command",
    },
  );
}

export function focusTerminalSession(sessionId?: string | null) {
  if (!sessionId) return;
  requestAnimationFrame(() => {
    void emit(`focus-terminal-${sessionId}`);
  });
}

export function capturePaneReconnectContent(pane: SessionPane) {
  if (pane.connecting || pane.connectError) return null;
  return captureTerminalReconnectContent(pane.sessionId);
}
