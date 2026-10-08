import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const platform = vi.hoisted(() => ({ windows: true }));
vi.mock("@/lib/platform", () => ({
  get isWindows() {
    return platform.windows;
  },
}));

describe("Windows RDP capability", () => {
  beforeEach(() => vi.resetModules());
  afterEach(() => Reflect.deleteProperty(window, "__TAURI_INTERNALS__"));

  it.each([
    [true, true, true],
    [true, false, false],
    [false, true, false],
    [false, false, false],
  ])("gates Windows %s and desktop %s", async (windows, desktop, expected) => {
    platform.windows = windows;
    if (desktop)
      Object.defineProperty(window, "__TAURI_INTERNALS__", {
        configurable: true,
        value: {},
      });
    else Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
    const { canUseWindowsRdpClient, supportsSettingsTab } =
      await import("./runtime");
    expect(canUseWindowsRdpClient()).toBe(expected);
    expect(supportsSettingsTab("remote-desktop")).toBe(expected);
    expect(supportsSettingsTab("general")).toBe(true);
  });
});
