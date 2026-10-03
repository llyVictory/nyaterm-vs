import { backendURL, runtime, requireCapability } from "./runtime";
import { csrfHeader } from "./http";

export function downloadBrowserFile(sessionId: string, path: string): void {
  requireCapability("browserFiles");
  const url = backendURL(
    `api/sessions/${encodeURIComponent(sessionId)}/download`,
  );
  url.searchParams.set("path", path);
  const link = document.createElement("a");
  link.href = url.href;
  link.download = path.split("/").pop() ?? "download";
  document.body.append(link);
  link.click();
  link.remove();
}
export async function uploadBrowserFiles(
  sessionId: string,
  directory: string,
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
    const url = backendURL(
      `api/sessions/${encodeURIComponent(sessionId)}/upload`,
    );
    url.searchParams.set(
      "path",
      `${directory.replace(/\/$/, "")}/${file.name}`,
    );
    const response = await fetch(url, {
      method: "POST",
      credentials: "same-origin",
      headers: { ...csrfHeader(), "Content-Type": "application/octet-stream" },
      body: file,
    });
    if (!response.ok)
      throw new Error((await response.json()).error ?? "Upload failed");
  }
  return true;
}
