import * as native from "@tauri-apps/api/window";
import { runtime, requireCapability } from "../runtime";
import { browserListen, browserEmit } from "../events";

export { PhysicalPosition, UserAttentionType } from "@tauri-apps/api/window";
export type { Window } from "@tauri-apps/api/window";
const noop = async () => {};
const browserWindow = {
  label:
    new URLSearchParams(window.location.search).get("webWindowLabel") ?? "main",
  close: async () => {
    const label = new URLSearchParams(window.location.search).get(
      "webWindowLabel",
    );
    if (label) {
      const { WebviewWindow } = await import("./webviewWindow");
      await (await WebviewWindow.getByLabel(label))?.close();
    }
  },
  destroy: async () => browserWindow.close(),
  setTitle: async (title: string) => {
    document.title = title;
  },
  setFocus: async () => window.focus(),
  isFocused: async () => document.hasFocus(),
  isMaximized: async () => false,
  isFullscreen: async () => Boolean(document.fullscreenElement),
  isVisible: async () => true,
  onFocusChanged: async (handler: (event: { payload: boolean }) => void) => {
    const focus = () => handler({ payload: true });
    const blur = () => handler({ payload: false });
    window.addEventListener("focus", focus);
    window.addEventListener("blur", blur);
    return () => {
      window.removeEventListener("focus", focus);
      window.removeEventListener("blur", blur);
    };
  },
  onResized: async (
    handler: (event: { payload: { width: number; height: number } }) => void,
  ) => {
    const resized = () =>
      handler({ payload: { width: innerWidth, height: innerHeight } });
    window.addEventListener("resize", resized);
    return () => window.removeEventListener("resize", resized);
  },
  onCloseRequested: async () => noop,
  listen: browserListen,
  emit: browserEmit,
  show: noop,
  hide: noop,
  setEnabled: noop,
  setFocusable: noop,
  setAlwaysOnTop: noop,
  setShadow: noop,
  setDecorations: noop,
  setSkipTaskbar: noop,
  setBackgroundColor: noop,
  setResizable: noop,
  setMinSize: noop,
  requestUserAttention: noop,
  startDragging: noop,
  minimize: async () => requireCapability("nativeWindows"),
  toggleMaximize: async () => requireCapability("nativeWindows"),
  innerPosition: async () => ({ x: 0, y: 0 }),
  outerPosition: async () => ({ x: 0, y: 0 }),
  outerSize: async () => ({ width: outerWidth, height: outerHeight }),
  innerSize: async () => ({ width: innerWidth, height: innerHeight }),
  scaleFactor: async () => devicePixelRatio,
  setPosition: noop,
  setSize: noop,
  setFullscreen: async (fullscreen: boolean) => {
    if (fullscreen) await document.documentElement.requestFullscreen();
    else await document.exitFullscreen();
  },
} as unknown as native.Window;
export function getCurrentWindow(): native.Window {
  return runtime === "desktop" ? native.getCurrentWindow() : browserWindow;
}
export const availableMonitors: typeof native.availableMonitors = () =>
  runtime === "desktop" ? native.availableMonitors() : Promise.resolve([]);
export const primaryMonitor: typeof native.primaryMonitor = () =>
  runtime === "desktop" ? native.primaryMonitor() : Promise.resolve(null);
export const currentMonitor: typeof native.currentMonitor = () =>
  runtime === "desktop" ? native.currentMonitor() : Promise.resolve(null);
