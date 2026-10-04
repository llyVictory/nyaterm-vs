import { backendURL, runtime, requireCapability } from "./runtime";
import { csrfHeader } from "./http";

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
  const url = backendURL(
    `api/sessions/${encodeURIComponent(sessionId)}/upload`,
  );
  url.searchParams.set("path", path);
  const response = await fetch(url, {
    method: "POST",
    credentials: "same-origin",
    headers: { ...csrfHeader(), "Content-Type": "application/octet-stream" },
    body: file,
  });
  const result = await response.json();
  if (!response.ok) throw new Error(result.error ?? "Upload failed");
  return result as BrowserUploadResult;
}

export async function downloadBrowserFile(
  sessionId: string,
  path: string,
): Promise<void> {
  requireCapability("browserFiles");
  const url = backendURL(
    `api/sessions/${encodeURIComponent(sessionId)}/download`,
  );
  url.searchParams.set("path", path);
  const link = document.createElement("a");
  const response = await fetch(url, { credentials: "same-origin" });
  if (!response.ok) {
    const result = await response.json().catch(() => ({}));
    throw new Error(result.error ?? "Download failed");
  }
  const objectURL = URL.createObjectURL(await response.blob());
  link.href = objectURL;
  link.download = path.split("/").pop() ?? "download";
  document.body.append(link);
  link.click();
  link.remove();
  window.setTimeout(() => URL.revokeObjectURL(objectURL), 60_000);
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
