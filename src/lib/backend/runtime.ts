export type Capability =
  | "ssh"
  | "telnet"
  | "vnc"
  | "networkProxy"
  | "sftp"
  | "ai"
  | "settings"
  | "browserFiles"
  | "plugins"
  | "localShell"
  | "serial"
  | "nativeWindows"
  | "nativeFiles"
  | "tray"
  | "globalShortcuts"
  | "notifications"
  | "credentialStore"
  | "updater"
  | "zmodem"
  | "nativePlugins"
  | "remoteDesktop"
  | "aiAgents"
  | "sshAgent"
  | "recording"
  | "remoteMonitoring"
  | "transferControl"
  | "legacyEncoding"
  | "shellIntegration"
  | "sshExtensions";

export const runtime =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window
    ? "desktop"
    : "web";
const webCapabilities = new Set<Capability>([
  "ssh",
  "telnet",
  "vnc",
  "networkProxy",
  "sftp",
  "ai",
  "settings",
  "browserFiles",
  "plugins",
]);
export function supports(capability: Capability): boolean {
  return runtime === "desktop" || webCapabilities.has(capability);
}
export function requireCapability(capability: Capability): void {
  if (!supports(capability))
    throw new Error(`Capability unavailable in Web mode: ${capability}`);
}
/** Build and reverse-proxy base paths are supplied by Vite, never by localhost. */
export function backendURL(path: string): URL {
  const base = new URL(import.meta.env.BASE_URL, window.location.origin);
  if (base.origin !== window.location.origin)
    throw new Error("Backend must use the same origin");
  return new URL(path.replace(/^\//, ""), base);
}

const webPanels = new Set([
  "savedConnections",
  "activeSessions",
  "fileExplorer",
  "securityAuth",
  "aiAssistant",
  "settings",
  "plugins",
  "network",
]);
export function supportsPanel(id: string): boolean {
  return runtime === "desktop" || webPanels.has(id);
}

export function supportsSettingsTab(id: string): boolean {
  return (
    runtime === "desktop" ||
    !new Set(["ai-agents", "security", "syncBackup", "translation"]).has(id)
  );
}
