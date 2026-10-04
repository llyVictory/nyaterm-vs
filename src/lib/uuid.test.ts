import { afterEach, describe, expect, it, vi } from "vitest";
import { randomUUID } from "./uuid";
import { restoreTabFromPersistence } from "./workspaceTabs";

afterEach(() => vi.unstubAllGlobals());

describe("request UUIDs", () => {
  it("uses the native API when available", () => {
    const native = vi.fn(() => "native-uuid");
    vi.stubGlobal("crypto", { randomUUID: native });
    expect(randomUUID()).toBe("native-uuid");
    expect(native).toHaveBeenCalledOnce();
  });

  it("creates UUID v4 request IDs and restores SSH tabs without the secure-context API", () => {
    const getRandomValues = globalThis.crypto.getRandomValues.bind(
      globalThis.crypto,
    );
    vi.stubGlobal("crypto", { getRandomValues });
    const ids = Array.from({ length: 100 }, () => randomUUID());
    expect(new Set(ids).size).toBe(ids.length);
    for (const id of ids) {
      expect(id).toMatch(
        /^[\da-f]{8}-[\da-f]{4}-4[\da-f]{3}-[89ab][\da-f]{3}-[\da-f]{12}$/,
      );
    }
    // Startup restoration used to throw before the terminal UI could mount.
    const tab = restoreTabFromPersistence(
      {
        title: "SSH",
        session_type: "SSH",
        root: {
          kind: "leaf",
          id: "saved-pane",
          session_type: "SSH",
          title: "SSH",
          connection_id: "saved-ssh",
        },
      },
      0,
    );
    expect(tab?.root.kind).toBe("leaf");
    if (tab?.root.kind === "leaf") {
      expect(tab.root.createRequestId).toMatch(/^[\da-f-]{36}$/);
    }
  });
});
