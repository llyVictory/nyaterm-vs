import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { describe, expect, it } from "vitest";
import { codeMirrorFileViewExtensions } from "./codeMirrorFileView";

function findSelectionRules(): string[] {
  const rules: string[] = [];
  for (const sheet of Array.from(document.styleSheets)) {
    const owner = sheet.ownerNode as HTMLStyleElement | null;
    // jsdom's cssText drops !important inside var() values, so read the raw
    // rule text from the mounted style tag instead.
    const text = owner?.textContent ?? "";
    for (const line of text.split("\n")) {
      if (line.includes("cm-selectionBackground")) {
        rules.push(line);
      }
    }
  }
  return rules;
}

describe("codeMirrorFileView selection styling", () => {
  it("renders the editor selection with the terminal selection color", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);

    const view = new EditorView({
      state: EditorState.create({
        doc: "hello",
        extensions: codeMirrorFileViewExtensions("plaintext"),
      }),
      parent: host,
    });

    try {
      const rules = findSelectionRules().join("\n");
      expect(rules).toContain("var(--df-terminal-selection");
      expect(rules).toContain("!important");
    } finally {
      view.destroy();
      host.remove();
    }
  });
});
