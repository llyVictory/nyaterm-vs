import * as native from "@tauri-apps/plugin-opener";
import { runtime, requireCapability } from "../runtime";
export const openUrl: typeof native.openUrl = async (url, openWith) => {
  if (runtime === "desktop") return native.openUrl(url, openWith);
  const target = new URL(url, window.location.href);
  if (!new Set(["https:", "http:", "mailto:"]).has(target.protocol))
    throw new Error("Unsupported URL protocol");
  window.open(target, "_blank", "noopener,noreferrer");
};
export const openPath: typeof native.openPath = async (path, openWith) => {
  if (runtime === "desktop") return native.openPath(path, openWith);
  requireCapability("nativeFiles");
};
export const revealItemInDir: typeof native.revealItemInDir = async (path) => {
  if (runtime === "desktop") return native.revealItemInDir(path);
  requireCapability("nativeFiles");
};
