import { describe, expect, it } from "vitest";
import { canStartDockerContainer, canStopDockerContainer } from "./dockerContainerActions";

describe("docker container action availability", () => {
  it.each(["created", "exited", " EXITED "])("allows starting %s containers", (state) => {
    expect(canStartDockerContainer(state)).toBe(true);
  });

  it.each(["running", "restarting", "paused", "removing", "dead", "unknown"])(
    "does not allow starting %s containers",
    (state) => {
      expect(canStartDockerContainer(state)).toBe(false);
    },
  );

  it.each(["running", "restarting", " RESTARTING "])("allows stopping %s containers", (state) => {
    expect(canStopDockerContainer(state)).toBe(true);
  });

  it.each(["created", "exited", "paused", "removing", "dead", "unknown"])(
    "does not allow stopping %s containers",
    (state) => {
      expect(canStopDockerContainer(state)).toBe(false);
    },
  );
});
