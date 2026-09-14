import type { CSSProperties } from "react";
import type { AppearanceSettings, BackgroundImageFit } from "@/types/global";
import { invoke } from "./invoke";
import { logger } from "./logger";
import { isMacOS, isWindows } from "./platform";
import type { TerminalColors, ThemeColors } from "./themes";

export const BACKGROUND_IMAGE_FITS = ["cover", "contain", "stretch", "tile"] as const;
export const DEFAULT_BACKGROUND_IMAGE_FIT: BackgroundImageFit = "cover";
export const DEFAULT_BACKGROUND_IMAGE_OPACITY = 0.45;
export const DEFAULT_BACKGROUND_CONTENT_OPACITY = 0.78;
export const DEFAULT_WINDOW_TRANSPARENCY_OPACITY = 1;

type CssVars = CSSProperties & Record<`--${string}`, string>;

export function clampOpacity(value: number | null | undefined, fallback = 1) {
  if (typeof value !== "number" || Number.isNaN(value)) return fallback;
  return Math.min(1, Math.max(0, value));
}

export function normalizeBackgroundImageFit(value: string | null | undefined): BackgroundImageFit {
  return BACKGROUND_IMAGE_FITS.includes(value as BackgroundImageFit)
    ? (value as BackgroundImageFit)
    : DEFAULT_BACKGROUND_IMAGE_FIT;
}

export function isBackgroundImageEnabled(
  appearance: Pick<AppearanceSettings, "background_image_path">,
) {
  return Boolean(appearance.background_image_path?.trim());
}

export function isTerminalTransparencyEnabled(
  appearance: Pick<
    AppearanceSettings,
    "background_image_path" | "window_transparency" | "window_transparency_tint"
  >,
) {
  return isBackgroundImageEnabled(appearance) || isWindowTransparencyEnabled(appearance);
}

export function shouldSuspendTerminalWebglForBackground(appearance: AppearanceSettings) {
  return isTerminalTransparencyEnabled(appearance);
}

export function getWindowTransparencyOpacity(
  appearance: Pick<AppearanceSettings, "window_transparency" | "window_transparency_tint">,
) {
  return clampOpacity(appearance.window_transparency_tint, DEFAULT_WINDOW_TRANSPARENCY_OPACITY);
}

export function windowTransparencyModeForOpacity(opacity: number): "none" | "transparent" {
  return clampOpacity(opacity, DEFAULT_WINDOW_TRANSPARENCY_OPACITY) >= 1
    ? "none"
    : "transparent";
}

/** Native window transparency makes the window show through to the desktop.
 * Requires translucent webview surface colors. */
export function isWindowTransparencyEnabled(
  appearance: Pick<AppearanceSettings, "window_transparency" | "window_transparency_tint">,
) {
  return (isWindows || isMacOS) && getWindowTransparencyOpacity(appearance) < 1;
}

function quoteCssUrl(url: string) {
  return `url("${url.replace(/["\\]/g, "\\$&")}")`;
}

/**
 * Load a background image file via a Rust command and return a data URL.
 * Returns empty string if the path is empty or the file cannot be read.
 */
export async function loadBackgroundImageDataUrl(path: string | null | undefined): Promise<string> {
  const trimmed = path?.trim();
  if (!trimmed) return "";
  try {
    return await invoke<string>("read_background_image_data_url", { path: trimmed });
  } catch (error) {
    logger.warn({
      domain: "background-image",
      event: "load_data_url_failed",
      message: `Failed to load background image: ${trimmed}`,
      error,
    });
    return "";
  }
}

export function getBackgroundFitStyle(fit: string | null | undefined): CSSProperties {
  switch (normalizeBackgroundImageFit(fit)) {
    case "contain":
      return {
        backgroundPosition: "center",
        backgroundRepeat: "no-repeat",
        backgroundSize: "contain",
      };
    case "stretch":
      return {
        backgroundPosition: "center",
        backgroundRepeat: "no-repeat",
        backgroundSize: "100% 100%",
      };
    case "tile":
      return {
        backgroundPosition: "top left",
        backgroundRepeat: "repeat",
        backgroundSize: "auto",
      };
    default:
      return {
        backgroundPosition: "center",
        backgroundRepeat: "no-repeat",
        backgroundSize: "cover",
      };
  }
}

export function buildBackgroundImageLayerStyle(
  appearance: AppearanceSettings,
  dataUrl: string,
): CSSProperties {
  if (!dataUrl) {
    return { display: "none" };
  }

  return {
    backgroundImage: quoteCssUrl(dataUrl),
    opacity: clampOpacity(appearance.background_image_opacity, DEFAULT_BACKGROUND_IMAGE_OPACITY),
    ...getBackgroundFitStyle(appearance.background_image_fit),
  };
}

function colorWithAlpha(color: string, opacity: number) {
  if (opacity >= 1) return color;

  const hex = color.trim();
  const shortHex = /^#([\da-f])([\da-f])([\da-f])$/i.exec(hex);
  if (shortHex) {
    const [, r, g, b] = shortHex;
    return `rgba(${Number.parseInt(`${r}${r}`, 16)}, ${Number.parseInt(`${g}${g}`, 16)}, ${Number.parseInt(`${b}${b}`, 16)}, ${opacity})`;
  }

  const longHex = /^#([\da-f]{2})([\da-f]{2})([\da-f]{2})$/i.exec(hex);
  if (longHex) {
    const [, r, g, b] = longHex;
    return `rgba(${Number.parseInt(r, 16)}, ${Number.parseInt(g, 16)}, ${Number.parseInt(b, 16)}, ${opacity})`;
  }

  return `color-mix(in srgb, ${color} ${Math.round(opacity * 100)}%, transparent)`;
}

export function buildSurfaceCssVariables(
  colors: ThemeColors,
  appearance: AppearanceSettings,
): CssVars {
  // Native window transparency uses translucent surfaces so apps behind the
  // window show through while the terminal remains readable.
  if (isWindowTransparencyEnabled(appearance)) {
    const surfaceOpacity = getWindowTransparencyOpacity(appearance);
    const bg = colorWithAlpha(colors.bg, surfaceOpacity);
    const bgPanel = colorWithAlpha(colors.bgPanel, surfaceOpacity);
    const bgTerminal = colorWithAlpha(colors.bgTerminal, surfaceOpacity);
    const bgHover = colorWithAlpha(colors.bgHover, surfaceOpacity);
    const bgInput = colorWithAlpha(colors.bgInput, surfaceOpacity);
    const bgSectionHeader = colorWithAlpha(colors.bgSectionHeader, surfaceOpacity);
    return {
      "--df-bg": bg,
      "--df-bg-panel": bgPanel,
      "--df-bg-panel-solid": colors.bgPanel,
      "--df-bg-terminal": bgTerminal,
      "--df-bg-terminal-solid": colors.bgTerminal,
      "--df-terminal-surface-bg": "transparent",
      "--df-bg-hover": bgHover,
      "--df-bg-input": bgInput,
      "--df-bg-section-header": bgSectionHeader,
      "--background": bg,
      "--card": bgPanel,
      "--popover": bgPanel,
      "--secondary": bgHover,
      "--muted": bgHover,
      "--accent": bgHover,
      "--input": colors.border,
    };
  }
  const surfaceOpacity = isBackgroundImageEnabled(appearance)
    ? clampOpacity(appearance.background_opacity)
    : 1;
  const bg = colorWithAlpha(colors.bg, surfaceOpacity);
  const bgPanel = colorWithAlpha(colors.bgPanel, surfaceOpacity);
  const bgTerminal = colorWithAlpha(colors.bgTerminal, surfaceOpacity);
  const bgHover = colorWithAlpha(colors.bgHover, surfaceOpacity);
  const bgInput = colorWithAlpha(colors.bgInput, surfaceOpacity);
  const bgSectionHeader = colorWithAlpha(colors.bgSectionHeader, surfaceOpacity);
  const terminalSurfaceBg = isBackgroundImageEnabled(appearance)
    ? "var(--df-bg-terminal)"
    : "var(--df-terminal-bg, var(--df-bg-terminal))";

  return {
    "--df-bg": bg,
    "--df-bg-panel": bgPanel,
    "--df-bg-panel-solid": colors.bgPanel,
    "--df-bg-terminal": bgTerminal,
    "--df-bg-terminal-solid": colors.bgTerminal,
    "--df-terminal-surface-bg": terminalSurfaceBg,
    "--df-bg-hover": bgHover,
    "--df-bg-input": bgInput,
    "--df-bg-section-header": bgSectionHeader,
    "--background": bg,
    "--card": bgPanel,
    "--popover": bgPanel,
    "--secondary": bgHover,
    "--muted": bgHover,
    "--accent": bgHover,
    "--input": colors.border,
  };
}

export function buildTerminalThemeColors(
  terminalColors: TerminalColors,
  appearance: AppearanceSettings,
): TerminalColors {
  const { foregroundIntense, ...baseColors } = terminalColors;
  const resolvedColors: TerminalColors = appearance.bold_default_foreground_highlight
    ? {
        ...baseColors,
        foregroundIntense: foregroundIntense ?? terminalColors.foreground,
      }
    : baseColors;

  if (!isTerminalTransparencyEnabled(appearance)) {
    return resolvedColors;
  }

  return {
    ...resolvedColors,
    background: "rgba(0, 0, 0, 0)",
  };
}
