import * as native from "@tauri-apps/api/path";
import { runtime, requireCapability } from "../runtime";
export const join: typeof native.join = (...parts) =>
  runtime === "desktop"
    ? native.join(...parts)
    : Promise.resolve(parts.join("/").replace(/\/+/g, "/"));
export const downloadDir: typeof native.downloadDir = async () => {
  if (runtime === "desktop") return native.downloadDir();
  return "";
};
export const tempDir: typeof native.tempDir = async () => {
  if (runtime === "desktop") return native.tempDir();
  requireCapability("nativeFiles");
  return "";
};

export const dirname: typeof native.dirname = async (path) =>
  runtime === "desktop"
    ? native.dirname(path)
    : path.slice(0, path.lastIndexOf("/")) || "/";
