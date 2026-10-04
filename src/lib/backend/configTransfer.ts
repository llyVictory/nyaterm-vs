import { downloadBlob } from "./browserArtifacts";
import { createBrowserRequestId } from "./browserDiagnostics";
import { BackendRequestError, csrfHeader } from "./http";
import { backendURL } from "./runtime";

export async function artifactRequest(path: string, init: RequestInit = {}): Promise<Response> {
  const requestId = createBrowserRequestId();
  const response = await fetch(backendURL(path), {
    ...init,
    credentials: "same-origin",
    headers: {
      "X-Nyaterm-Request": "1",
      "X-Nyaterm-Request-Id": requestId,
      ...csrfHeader(),
      ...init.headers,
    },
  });
  if (!response.ok) {
    const value = await response.json().catch(() => ({}));
    throw new BackendRequestError(
      value.error ?? `Backend request failed (${response.status})`,
      response.status,
      response.headers.get("X-Nyaterm-Request-Id") ?? requestId,
    );
  }
  return response;
}
export async function exportBrowserConfig(password: string): Promise<void> {
  const response = await artifactRequest("api/backups/export", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ password }),
  });
  downloadBlob("nyaterm-backup.nya", await response.blob());
}
export async function importBrowserConfig(file: File, password: string): Promise<void> {
  if (file.size > 50 * 1024 * 1024) throw new Error("Backup exceeds 50 MiB");
  const body = new FormData();
  body.append("password", password);
  body.append("file", file);
  await artifactRequest("api/backups/import", { method: "POST", body });
}
export async function importBrowserConnections(
  source: string,
  file: File,
): Promise<{ imported: number; warnings: string[] }> {
  if (file.size > 10 * 1024 * 1024) throw new Error("Import exceeds 10 MiB");
  const body = new FormData();
  body.append("source", source);
  body.append("file", file);
  return (await artifactRequest("api/imports/connections", { method: "POST", body })).json();
}
export async function exportBrowserDiagnostics(): Promise<void> {
  downloadBlob(
    "nyaterm-diagnostics.zip",
    await (await artifactRequest("api/diagnostics/export")).blob(),
  );
}
