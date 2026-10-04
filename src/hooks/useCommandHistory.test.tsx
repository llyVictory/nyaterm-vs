import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { createTerminalInputState } from "@/lib/terminalInputTracker";
import { useCommandHistory } from "./useCommandHistory";

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@/lib/invoke", () => ({ invoke: mocks.invoke }));
vi.mock("@/lib/backend/api", () => ({ listen: async () => () => {} }));

beforeEach(() => {
  mocks.invoke.mockReset();
  mocks.invoke.mockImplementation(async (command: string) =>
    command === "fuzzy_search_commands"
      ? [
          {
            command: "ls -la",
            display: "List files",
            indices: [],
            score: 10,
            source: "quickCommand",
          },
        ]
      : [],
  );
});

it("keeps command suggestion searches available on desktop", async () => {
  const inputStateRef = {
    current: { ...createTerminalInputState(), value: "ls", cursor: 2 },
  };
  const { result } = renderHook(() =>
    useCommandHistory({ current: null }, inputStateRef, vi.fn(), () => true, true, 1, 100),
  );
  act(() => result.current.triggerSearch({ manual: true }));
  await waitFor(() => expect(result.current.showSuggestions).toBe(true));
  expect(mocks.invoke).toHaveBeenCalledWith("fuzzy_search_history", {
    pattern: "ls",
    limit: 8,
    minCommandLength: 1,
    maxCommandLength: 100,
  });
  expect(mocks.invoke).toHaveBeenCalledWith("fuzzy_search_commands", {
    pattern: "ls",
    limit: 8,
  });
  expect(result.current.suggestions[0].command).toBe("ls -la");
});
