import type { Terminal } from "@xterm/xterm";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const platform = vi.hoisted(() => ({
  isLinux: false,
  isMacOS: true,
}));

vi.mock("@/lib/platform", () => ({
  get isLinux() {
    return platform.isLinux;
  },
  get isMacOS() {
    return platform.isMacOS;
  },
}));

vi.mock("@/lib/logger", () => ({
  logger: {
    debug: vi.fn(),
    warn: vi.fn(),
  },
}));

import { installImeCompatibilityPatch } from "./xtermImeCompatibility";

interface FakeCompositionHelper {
  _isComposing: boolean;
  _isSendingComposition: boolean;
  _textareaChangeTimer?: number;
  compositionstart: () => void;
}

interface FakeCore {
  _inputEvent: (event: InputEvent) => unknown;
  _keyDownSeen: boolean;
  _compositionHelper: FakeCompositionHelper;
  textarea: HTMLTextAreaElement;
}

function createHarness() {
  const textarea = document.createElement("textarea");
  const observedKeyDownSeen: boolean[] = [];
  const originalInputEvent = vi.fn(function (this: FakeCore) {
    observedKeyDownSeen.push(this._keyDownSeen);
    return true;
  });
  const core: FakeCore = {
    _inputEvent: originalInputEvent,
    _keyDownSeen: false,
    _compositionHelper: {
      _isComposing: false,
      _isSendingComposition: false,
      compositionstart: vi.fn(),
    },
    textarea,
  };
  const terminal = { _core: core } as unknown as Terminal;

  return { core, observedKeyDownSeen, originalInputEvent, terminal, textarea };
}

function dispatchKeydown(
  textarea: HTMLTextAreaElement,
  key: string,
  keyCode: number,
) {
  const event = new KeyboardEvent("keydown", { bubbles: true, key });
  Object.defineProperty(event, "keyCode", { value: keyCode });
  textarea.dispatchEvent(event);
}

function inputEvent(data: string): InputEvent {
  return {
    data,
    inputType: "insertText",
    isComposing: false,
  } as InputEvent;
}

describe("xterm IME compatibility", () => {
  beforeEach(() => {
    platform.isLinux = false;
    platform.isMacOS = true;
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("forces the first direct-commit character after a held modifier", () => {
    const { core, observedKeyDownSeen, terminal, textarea } = createHarness();
    const patch = installImeCompatibilityPatch(terminal, true);

    dispatchKeydown(textarea, "Shift", 16);
    core._keyDownSeen = true;
    core._inputEvent(inputEvent("$"));

    expect(observedKeyDownSeen).toEqual([false]);
    expect(core._keyDownSeen).toBe(true);
    patch.dispose();
  });

  it("keeps the existing keyCode 229 compatibility path", () => {
    const { core, observedKeyDownSeen, terminal, textarea } = createHarness();
    const patch = installImeCompatibilityPatch(terminal, true);

    dispatchKeydown(textarea, "a", 229);
    core._keyDownSeen = true;
    core._inputEvent(inputEvent("a"));

    expect(observedKeyDownSeen).toEqual([false]);
    patch.dispose();
  });

  it("does not force normal Shift plus physical-key input", () => {
    const { core, observedKeyDownSeen, terminal, textarea } = createHarness();
    const patch = installImeCompatibilityPatch(terminal, true);

    dispatchKeydown(textarea, "Shift", 16);
    dispatchKeydown(textarea, "$", 52);
    core._keyDownSeen = true;
    core._inputEvent(inputEvent("$"));

    expect(observedKeyDownSeen).toEqual([true]);
    patch.dispose();
  });

  it("does not force direct commits while composition is active", () => {
    const { core, observedKeyDownSeen, terminal, textarea } = createHarness();
    const patch = installImeCompatibilityPatch(terminal, true);

    core._compositionHelper._isComposing = true;
    dispatchKeydown(textarea, "Shift", 16);
    core._keyDownSeen = true;
    core._inputEvent(inputEvent("$"));

    expect(observedKeyDownSeen).toEqual([true]);
    patch.dispose();
  });

  it("clears stale Linux textarea state after compositionend", () => {
    vi.useFakeTimers();
    platform.isLinux = true;
    platform.isMacOS = false;
    const { terminal, textarea } = createHarness();
    const patch = installImeCompatibilityPatch(terminal, true);

    textarea.value = "一";
    textarea.dispatchEvent(
      new CompositionEvent("compositionend", { bubbles: true }),
    );
    expect(textarea.value).toBe("一");

    vi.runAllTimers();
    expect(textarea.value).toBe("");

    textarea.value = "二";
    textarea.dispatchEvent(
      new CompositionEvent("compositionend", { bubbles: true }),
    );
    vi.runAllTimers();
    expect(textarea.value).toBe("");

    patch.dispose();
  });

  it("does not let stale cleanup erase a new Linux composition", () => {
    vi.useFakeTimers();
    platform.isLinux = true;
    platform.isMacOS = false;
    const { core, terminal, textarea } = createHarness();
    const patch = installImeCompatibilityPatch(terminal, true);

    textarea.value = "一";
    textarea.dispatchEvent(
      new CompositionEvent("compositionend", { bubbles: true }),
    );

    core._compositionHelper.compositionstart();
    expect(textarea.value).toBe("");

    textarea.value = "二";
    vi.runAllTimers();

    expect(textarea.value).toBe("二");
    patch.dispose();
  });

  it("cancels pending Linux textarea cleanup when disposed", () => {
    vi.useFakeTimers();
    platform.isLinux = true;
    platform.isMacOS = false;
    const { terminal, textarea } = createHarness();
    const patch = installImeCompatibilityPatch(terminal, true);

    textarea.value = "一";
    textarea.dispatchEvent(
      new CompositionEvent("compositionend", { bubbles: true }),
    );
    patch.dispose();
    vi.runAllTimers();

    expect(textarea.value).toBe("一");
  });
});
