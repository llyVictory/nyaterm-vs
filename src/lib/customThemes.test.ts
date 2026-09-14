import { describe, expect, it } from "vitest";
import type { AppearanceSettings } from "@/types/global";
import { DEFAULT_THEME_ID, themes, type Theme } from "./themes";
import {
  appendCustomThemePatch,
  getThemeColor,
  normalizeImportedTheme,
  removeCustomThemePatch,
  setThemeColor,
  TERMINAL_THEME_COLOR_FIELDS,
  upsertCustomThemePatch,
  validateTheme,
} from "./customThemes";

function makeTheme(id: string, name = id): Theme {
  return { ...structuredClone(themes[DEFAULT_THEME_ID]), id, name, label: name };
}

function makeAppearance(overrides: Partial<AppearanceSettings> = {}): AppearanceSettings {
  return {
    theme: DEFAULT_THEME_ID,
    terminal_theme: null,
    custom_themes: [],
    ...overrides,
  } as AppearanceSettings;
}

describe("upsertCustomThemePatch", () => {
  it("appends the theme when the list does not contain it", () => {
    const existing = makeTheme("custom-1");
    const incoming = makeTheme("custom-2");
    const patch = upsertCustomThemePatch(makeAppearance({ custom_themes: [existing] }), incoming);
    expect(patch.custom_themes).toEqual([existing, incoming]);
  });

  it("replaces the theme with the same id", () => {
    const existing = makeTheme("custom-1");
    const updated = { ...existing, name: "renamed" };
    const patch = upsertCustomThemePatch(makeAppearance({ custom_themes: [existing] }), updated);
    expect(patch.custom_themes).toEqual([updated]);
  });

  it("treats a missing custom_themes list as empty", () => {
    const incoming = makeTheme("custom-1");
    const patch = upsertCustomThemePatch(
      makeAppearance({ custom_themes: undefined }),
      incoming,
    );
    expect(patch.custom_themes).toEqual([incoming]);
  });
});

describe("appendCustomThemePatch", () => {
  it("appends the theme to the existing list", () => {
    const existing = makeTheme("custom-1");
    const incoming = makeTheme("custom-2");
    const patch = appendCustomThemePatch(makeAppearance({ custom_themes: [existing] }), incoming);
    expect(patch.custom_themes).toEqual([existing, incoming]);
  });

  it("treats a missing custom_themes list as empty", () => {
    const incoming = makeTheme("custom-1");
    const patch = appendCustomThemePatch(makeAppearance({ custom_themes: undefined }), incoming);
    expect(patch.custom_themes).toEqual([incoming]);
  });
});

describe("removeCustomThemePatch", () => {
  it("removes the theme and keeps unrelated active themes", () => {
    const first = makeTheme("custom-1");
    const second = makeTheme("custom-2");
    const patch = removeCustomThemePatch(
      makeAppearance({ theme: DEFAULT_THEME_ID, custom_themes: [first, second] }),
      "custom-1",
      DEFAULT_THEME_ID,
    );
    expect(patch.custom_themes).toEqual([second]);
    expect(patch.theme).toBeUndefined();
    expect(patch.terminal_theme).toBeUndefined();
  });

  it("resets the active UI theme when the removed theme is active", () => {
    const first = makeTheme("custom-1");
    const patch = removeCustomThemePatch(
      makeAppearance({ theme: "custom-1", custom_themes: [first] }),
      "custom-1",
      DEFAULT_THEME_ID,
    );
    expect(patch.theme).toBe(DEFAULT_THEME_ID);
  });

  it("resets the active terminal theme when the removed theme is active", () => {
    const first = makeTheme("custom-1");
    const patch = removeCustomThemePatch(
      makeAppearance({ terminal_theme: "custom-1", custom_themes: [first] }),
      "custom-1",
      DEFAULT_THEME_ID,
    );
    expect(patch.terminal_theme).toBeNull();
  });
});

describe("foregroundIntense custom theme compatibility", () => {
  it("falls back to foreground and still validates legacy themes", () => {
    const legacy = makeTheme("legacy-theme");
    delete legacy.colors.terminal.foregroundIntense;

    expect(getThemeColor(legacy, "terminal.foregroundIntense")).toBe(
      legacy.colors.terminal.foreground,
    );
    expect(validateTheme(legacy)).toEqual([]);
  });

  it("edits and preserves foregroundIntense through import normalization", () => {
    const updated = setThemeColor(
      makeTheme("custom-intense"),
      "terminal.foregroundIntense",
      "#abcdef",
    );
    const normalized = normalizeImportedTheme(updated, new Set());

    expect(
      TERMINAL_THEME_COLOR_FIELDS.some((field) => field.path === "terminal.foregroundIntense"),
    ).toBe(true);
    expect(getThemeColor(normalized, "terminal.foregroundIntense")).toBe("#abcdef");
    expect(validateTheme(normalized)).toEqual([]);
  });
});
