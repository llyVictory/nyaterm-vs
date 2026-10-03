import { listen, type UnlistenFn } from "@/lib/backend/api";
import type { Terminal } from "@xterm/xterm";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  type CredentialPromptKind,
  compilePromptRegex,
  extractCredentialPromptText,
  findMatchingCredentials,
  findPasswordOnlyFallbackCredentials,
  getCredentialPromptPattern,
  isDefaultPasswordPrompt,
  stripTerminalControlSequences,
} from "@/lib/credentialAutofill";
import { invoke } from "@/lib/invoke";
import { sendSessionInput } from "@/lib/sessionInput";
import type { SuggestionCursorPosition } from "@/lib/terminalSuggestionPosition";
import type { SavedCredential } from "@/types/global";

interface XTermCoreWithRenderDimensions {
  _core?: {
    _renderService?: {
      dimensions?: {
        css: {
          cell: {
            height: number;
            width: number;
          };
        };
      };
    };
  };
}

export interface CredentialPanelState {
  kind: CredentialPromptKind;
  matches: SavedCredential[];
  promptText: string;
}

function extractCurrentPromptLine(promptText: string) {
  return promptText.trim();
}

function credentialPatternMatches(
  credential: SavedCredential,
  kind: CredentialPromptKind,
  output: string,
) {
  const pattern = getCredentialPromptPattern(credential, kind);
  if (!pattern) return false;
  const regex = compilePromptRegex(pattern);
  return Boolean(regex?.test(output));
}

export function useCredentialAutofill(
  terminalRef: React.RefObject<Terminal | null>,
  sessionIdRef: React.RefObject<string>,
  activeRef: React.RefObject<boolean>,
  visibleRef: React.RefObject<boolean>,
  performanceModeRef: React.RefObject<string>,
  canDetectPrompt?: () => boolean,
) {
  const [panelState, setPanelState] = useState<CredentialPanelState | null>(null);
  const [selectedIndex, setSelectedIndex] = useState(-1);
  const [cursorPosition, setCursorPosition] = useState<SuggestionCursorPosition>({
    top: 0,
    left: 0,
  });

  const panelStateRef = useRef<CredentialPanelState | null>(null);
  const selectedIndexRef = useRef(-1);
  const showPanelRef = useRef(false);
  const matchesRef = useRef<SavedCredential[]>([]);

  const credentialsRef = useRef<SavedCredential[]>([]);
  const credentialsLoadedRef = useRef(false);
  const loadInFlightRef = useRef<Promise<void> | null>(null);
  const reloadAfterInFlightRef = useRef(false);
  const detectionInFlightRef = useRef(false);
  const detectionPendingRef = useRef(false);
  const outputBufferRef = useRef("");
  const recentPromptsRef = useRef<Map<string, number>>(new Map());
  const pendingCredentialRef = useRef<{
    credentialId: string;
    expiresAt: number;
  } | null>(null);
  const sendingRef = useRef(false);
  const detectPromptRef = useRef<((snapshot: string) => Promise<void>) | null>(null);
  const feedOutputRef = useRef<((payload: string) => void) | null>(null);
  const handleSelectRef = useRef<((credential: SavedCredential) => Promise<void>) | null>(null);

  const loadCredentials = useCallback(async (force = false) => {
    if (credentialsLoadedRef.current && !force) return;

    if (loadInFlightRef.current) {
      if (force) reloadAfterInFlightRef.current = true;
      await loadInFlightRef.current;
      if (force && reloadAfterInFlightRef.current) {
        await loadCredentials(true);
      }
      return;
    }

    reloadAfterInFlightRef.current = false;
    const promise = invoke<SavedCredential[]>("get_saved_credentials")
      .then((creds) => {
        credentialsRef.current = creds;
        credentialsLoadedRef.current = true;
      })
      .catch(() => {
        credentialsRef.current = [];
        credentialsLoadedRef.current = true;
      })
      .finally(() => {
        loadInFlightRef.current = null;
      });

    loadInFlightRef.current = promise;
    await promise;
  }, []);

  useEffect(() => {
    let cancelled = false;
    let unlisten: UnlistenFn | null = null;

    void loadCredentials();
    void listen<void>("credentials-changed", () => {
      if (!cancelled) void loadCredentials(true);
    }).then((cleanup) => {
      if (cancelled) cleanup();
      else unlisten = cleanup;
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [loadCredentials]);

  const getCursorViewportPosition = useCallback((): SuggestionCursorPosition => {
    try {
      const terminal = terminalRef.current;
      if (!terminal) return { top: 0, left: 0 };
      const core = (terminal as Terminal & XTermCoreWithRenderDimensions)._core;
      const dims = core?._renderService?.dimensions;
      if (!dims) return { top: 0, left: 0 };

      const cellHeight = dims.css.cell.height;
      const cellWidth = dims.css.cell.width;
      const cursorY = terminal.buffer.active.cursorY;
      const cursorX = terminal.buffer.active.cursorX;

      const screenEl = terminal.element?.querySelector(".xterm-screen");
      if (!screenEl) return { top: 0, left: 0 };
      const rect = screenEl.getBoundingClientRect();

      return {
        top: rect.top + (cursorY + 1) * cellHeight,
        left: rect.left + cursorX * cellWidth,
        lineTop: rect.top + cursorY * cellHeight,
      };
    } catch {
      return { top: 0, left: 0 };
    }
  }, [terminalRef]);

  const dismiss = useCallback(() => {
    if (!showPanelRef.current && matchesRef.current.length === 0 && selectedIndexRef.current === -1)
      return;
    showPanelRef.current = false;
    matchesRef.current = [];
    selectedIndexRef.current = -1;
    panelStateRef.current = null;
    detectionPendingRef.current = false;
    outputBufferRef.current = "";
    recentPromptsRef.current.clear();
    setPanelState(null);
    setSelectedIndex(-1);
  }, []);

  const sendCredentialValue = useCallback(
    async (credential: SavedCredential, kind: CredentialPromptKind) => {
      const session = sessionIdRef.current;
      if (kind === "username") {
        await sendSessionInput(session, `${credential.username}\r`, {
          preview: null,
          registerSubmission: null,
          origin: "credential_autofill",
          sensitivity: "secret",
        });
        return;
      }

      const password = await invoke<string | null>("get_saved_credential_password", {
        id: credential.id,
      });
      if (!password) return;
      await sendSessionInput(session, `${password}\r`, {
        preview: null,
        registerSubmission: null,
        origin: "credential_autofill",
        sensitivity: "secret",
      });
    },
    [sessionIdRef],
  );

  const showPanel = useCallback(
    (kind: CredentialPromptKind, matches: SavedCredential[], promptText: string) => {
      const state: CredentialPanelState = { kind, matches, promptText };
      panelStateRef.current = state;
      matchesRef.current = matches;
      showPanelRef.current = true;
      selectedIndexRef.current = 0;
      setPanelState(state);
      setSelectedIndex(0);
      setCursorPosition(getCursorViewportPosition());
    },
    [getCursorViewportPosition],
  );

  const handleSelectImpl = useCallback(
    async (credential: SavedCredential) => {
      const state = panelStateRef.current;
      if (!state || sendingRef.current) return;

      const wasUsername = state.kind === "username";

      sendingRef.current = true;
      try {
        await sendCredentialValue(credential, state.kind);
        if (wasUsername) {
          pendingCredentialRef.current = {
            credentialId: credential.id,
            expiresAt: Date.now() + 60_000,
          };
        } else {
          pendingCredentialRef.current = null;
        }
      } finally {
        sendingRef.current = false;
      }

      showPanelRef.current = false;
      matchesRef.current = [];
      selectedIndexRef.current = -1;
      panelStateRef.current = null;
      detectionPendingRef.current = false;
      setPanelState(null);
      setSelectedIndex(-1);
      recentPromptsRef.current.clear();

      if (wasUsername) {
        // Keep the buffer so a password prompt that arrived during selection
        // can still be detected. The pending prompt text suppresses only the
        // username prompt that was just filled.
        if (outputBufferRef.current) {
          void detectPromptRef.current?.(outputBufferRef.current);
        }
      } else {
        outputBufferRef.current = "";
        recentPromptsRef.current.clear();
      }
    },
    [sendCredentialValue],
  );

  useEffect(() => {
    handleSelectRef.current = handleSelectImpl;
  }, [handleSelectImpl]);

  const handleSelect = useCallback(async (credential: SavedCredential) => {
    await handleSelectRef.current?.(credential);
  }, []);

  const rememberPrompt = useCallback(
    (kind: CredentialPromptKind, promptText: string, now: number): boolean => {
      for (const [key, ts] of recentPromptsRef.current) {
        if (now - ts > 30_000) recentPromptsRef.current.delete(key);
      }
      const key = `${kind}:${promptText}`;
      const last = recentPromptsRef.current.get(key);
      if (last && now - last < 30_000) return false;
      recentPromptsRef.current.set(key, now);
      return true;
    },
    [],
  );

  const detectPrompt = useCallback(
    async (snapshot: string) => {
      if (detectionInFlightRef.current) {
        detectionPendingRef.current = true;
        return;
      }
      if (!activeRef.current || !visibleRef.current || panelStateRef.current) return;

      detectionInFlightRef.current = true;
      try {
        await loadCredentials();
        if (!activeRef.current || !visibleRef.current || panelStateRef.current) return;

        const credentials = credentialsRef.current;
        if (credentials.length === 0) return;

        const now = Date.now();
        const promptText = extractCredentialPromptText(snapshot);
        if (!promptText) return;

        const currentLine = extractCurrentPromptLine(promptText);
        const pending = pendingCredentialRef.current;
        if (pending && pending.expiresAt <= now) {
          pendingCredentialRef.current = null;
        }

        const activePending = pendingCredentialRef.current;
        if (activePending) {
          const pendingCred = credentials.find(
            (credential) => credential.id === activePending.credentialId,
          );

          if (
            pendingCred &&
            (credentialPatternMatches(pendingCred, "password", currentLine) ||
              credentialPatternMatches(pendingCred, "password", promptText))
          ) {
            pendingCredentialRef.current = null;
            outputBufferRef.current = "";
            recentPromptsRef.current.clear();
            await sendCredentialValue(pendingCred, "password");
            return;
          }

          if (isDefaultPasswordPrompt(currentLine)) {
            pendingCredentialRef.current = null;
          } else {
            return;
          }
        }

        const passwordMatches = findMatchingCredentials(credentials, "password", promptText);
        if (passwordMatches.length > 0) {
          if (!rememberPrompt("password", promptText, now)) return;

          showPanel("password", passwordMatches, promptText);
          return;
        }

        if (isDefaultPasswordPrompt(promptText)) {
          const fallbackMatches = findPasswordOnlyFallbackCredentials(credentials);
          if (fallbackMatches.length > 0) {
            if (!rememberPrompt("password", promptText, now)) return;

            showPanel("password", fallbackMatches, promptText);
            return;
          }
        }

        const usernameMatches = findMatchingCredentials(credentials, "username", promptText);
        if (usernameMatches.length === 0) return;
        if (!rememberPrompt("username", promptText, now)) return;
        showPanel("username", usernameMatches, promptText);
      } finally {
        detectionInFlightRef.current = false;
        if (detectionPendingRef.current) {
          detectionPendingRef.current = false;
          void detectPromptRef.current?.(outputBufferRef.current);
        }
      }
    },
    [activeRef, visibleRef, loadCredentials, rememberPrompt, sendCredentialValue, showPanel],
  );

  useEffect(() => {
    detectPromptRef.current = detectPrompt;
  }, [detectPrompt]);

  const feedOutputImpl = useCallback(
    (payload: string) => {
      if (performanceModeRef.current !== "normal") return;
      if (!activeRef.current || !visibleRef.current) return;
      if (canDetectPrompt && !canDetectPrompt()) {
        outputBufferRef.current = "";
        recentPromptsRef.current.clear();
        detectionPendingRef.current = false;
        return;
      }

      const visible = stripTerminalControlSequences(payload);
      if (!visible) return;
      outputBufferRef.current = `${outputBufferRef.current}${visible}`.slice(-4096);

      if (panelStateRef.current || sendingRef.current) return;
      void detectPrompt(outputBufferRef.current);
    },
    [activeRef, visibleRef, performanceModeRef, canDetectPrompt, detectPrompt],
  );

  useEffect(() => {
    feedOutputRef.current = feedOutputImpl;
  }, [feedOutputImpl]);

  const feedOutput = useCallback((payload: string) => {
    feedOutputRef.current?.(payload);
  }, []);

  const reset = useCallback(() => {
    showPanelRef.current = false;
    matchesRef.current = [];
    selectedIndexRef.current = -1;
    panelStateRef.current = null;
    outputBufferRef.current = "";
    pendingCredentialRef.current = null;
    detectionInFlightRef.current = false;
    detectionPendingRef.current = false;
    recentPromptsRef.current.clear();
    sendingRef.current = false;
    setPanelState(null);
    setSelectedIndex(-1);
  }, []);

  return {
    panelState,
    selectedIndex,
    setSelectedIndex,
    cursorPosition,
    showPanelRef,
    matchesRef,
    selectedIndexRef,
    feedOutput,
    handleSelect,
    dismiss,
    reset,
  };
}
