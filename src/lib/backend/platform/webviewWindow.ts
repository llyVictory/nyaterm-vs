import { WebviewWindow as NativeWindow } from "@tauri-apps/api/webviewWindow";
import type { EventCallback } from "@tauri-apps/api/event";
import { runtime, backendURL, requireCapability } from "../runtime";
import { getCurrentWindow } from "./window";
import { browserEmit, browserListen } from "../events";

type RegistryWindow = Window & {
  __nyatermBrowserWindows?: Map<string, BrowserWindow>;
};
function registry(): Map<string, BrowserWindow> {
  const host = (window.top ?? window) as RegistryWindow;
  if (!host.__nyatermBrowserWindows) host.__nyatermBrowserWindows = new Map();
  return host.__nyatermBrowserWindows;
}
class BrowserWindow {
  label: string;
  private frame: HTMLIFrameElement;
  private overlay: HTMLDivElement;
  private destroyed = new Set<() => void>();
  constructor(
    label: string,
    options: ConstructorParameters<typeof NativeWindow>[1] = {},
  ) {
    this.label = label;
    const url = new URL(options.url ?? "", backendURL(""));
    url.searchParams.set("webWindowLabel", label);
    const type = url.searchParams.get("window");
    if (
      type &&
      !new Set([
        "settings",
        "new-session",
        "quick-command",
        "file-editor",
        "file-preview",
      ]).has(type)
    )
      requireCapability("nativeFiles");
    const doc = (window.top ?? window).document;
    this.overlay = doc.createElement("div");
    this.overlay.setAttribute("role", "dialog");
    this.overlay.setAttribute("aria-modal", "true");
    Object.assign(this.overlay.style, {
      position: "fixed",
      inset: "0",
      zIndex: "10000",
      display: options.visible === false ? "none" : "flex",
      alignItems: "center",
      justifyContent: "center",
      background: "rgba(0,0,0,.5)",
    });
    this.frame = doc.createElement("iframe");
    this.frame.src = url.href;
    this.frame.title = options.title ?? "NyaTerm";
    Object.assign(this.frame.style, {
      width: `${options.width ?? 960}px`,
      height: `${options.height ?? 720}px`,
      maxWidth: "95vw",
      maxHeight: "95vh",
      border: "0",
      borderRadius: "8px",
    });
    this.overlay.append(this.frame);
    doc.body.append(this.overlay);
    registry().set(label, this);
  }
  static async getByLabel(label: string) {
    return (
      registry().get(label) ??
      (label === "main"
        ? {
            ...getCurrentWindow(),
            label: "main",
            setFocus: async () => (window.top ?? window).focus(),
          }
        : null)
    );
  }
  static async getAll() {
    return [...registry().values()];
  }
  async close() {
    this.overlay.remove();
    registry().delete(this.label);
    for (const callback of this.destroyed) callback();
  }
  async destroy() {
    await this.close();
  }
  async show() {
    this.overlay.style.display = "flex";
  }
  async hide() {
    this.overlay.style.display = "none";
  }
  async setFocus() {
    this.frame.focus();
  }
  async setTitle(title: string) {
    this.frame.title = title;
  }
  async setFocusable() {}
  async isVisible() {
    return this.overlay.isConnected && this.overlay.style.display !== "none";
  }
  async isFocused() {
    return this.frame === this.frame.ownerDocument.activeElement;
  }
  async once<T>(event: string, handler: EventCallback<T>) {
    if (event === "tauri://created") {
      queueMicrotask(() => handler({ event, id: 0, payload: undefined as T }));
      return () => {};
    }
    if (event === "tauri://destroyed") {
      const callback = () => handler({ event, id: 0, payload: undefined as T });
      this.destroyed.add(callback);
      return () => {
        this.destroyed.delete(callback);
      };
    }
    return browserListen(event, handler);
  }
  async listen<T>(event: string, handler: EventCallback<T>) {
    return browserListen(event, handler);
  }
  async emit<T>(event: string, payload?: T) {
    return browserEmit(event, payload);
  }
  async outerSize() {
    return { width: this.frame.clientWidth, height: this.frame.clientHeight };
  }
  async innerSize() {
    return this.outerSize();
  }
  async outerPosition() {
    return { x: this.frame.offsetLeft, y: this.frame.offsetTop };
  }
  async scaleFactor() {
    return 1;
  }
  async setPosition() {}
  async setSize() {}
  async setAlwaysOnTop() {}
  async setEnabled() {}
  async requestUserAttention() {}
}
export type WebviewWindow = NativeWindow;
export const WebviewWindow: typeof NativeWindow =
  runtime === "desktop"
    ? NativeWindow
    : (BrowserWindow as unknown as typeof NativeWindow);
