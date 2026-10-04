import { downloadBlob } from "./browserArtifacts";
import { createBrowserRequestId } from "./browserDiagnostics";
import { BackendRequestError, csrfHeader } from "./http";
import { backendURL, requireCapability, runtime } from "./runtime";

export interface BrowserUploadResult {
  bytes: number;
  status: "completed" | "skipped";
  path: string;
}

export async function uploadBrowserFile(
  sessionId: string,
  path: string,
  file: Blob,
): Promise<BrowserUploadResult> {
  const url = backendURL(`api/sessions/${encodeURIComponent(sessionId)}/upload`);
  url.searchParams.set("path", path);
  const requestId = createBrowserRequestId();
  const response = await fetch(url, {
    method: "POST",
    credentials: "same-origin",
    headers: {
      ...csrfHeader(),
      "X-Nyaterm-Request-Id": requestId,
      "Content-Type": "application/octet-stream",
    },
    body: file,
  });
  const result = await response.json();
  if (!response.ok)
    throw new BackendRequestError(
      result.error ?? "Upload failed",
      response.status,
      response.headers.get("X-Nyaterm-Request-Id") ?? requestId,
    );
  return result as BrowserUploadResult;
}

export async function downloadBrowserFile(sessionId: string, path: string): Promise<void> {
  requireCapability("browserFiles");
  const url = backendURL(`api/sessions/${encodeURIComponent(sessionId)}/download`);
  url.searchParams.set("path", path);
  const requestId = createBrowserRequestId();
  const response = await fetch(url, {
    credentials: "same-origin",
    headers: { "X-Nyaterm-Request-Id": requestId },
  });
  if (!response.ok) {
    const result = await response.json().catch(() => ({}));
    throw new BackendRequestError(
      result.error ?? "Download failed",
      response.status,
      response.headers.get("X-Nyaterm-Request-Id") ?? requestId,
    );
  }
  downloadBlob(path.split("/").pop() ?? "download", await response.blob());
}
export async function uploadBrowserFiles(
  sessionId: string,
  directory: string,
  onResult?: (result: BrowserUploadResult, requestedPath: string) => void,
): Promise<boolean> {
  if (runtime === "desktop") return false;
  const picker = document.createElement("input");
  picker.type = "file";
  picker.multiple = true;
  const files = await new Promise<File[]>((resolve) => {
    picker.onchange = () => resolve(Array.from(picker.files ?? []));
    picker.oncancel = () => resolve([]);
    picker.click();
  });
  for (const file of files) {
    const path = `${directory.replace(/\/$/, "")}/${file.name}`;
    const result = await uploadBrowserFile(sessionId, path, file);
    onResult?.(result, path);
  }
  return true;
}
