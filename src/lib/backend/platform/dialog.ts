import * as native from "@tauri-apps/plugin-dialog";
import { runtime, requireCapability } from "../runtime";
export const open: typeof native.open = (async (
  options: Parameters<typeof native.open>[0],
) => {
  if (runtime === "desktop") return native.open(options);
  requireCapability("nativeFiles");
  return null;
}) as typeof native.open;
export const save: typeof native.save = async (options) => {
  if (runtime === "desktop") return native.save(options);
  requireCapability("nativeFiles");
  return null;
};
export const confirm: typeof native.confirm = async (message, options) =>
  runtime === "desktop"
    ? native.confirm(message, options)
    : window.confirm(message);
export const message: typeof native.message = async (message, options) => {
  if (runtime === "desktop") return native.message(message, options);
  window.alert(message);
  return "Ok";
};
