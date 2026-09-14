import { describe, expect, it } from "vitest";
import {
  createDefaultTerminalSearchState,
  shouldBlockTerminalSearchNavigation,
  TERMINAL_SEARCH_VISIBLE_MATCH_LIMIT,
  type TerminalSearchState,
} from "./terminalSearch";

describe("shouldBlockTerminalSearchNavigation", () => {
  it("blocks navigation past known result boundaries when wrapping is disabled", () => {
    expect(shouldBlockTerminalSearchNavigation(createFoundState(4, 5), "next", false)).toBe(true);
    expect(shouldBlockTerminalSearchNavigation(createFoundState(0, 5), "previous", false)).toBe(
      true,
    );
  });

  it("allows navigation within known result boundaries", () => {
    expect(shouldBlockTerminalSearchNavigation(createFoundState(3, 5), "next", false)).toBe(false);
    expect(shouldBlockTerminalSearchNavigation(createFoundState(1, 5), "previous", false)).toBe(
      false,
    );
  });

  it("keeps xterm navigation for wrapped and 1000+ result sets", () => {
    expect(shouldBlockTerminalSearchNavigation(createFoundState(4, 5), "next", true)).toBe(false);
    expect(
      shouldBlockTerminalSearchNavigation(
        createFoundState(
          TERMINAL_SEARCH_VISIBLE_MATCH_LIMIT - 1,
          TERMINAL_SEARCH_VISIBLE_MATCH_LIMIT,
        ),
        "next",
        false,
      ),
    ).toBe(false);
  });

  it("does not block incomplete or non-found search state", () => {
    const state = createDefaultTerminalSearchState();
    state.query = "match";
    state.status = "searching";

    expect(shouldBlockTerminalSearchNavigation(state, "next", false)).toBe(false);
  });
});

function createFoundState(activeIndex: number, resultCount: number): TerminalSearchState {
  return {
    ...createDefaultTerminalSearchState(),
    query: "match",
    status: "found",
    activeIndex,
    resultCount,
  };
}
