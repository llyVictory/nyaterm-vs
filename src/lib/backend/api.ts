import type { EventCallback, Options, UnlistenFn } from "@tauri-apps/api/event";
import { runtime } from "./runtime";
import { tauriBackend } from "./tauri";
import { httpInvoke } from "./http";
import { browserEmit, browserListen } from "./events";

export function invoke<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  return runtime === "desktop"
    ? tauriBackend.invoke<T>(command, args)
    : httpInvoke<T>(command, args);
}
export function listen<T>(
  event: string,
  callback: EventCallback<T>,
  options?: Options,
): Promise<UnlistenFn> {
  return runtime === "desktop"
    ? options === undefined
      ? tauriBackend.listen(event, callback)
      : tauriBackend.listen(event, callback, options)
    : browserListen(event, callback);
}
export function emit<T>(event: string, payload?: T): Promise<void> {
  return runtime === "desktop"
    ? tauriBackend.emit(event, payload)
    : browserEmit(event, payload);
}
export type { Event, EventCallback, UnlistenFn } from "@tauri-apps/api/event";
