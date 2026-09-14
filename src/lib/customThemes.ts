import type { AppearanceSettings } from "@/types/global";
import { isBuiltinThemeId, type TerminalColors, type Theme, type ThemeColors } from "./themes";

export type ThemeColorPath =
  | keyof Omit<ThemeColors, "terminal">
  | `terminal.${keyof TerminalColors}`;

export interface ThemeColorField {
  path: ThemeColorPath;
  labelKey: string;
  cssColor?: boolean;
}

export const UI_THEME_COLOR_FIELDS: readonly ThemeColorField[] = [
  { path: "bg", labelKey: "settings.themeColorBg" },
  { path: "bgPanel", labelKey: "settings.themeColorBgPanel" },
  { path: "bgTerminal", labelKey: "settings.themeColorBgTerminal" },
  { path: "bgHover", labelKey: "settings.themeColorBgHover" },
  { path: "bgInput", labelKey: "settings.themeColorBgInput" },
  { path: "bgSectionHeader", labelKey: "settings.themeColorBgSectionHeader" },
  { path: "border", labelKey: "settings.themeColorBorder" },
  { path: "text", labelKey: "settings.themeColorText" },
  { path: "textMuted", labelKey: "settings.themeColorTextMuted" },
  { path: "textDimmed", labelKey: "settings.themeColorTextDimmed" },
  { path: "primary", labelKey: "settings.themeColorPrimary" },
  { path: "primaryHover", labelKey: "settings.themeColorPrimaryHover" },
  { path: "onPrimary", labelKey: "settings.themeColorOnPrimary" },
  { path: "focusRing", labelKey: "settings.themeColorFocusRing" },
  { path: "danger", labelKey: "settings.themeColorDanger" },
  { path: "dangerHover", labelKey: "settings.themeColorDangerHover" },
  { path: "success", labelKey: "settings.themeColorSuccess" },
  { path: "warning", labelKey: "settings.themeColorWarning" },
  { path: "link", labelKey: "settings.themeColorLink" },
  { path: "shadow", labelKey: "settings.themeColorShadow", cssColor: true },
  { path: "scrollThumb", labelKey: "settings.themeColorScrollThumb" },
  { path: "accent", labelKey: "settings.themeColorAccent" },
];

export const TERMINAL_THEME_COLOR_FIELDS: readonly ThemeColorField[] = [
  { path: "terminal.background", labelKey: "settings.themeColorTerminalBackground" },
  { path: "terminal.foreground", labelKey: "settings.themeColorTerminalForeground" },
  {
    path: "terminal.foregroundIntense",
    labelKey: "settings.themeColorTerminalForegroundIntense",
  },
  { path: "terminal.cursor", labelKey: "settings.themeColorTerminalCursor" },
  { path: "terminal.selectionBackground", labelKey: "settings.themeColorTerminalSelection" },
  { path: "terminal.lineHighlight", labelKey: "settings.themeColorTerminalLineHighlight" },
  {
    path: "terminal.findMatchBackground",
    labelKey: "settings.themeColorTerminalFindMatchBackground",
    cssColor: true,
  },
  { path: "terminal.findMatchBorder", labelKey: "settings.themeColorTerminalFindMatchBorder" },
  { path: "terminal.black", labelKey: "settings.themeColorAnsiBlack" },
  { path: "terminal.red", labelKey: "settings.themeColorAnsiRed" },
  { path: "terminal.green", labelKey: "settings.themeColorAnsiGreen" },
  { path: "terminal.yellow", labelKey: "settings.themeColorAnsiYellow" },
  { path: "terminal.blue", labelKey: "settings.themeColorAnsiBlue" },
  { path: "terminal.magenta", labelKey: "settings.themeColorAnsiMagenta" },
  { path: "terminal.cyan", labelKey: "settings.themeColorAnsiCyan" },
  { path: "terminal.white", labelKey: "settings.themeColorAnsiWhite" },
  { path: "terminal.brightBlack", labelKey: "settings.themeColorAnsiBrightBlack" },
  { path: "terminal.brightRed", labelKey: "settings.themeColorAnsiBrightRed" },
  { path: "terminal.brightGreen", labelKey: "settings.themeColorAnsiBrightGreen" },
  { path: "terminal.brightYellow", labelKey: "settings.themeColorAnsiBrightYellow" },
  { path: "terminal.brightBlue", labelKey: "settings.themeColorAnsiBrightBlue" },
  { path: "terminal.brightMagenta", labelKey: "settings.themeColorAnsiBrightMagenta" },
  { path: "terminal.brightCyan", labelKey: "settings.themeColorAnsiBrightCyan" },
  { path: "terminal.brightWhite", labelKey: "settings.themeColorAnsiBrightWhite" },
];

export const ALL_THEME_COLOR_FIELDS = [
  ...UI_THEME_COLOR_FIELDS,
  ...TERMINAL_THEME_COLOR_FIELDS,
] as const;

export function generateCustomThemeId() {
  const random = Math.random().toString(36).slice(2, 8);
  return `custom-${Date.now()}-${random}`;
}

export function cloneThemeAsCustom(source: Theme, name?: string): Theme {
  return {
    ...structuredCloneTheme(source),
    id: generateCustomThemeId(),
    name: name?.trim() || `${source.name} Custom`,
    label: source.label.slice(0, 14),
  };
}

export function structuredCloneTheme(theme: Theme): Theme {
  return JSON.parse(JSON.stringify(theme)) as Theme;
}

/** Appearance patch that inserts or updates a custom theme in the list. */
export function upsertCustomThemePatch(
  prev: AppearanceSettings,
  theme: Theme,
): Partial<AppearanceSettings> {
  const prevThemes = prev.custom_themes ?? [];
  const nextThemes = prevThemes.some((item) => item.id === theme.id)
    ? prevThemes.map((item) => (item.id === theme.id ? theme : item))
    : [...prevThemes, theme];
  return { custom_themes: nextThemes };
}

/** Appearance patch that appends a custom theme to the list. */
export function appendCustomThemePatch(
  prev: AppearanceSettings,
  theme: Theme,
): Partial<AppearanceSettings> {
  return { custom_themes: [...(prev.custom_themes ?? []), theme] };
}

/**
 * Appearance patch that removes a custom theme, resetting the active UI/terminal
 * theme when it pointed at the removed theme.
 */
export function removeCustomThemePatch(
  prev: AppearanceSettings,
  themeId: string,
  defaultThemeId: string,
): Partial<AppearanceSettings> {
  const nextThemes = (prev.custom_themes ?? []).filter((item) => item.id !== themeId);
  const patch: Partial<AppearanceSettings> = { custom_themes: nextThemes };
  if (prev.theme === themeId) patch.theme = defaultThemeId;
  if (prev.terminal_theme === themeId) patch.terminal_theme = null;
  return patch;
}

export function isHexColor(value: string) {
  return /^#[0-9a-f]{6}$/i.test(value.trim());
}

export function isCssColor(value: string) {
  const trimmed = value.trim();
  if (!trimmed) return false;
  if (typeof CSS !== "undefined" && CSS.supports) {
    return CSS.supports("color", trimmed);
  }
  return isHexColor(trimmed) || /^(?:rgb|rgba|hsl|hsla)\(/i.test(trimmed);
}

export function getThemeColor(theme: Theme, path: ThemeColorPath): string {
  if (path.startsWith("terminal.")) {
    const key = path.slice("terminal.".length) as keyof TerminalColors;
    if (key === "foregroundIntense") {
      return theme.colors.terminal.foregroundIntense ?? theme.colors.terminal.foreground;
    }
    return theme.colors.terminal[key] as string;
  }
  return theme.colors[path as keyof Omit<ThemeColors, "terminal">] as string;
}

export function setThemeColor(theme: Theme, path: ThemeColorPath, value: string): Theme {
  const next = structuredCloneTheme(theme);
  if (path.startsWith("terminal.")) {
    const key = path.slice("terminal.".length) as keyof TerminalColors;
    next.colors.terminal[key] = value;
    return next;
  }
  (next.colors[path as keyof Omit<ThemeColors, "terminal">] as string) = value;
  return next;
}

export function normalizeImportedTheme(theme: Theme, existingIds: Set<string>): Theme {
  const next = structuredCloneTheme(theme);
  if (!next.id || isBuiltinThemeId(next.id) || existingIds.has(next.id)) {
    next.id = generateCustomThemeId();
  }
  next.name = next.name?.trim() || "Imported Theme";
  next.label = (next.label?.trim() || next.name).slice(0, 14);
  return next;
}

export function validateTheme(theme: Theme): string[] {
  const errors: string[] = [];
  if (!theme.id?.trim()) errors.push("id");
  if (!theme.name?.trim()) errors.push("name");
  if (!theme.label?.trim()) errors.push("label");
  if (!isHexColor(theme.swatch ?? "")) errors.push("swatch");

  for (const field of ALL_THEME_COLOR_FIELDS) {
    const value = getThemeColor(theme, field.path);
    const valid = field.cssColor ? isCssColor(value) : isHexColor(value);
    if (!valid) errors.push(field.path);
  }

  return errors;
}
