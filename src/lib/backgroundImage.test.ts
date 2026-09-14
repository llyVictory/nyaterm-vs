import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppearanceSettings } from "@/types/global";
import { resolveTheme } from "./themes";

vi.mock("./invoke", () => ({ invoke: vi.fn() }));
vi.mock("./logger", () => ({
  logger: {
    warn: vi.fn(),
  },
}));

const themeColors = resolveTheme("github-dark").colors;

function appearance(overrides: Partial<AppearanceSettings> = {}): AppearanceSettings {
  return {
    theme: "github-dark",
    custom_themes: [],
    font_family: "JetBrains Mono",
    ui_font_family: "Inter",
    font_size: 14,
    font_weight: 400,
    font_weight_bold: 700,
    bold_default_foreground_highlight: false,
    background_opacity: 1,
    background_image_path: null,
    background_image_fit: "cover",
    background_image_opacity: 0.45,
    cursor_style: "block",
    cursor_blink: true,
    ui_font_size: 16,
    terminal_theme: null,
    minimum_contrast_ratio: 1,
    panel_multi_open: false,
    window_transparency: "none",
    window_transparency_tint: 1,
    window_transparency_blur: false,
    ...overrides,
  };
}

function setNavigator(userAgent: string, platform = "") {
  Object.defineProperty(window.navigator, "userAgent", {
    configurable: true,
    value: userAgent,
  });
  Object.defineProperty(window.navigator, "platform", {
    configurable: true,
    value: platform,
  });
}

async function importBackgroundImage() {
  vi.resetModules();
  return import("./backgroundImage");
}

describe("terminal surface background variables", () => {
  beforeEach(() => {
    setNavigator("Mozilla/5.0 (X11; Linux x86_64)", "Linux x86_64");
  });

  it("keeps the terminal surface pointed at the terminal theme background in normal mode", async () => {
    const { buildSurfaceCssVariables, buildTerminalThemeColors } = await importBackgroundImage();
    const cssVars = buildSurfaceCssVariables(themeColors, appearance());
    const terminalColors = buildTerminalThemeColors(themeColors.terminal, appearance());

    expect(cssVars["--df-bg-terminal"]).toBe(themeColors.bgTerminal);
    expect(cssVars["--df-bg-terminal-solid"]).toBe(themeColors.bgTerminal);
    expect(cssVars["--df-terminal-surface-bg"]).toBe(
      "var(--df-terminal-bg, var(--df-bg-terminal))",
    );
    expect(terminalColors.background).toBe(themeColors.terminal.background);
    expect(terminalColors).not.toHaveProperty("foregroundIntense");
  });

  it("passes the configured intense foreground only when bold default highlighting is enabled", async () => {
    const { buildTerminalThemeColors } = await importBackgroundImage();
    const terminalColors = buildTerminalThemeColors(
      { ...themeColors.terminal, foregroundIntense: "#abcdef" },
      appearance({ bold_default_foreground_highlight: true }),
    );

    expect(terminalColors.foregroundIntense).toBe("#abcdef");
  });

  it("falls back to the normal foreground for legacy themes without an intense foreground", async () => {
    const { buildTerminalThemeColors } = await importBackgroundImage();
    const { foregroundIntense: _foregroundIntense, ...legacyTerminalColors } =
      themeColors.terminal;
    const terminalColors = buildTerminalThemeColors(
      legacyTerminalColors,
      appearance({ bold_default_foreground_highlight: true }),
    );

    expect(terminalColors.foregroundIntense).toBe(themeColors.terminal.foreground);
  });

  it("keeps background-image transparency for xterm while terminal wrappers provide tint", async () => {
    const { buildSurfaceCssVariables, buildTerminalThemeColors } = await importBackgroundImage();
    const withWallpaper = appearance({
      background_image_path: "C:\\wallpapers\\terminal.png",
      background_opacity: 0.5,
    });

    const cssVars = buildSurfaceCssVariables(themeColors, withWallpaper);
    const terminalColors = buildTerminalThemeColors(themeColors.terminal, withWallpaper);

    expect(cssVars["--df-bg-terminal"]).toBe("rgba(13, 17, 23, 0.5)");
    expect(cssVars["--df-bg-terminal-solid"]).toBe(themeColors.bgTerminal);
    expect(cssVars["--df-terminal-surface-bg"]).toBe("var(--df-bg-terminal)");
    expect(terminalColors.background).toBe("rgba(0, 0, 0, 0)");
  });

  it("keeps intense foreground highlighting while making the terminal background transparent", async () => {
    const { buildTerminalThemeColors } = await importBackgroundImage();
    const terminalColors = buildTerminalThemeColors(
      { ...themeColors.terminal, foregroundIntense: "#abcdef" },
      appearance({
        bold_default_foreground_highlight: true,
        background_image_path: "C:\\wallpapers\\terminal.png",
      }),
    );

    expect(terminalColors.background).toBe("rgba(0, 0, 0, 0)");
    expect(terminalColors.foregroundIntense).toBe("#abcdef");
  });

  it("keeps terminal wrappers transparent while the UI terminal surface provides Windows tint", async () => {
    setNavigator("Mozilla/5.0 (Windows NT 10.0; Win64; x64)", "Win32");
    const { buildSurfaceCssVariables, buildTerminalThemeColors } = await importBackgroundImage();
    const transparentWindow = appearance({
      window_transparency: "transparent",
      window_transparency_tint: 0.6,
    });

    const cssVars = buildSurfaceCssVariables(themeColors, transparentWindow);
    const terminalColors = buildTerminalThemeColors(themeColors.terminal, transparentWindow);

    expect(cssVars["--df-bg-terminal"]).toBe("rgba(13, 17, 23, 0.6)");
    expect(cssVars["--df-bg-terminal-solid"]).toBe(themeColors.bgTerminal);
    expect(cssVars["--df-terminal-surface-bg"]).toBe("transparent");
    expect(terminalColors.background).toBe("rgba(0, 0, 0, 0)");
  });

  it("preserves custom terminal theme colors while Windows transparency owns the wrapper background", async () => {
    setNavigator("Mozilla/5.0 (Windows NT 10.0; Win64; x64)", "Win32");
    const { buildSurfaceCssVariables, buildTerminalThemeColors } = await importBackgroundImage();
    const customTerminalColors = {
      ...themeColors.terminal,
      background: "#123456",
      foreground: "#abcdef",
      red: "#ff0000",
    };
    const transparentWindow = appearance({
      terminal_theme: "custom-terminal",
      window_transparency: "transparent",
      window_transparency_tint: 0.6,
    });

    const cssVars = buildSurfaceCssVariables(themeColors, transparentWindow);
    const terminalColors = buildTerminalThemeColors(customTerminalColors, transparentWindow);

    expect(cssVars["--df-terminal-surface-bg"]).toBe("transparent");
    expect(terminalColors.background).toBe("rgba(0, 0, 0, 0)");
    expect(terminalColors.foreground).toBe("#abcdef");
    expect(terminalColors.red).toBe("#ff0000");
  });

  it("enables transparent window surfaces on macOS below full opacity", async () => {
    setNavigator(
      "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15",
      "MacIntel",
    );
    const { buildSurfaceCssVariables, buildTerminalThemeColors, isWindowTransparencyEnabled } =
      await importBackgroundImage();
    const transparentWindow = appearance({
      window_transparency: "transparent",
      window_transparency_tint: 0.6,
    });

    const cssVars = buildSurfaceCssVariables(themeColors, transparentWindow);
    const terminalColors = buildTerminalThemeColors(themeColors.terminal, transparentWindow);

    expect(isWindowTransparencyEnabled(transparentWindow)).toBe(true);
    expect(
      isWindowTransparencyEnabled(
        appearance({ window_transparency: "none", window_transparency_tint: 1 }),
      ),
    ).toBe(false);
    expect(cssVars["--df-bg-terminal"]).toBe("rgba(13, 17, 23, 0.6)");
    expect(cssVars["--df-terminal-surface-bg"]).toBe("transparent");
    expect(terminalColors.background).toBe("rgba(0, 0, 0, 0)");
  });

  it("does not enable native window transparency on Linux", async () => {
    const { buildSurfaceCssVariables, buildTerminalThemeColors, isWindowTransparencyEnabled } =
      await importBackgroundImage();
    const transparentWindow = appearance({
      window_transparency: "transparent",
      window_transparency_tint: 0.6,
    });

    const cssVars = buildSurfaceCssVariables(themeColors, transparentWindow);
    const terminalColors = buildTerminalThemeColors(themeColors.terminal, transparentWindow);

    expect(isWindowTransparencyEnabled(transparentWindow)).toBe(false);
    expect(cssVars["--df-bg-terminal"]).toBe(themeColors.bgTerminal);
    expect(cssVars["--df-terminal-surface-bg"]).toBe(
      "var(--df-terminal-bg, var(--df-bg-terminal))",
    );
    expect(terminalColors.background).toBe(themeColors.terminal.background);
  });
});
