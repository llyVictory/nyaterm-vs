import { describe, expect, it } from "vitest";
import { themeList } from "./themes";

describe("built-in terminal foregroundIntense colors", () => {
  it("defines a distinct valid foregroundIntense for every built-in theme", () => {
    for (const theme of themeList) {
      const { foreground, foregroundIntense } = theme.colors.terminal;

      expect(foregroundIntense, theme.id).toMatch(/^#[0-9a-f]{6}$/i);
      expect(foregroundIntense, theme.id).not.toBe(foreground);
    }
  });
});
