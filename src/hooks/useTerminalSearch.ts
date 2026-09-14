import type { Terminal } from "@xterm/xterm";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@/lib/invoke";
import {
  createDefaultTerminalHistorySearchState,
  createDefaultTerminalSearchState,
  DEFAULT_TERMINAL_SEARCH_DECORATIONS,
  type Disposable,
  shouldBlockTerminalSearchNavigation,
  TERMINAL_HISTORY_DEFAULT_LINES,
  TERMINAL_HISTORY_RESULT_LIMIT,
  TERMINAL_SEARCH_DEBOUNCE_MS,
  TERMINAL_SEARCH_MIN_QUERY_LENGTH,
  type TerminalHistorySearchResponse,
  type TerminalHistorySearchState,
  type TerminalSearchAddon,
  type TerminalSearchDecorations,
  type TerminalSearchDirection,
  type TerminalSearchFlags,
  type TerminalSearchMode,
  type TerminalSearchPerformanceMode,
  type TerminalSearchResultChangeEvent,
  type TerminalSearchState,
} from "@/lib/terminalSearch";

export interface UseTerminalSearchOptions {
  terminal?: Terminal | null;
  sessionId?: string | null;
  visible?: boolean;
  performanceMode?: TerminalSearchPerformanceMode;
  debounceMs?: number;
  minIncrementalQueryLength?: number;
  decorations?: TerminalSearchDecorations;
  focusTerminalAfterNavigation?: boolean;
}

export interface UseTerminalSearchResult {
  registerSearchAddon: (addon: TerminalSearchAddon | null) => void;
  showSearchBar: boolean;
  setShowSearchBar: (show: boolean) => void;
  searchQuery: string;
  setSearchQuery: (query: string) => void;
  searchState: TerminalSearchState;
  searchFlags: TerminalSearchFlags;
  setSearchFlag: (flag: keyof TerminalSearchFlags, value: boolean) => void;
  wrapAround: boolean;
  setWrapAround: (wrapAround: boolean) => void;
  activeMode: TerminalSearchMode;
  setActiveMode: (mode: TerminalSearchMode) => void;
  historyState: TerminalHistorySearchState;
  handleSearchNext: (query?: string) => void;
  handleSearchPrev: (query?: string) => void;
  handleCloseSearch: () => void;
}

export function useTerminalSearch(
  terminalRef: React.RefObject<Terminal | null>,
  options: UseTerminalSearchOptions = {},
): UseTerminalSearchResult {
  const [showSearchBar, setShowSearchBarState] = useState(false);
  const [searchState, setSearchState] = useState<TerminalSearchState>(
    createDefaultTerminalSearchState,
  );
  const [historyState, setHistoryState] = useState<TerminalHistorySearchState>(
    createDefaultTerminalHistorySearchState,
  );
  const [searchFlags, setSearchFlags] = useState<TerminalSearchFlags>({
    caseSensitive: false,
    regex: false,
    wholeWord: false,
  });
  const [wrapAround, setWrapAroundState] = useState(true);
  const [activeMode, setActiveModeState] = useState<TerminalSearchMode>("buffer");

  const addonRef = useRef<TerminalSearchAddon | null>(null);
  const resultDisposableRef = useRef<Disposable | null>(null);
  const latestResultEventRef = useRef<TerminalSearchResultChangeEvent | null>(null);
  const pendingTimerRef = useRef<number | null>(null);
  const pendingHistoryTimerRef = useRef<number | null>(null);
  const searchVersionRef = useRef(0);
  const historySearchVersionRef = useRef(0);
  const searchStateRef = useRef(searchState);
  const historyStateRef = useRef(historyState);
  const searchFlagsRef = useRef(searchFlags);
  const wrapAroundRef = useRef(wrapAround);
  const optionsRef = useRef(options);
  const showSearchBarRef = useRef(showSearchBar);
  const activeModeRef = useRef(activeMode);

  useEffect(() => {
    optionsRef.current = options;
  }, [options]);

  useEffect(() => {
    showSearchBarRef.current = showSearchBar;
  }, [showSearchBar]);

  useEffect(() => {
    searchStateRef.current = searchState;
  }, [searchState]);

  useEffect(() => {
    historyStateRef.current = historyState;
  }, [historyState]);

  useEffect(() => {
    searchFlagsRef.current = searchFlags;
  }, [searchFlags]);

  useEffect(() => {
    wrapAroundRef.current = wrapAround;
  }, [wrapAround]);

  useEffect(() => {
    activeModeRef.current = activeMode;
  }, [activeMode]);

  const updateSearchState = useCallback((next: Partial<TerminalSearchState>) => {
    setSearchState((current) => {
      const updated = { ...current, ...next };
      searchStateRef.current = updated;
      return updated;
    });
  }, []);

  const cancelPendingSearch = useCallback(() => {
    if (pendingTimerRef.current !== null) {
      window.clearTimeout(pendingTimerRef.current);
      pendingTimerRef.current = null;
    }
  }, []);

  const cancelPendingHistorySearch = useCallback(() => {
    if (pendingHistoryTimerRef.current !== null) {
      window.clearTimeout(pendingHistoryTimerRef.current);
      pendingHistoryTimerRef.current = null;
    }
  }, []);

  const clearAddonSearch = useCallback(() => {
    try {
      addonRef.current?.clearDecorations?.();
      addonRef.current?.clearActiveDecoration?.();
      const terminal = optionsRef.current.terminal ?? terminalRef.current;
      terminal?.clearSelection();
    } catch {
      // Search cleanup should never break the search UI.
    }
  }, [terminalRef]);

  const resetSearch = useCallback(() => {
    cancelPendingSearch();
    cancelPendingHistorySearch();
    searchVersionRef.current += 1;
    historySearchVersionRef.current += 1;
    latestResultEventRef.current = null;
    clearAddonSearch();
    const nextState = createDefaultTerminalSearchState();
    const nextHistoryState = createDefaultTerminalHistorySearchState();
    searchStateRef.current = nextState;
    historyStateRef.current = nextHistoryState;
    setSearchState(nextState);
    setHistoryState(nextHistoryState);
  }, [cancelPendingHistorySearch, cancelPendingSearch, clearAddonSearch]);

  const runHistorySearch = useCallback(async (query: string, version: number) => {
    const currentOptions = optionsRef.current;
    const sessionId = currentOptions.sessionId;
    const flags = searchFlagsRef.current;

    if (!sessionId || !query) {
      return;
    }

    if (flags.regex && !validateSearchRegex(query, true)) {
      if (version !== historySearchVersionRef.current) return;
      setHistoryState({
        ...createDefaultTerminalHistorySearchState(),
        status: "error",
        query,
        error: "Invalid regular expression",
      });
      return;
    }

    if (version !== historySearchVersionRef.current) {
      return;
    }

    setHistoryState((current) => ({
      ...current,
      status: "searching",
      query,
      error: null,
    }));

    try {
      const response = await invoke<TerminalHistorySearchResponse>("terminal_history_search", {
        request: {
          sessionId,
          query,
          caseSensitive: flags.caseSensitive,
          regex: flags.regex,
          wholeWord: flags.wholeWord,
          limit: TERMINAL_HISTORY_RESULT_LIMIT,
          contextBefore: 1,
          contextAfter: 1,
          maxLines: TERMINAL_HISTORY_DEFAULT_LINES,
        },
      });

      if (version !== historySearchVersionRef.current) {
        return;
      }

      setHistoryState({
        status: "done",
        query,
        total: response.total,
        elapsedMs: response.elapsedMs,
        truncated: response.truncated,
        results: response.results,
        error: null,
      });
    } catch (error) {
      if (version !== historySearchVersionRef.current) {
        return;
      }

      setHistoryState({
        status: "error",
        query,
        total: 0,
        elapsedMs: null,
        truncated: false,
        results: [],
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }, []);

  const scheduleHistorySearch = useCallback(
    (query: string) => {
      cancelPendingHistorySearch();
      historySearchVersionRef.current += 1;
      const version = historySearchVersionRef.current;

      if (!query) {
        const nextHistoryState = createDefaultTerminalHistorySearchState();
        historyStateRef.current = nextHistoryState;
        setHistoryState(nextHistoryState);
        return;
      }

      if (!optionsRef.current.sessionId) {
        setHistoryState({
          ...createDefaultTerminalHistorySearchState(),
          status: "error",
          query,
          error: "Terminal history is not ready",
        });
        return;
      }

      setHistoryState((current) => ({
        ...current,
        status: "pending",
        query,
        error: null,
      }));

      pendingHistoryTimerRef.current = window.setTimeout(() => {
        pendingHistoryTimerRef.current = null;
        void runHistorySearch(query, version);
      }, optionsRef.current.debounceMs ?? TERMINAL_SEARCH_DEBOUNCE_MS);
    },
    [cancelPendingHistorySearch, runHistorySearch],
  );

  const registerSearchAddon = useCallback(
    (addon: TerminalSearchAddon | null) => {
      resultDisposableRef.current?.dispose();
      resultDisposableRef.current = null;
      addonRef.current = addon;
      latestResultEventRef.current = null;

      if (!addon?.onDidChangeResults) {
        return;
      }

      resultDisposableRef.current = addon.onDidChangeResults((event) => {
        latestResultEventRef.current = event;
        const current = searchStateRef.current;
        if (!current.query || current.status === "idle" || current.status === "pending") {
          return;
        }

        updateSearchState({
          activeIndex: normalizeResultIndex(event.resultIndex),
          resultCount: event.resultCount,
          status: event.resultCount > 0 ? "found" : "not-found",
        });
      });
    },
    [updateSearchState],
  );

  const runSearch = useCallback(
    (direction: TerminalSearchDirection, query: string, preview: boolean) => {
      cancelPendingSearch();
      searchVersionRef.current += 1;

      if (!query) {
        resetSearch();
        return;
      }

      const currentOptions = optionsRef.current;
      const flags = searchFlagsRef.current;
      const regex = flags.regex;
      const isRegexValid = validateSearchRegex(query, regex);

      if (!isRegexValid) {
        clearAddonSearch();
        updateSearchState({
          query,
          status: "error",
          activeIndex: null,
          resultCount: null,
          lastDirection: direction,
          error: "Invalid regular expression",
          isPreview: preview,
          isRegexValid: false,
        });
        return;
      }

      const terminal = currentOptions.terminal ?? terminalRef.current;
      const addon = addonRef.current;

      if (!terminal || !addon) {
        updateSearchState({
          query,
          status: "error",
          activeIndex: null,
          resultCount: null,
          lastDirection: direction,
          error: "Terminal search is not ready",
          isPreview: preview,
          isRegexValid: true,
        });
        return;
      }

      const currentState = searchStateRef.current;
      if (
        !preview &&
        currentState.query === query &&
        shouldBlockTerminalSearchNavigation(currentState, direction, wrapAroundRef.current)
      ) {
        if (currentOptions.focusTerminalAfterNavigation ?? false) {
          terminal.focus();
        }
        return;
      }

      updateSearchState({
        query,
        status: "searching",
        activeIndex: null,
        resultCount: null,
        lastDirection: direction,
        error: null,
        isPreview: preview,
        isRegexValid: true,
      });

      try {
        latestResultEventRef.current = null;
        const searchOptions = {
          caseSensitive: flags.caseSensitive,
          regex,
          wholeWord: flags.wholeWord,
          incremental: preview,
          decorations: currentOptions.decorations ?? DEFAULT_TERMINAL_SEARCH_DECORATIONS,
        };
        const found =
          direction === "next"
            ? addon.findNext(query, searchOptions)
            : addon.findPrevious(query, searchOptions);
        const resultEvent = getLatestResultEvent(latestResultEventRef);

        updateSearchState({
          query,
          status: found ? "found" : "not-found",
          activeIndex: resultEvent ? normalizeResultIndex(resultEvent.resultIndex) : null,
          resultCount: resultEvent ? resultEvent.resultCount : found ? null : 0,
          lastDirection: direction,
          error: null,
          isPreview: preview,
          isRegexValid: true,
        });

        if (!preview && (currentOptions.focusTerminalAfterNavigation ?? false)) {
          terminal.focus();
        }
      } catch (error) {
        updateSearchState({
          query,
          status: "error",
          activeIndex: null,
          resultCount: null,
          lastDirection: direction,
          error: error instanceof Error ? error.message : String(error),
          isPreview: preview,
          isRegexValid: true,
        });
      }
    },
    [cancelPendingSearch, clearAddonSearch, resetSearch, terminalRef, updateSearchState],
  );

  const schedulePreviewSearch = useCallback(
    (query: string) => {
      cancelPendingSearch();
      searchVersionRef.current += 1;

      if (!query) {
        resetSearch();
        return;
      }

      const currentOptions = optionsRef.current;
      const flags = searchFlagsRef.current;
      const regex = flags.regex;
      const isRegexValid = validateSearchRegex(query, regex);
      const visible = (currentOptions.visible ?? true) && showSearchBarRef.current;
      const performanceMode = currentOptions.performanceMode ?? "normal";
      const minQueryLength =
        currentOptions.minIncrementalQueryLength ??
        (performanceMode === "busy" ? 3 : TERMINAL_SEARCH_MIN_QUERY_LENGTH);
      const shouldPreview =
        visible &&
        performanceMode !== "overloaded" &&
        isRegexValid &&
        query.length >= minQueryLength;

      updateSearchState({
        query,
        status: shouldPreview ? "pending" : isRegexValid ? "idle" : "error",
        activeIndex: null,
        resultCount: null,
        lastDirection: searchStateRef.current.lastDirection,
        error: isRegexValid ? null : "Invalid regular expression",
        isPreview: true,
        isRegexValid,
      });

      if (!shouldPreview) {
        if (!isRegexValid || !visible || performanceMode === "overloaded") {
          clearAddonSearch();
        }
        return;
      }

      const version = searchVersionRef.current;
      const debounceMs =
        currentOptions.debounceMs ??
        (performanceMode === "busy" ? 220 : TERMINAL_SEARCH_DEBOUNCE_MS);
      pendingTimerRef.current = window.setTimeout(() => {
        pendingTimerRef.current = null;
        if (version !== searchVersionRef.current) {
          return;
        }
        runSearch("next", query, true);
      }, debounceMs);
    },
    [cancelPendingSearch, clearAddonSearch, resetSearch, runSearch, updateSearchState],
  );

  const setShowSearchBar = useCallback(
    (show: boolean) => {
      setShowSearchBarState(show);
      showSearchBarRef.current = show;

      if (!show) {
        resetSearch();
      }
    },
    [resetSearch],
  );

  const setSearchQuery = useCallback(
    (query: string) => {
      if (activeModeRef.current === "history") {
        updateSearchState({ query });
        scheduleHistorySearch(query);
        return;
      }

      schedulePreviewSearch(query);
    },
    [scheduleHistorySearch, schedulePreviewSearch, updateSearchState],
  );

  const setSearchFlag = useCallback(
    (flag: keyof TerminalSearchFlags, value: boolean) => {
      setSearchFlags((current) => {
        const next = { ...current, [flag]: value };
        searchFlagsRef.current = next;
        return next;
      });

      const query = searchStateRef.current.query;
      if (!query) return;

      clearAddonSearch();
      if (activeModeRef.current === "history") {
        scheduleHistorySearch(query);
      } else {
        schedulePreviewSearch(query);
      }
    },
    [clearAddonSearch, scheduleHistorySearch, schedulePreviewSearch],
  );

  const setWrapAround = useCallback((value: boolean) => {
    wrapAroundRef.current = value;
    setWrapAroundState(value);
  }, []);

  const setActiveMode = useCallback(
    (mode: TerminalSearchMode) => {
      setActiveModeState(mode);
      activeModeRef.current = mode;
      const query = searchStateRef.current.query;

      if (mode === "history") {
        clearAddonSearch();
        if (query) {
          scheduleHistorySearch(query);
        }
        return;
      }

      cancelPendingHistorySearch();
      historySearchVersionRef.current += 1;
      if (query) {
        schedulePreviewSearch(query);
      }
    },
    [cancelPendingHistorySearch, clearAddonSearch, scheduleHistorySearch, schedulePreviewSearch],
  );

  const handleSearchNext = useCallback(
    (query?: string) => {
      runSearch("next", typeof query === "string" ? query : searchStateRef.current.query, false);
    },
    [runSearch],
  );

  const handleSearchPrev = useCallback(
    (query?: string) => {
      runSearch(
        "previous",
        typeof query === "string" ? query : searchStateRef.current.query,
        false,
      );
    },
    [runSearch],
  );

  const handleCloseSearch = useCallback(() => {
    setShowSearchBarState(false);
    showSearchBarRef.current = false;
    resetSearch();
    terminalRef.current?.focus();
  }, [resetSearch, terminalRef]);

  useEffect(() => {
    return () => {
      cancelPendingSearch();
      cancelPendingHistorySearch();
      resultDisposableRef.current?.dispose();
      resultDisposableRef.current = null;
      clearAddonSearch();
      addonRef.current = null;
    };
  }, [cancelPendingHistorySearch, cancelPendingSearch, clearAddonSearch]);

  return useMemo(
    () => ({
      registerSearchAddon,
      showSearchBar,
      setShowSearchBar,
      searchQuery: searchState.query,
      setSearchQuery,
      searchState,
      searchFlags,
      setSearchFlag,
      wrapAround,
      setWrapAround,
      activeMode,
      setActiveMode,
      historyState,
      handleSearchNext,
      handleSearchPrev,
      handleCloseSearch,
    }),
    [
      registerSearchAddon,
      showSearchBar,
      setShowSearchBar,
      searchState,
      searchFlags,
      setSearchFlag,
      wrapAround,
      setWrapAround,
      activeMode,
      setActiveMode,
      historyState,
      setSearchQuery,
      handleSearchNext,
      handleSearchPrev,
      handleCloseSearch,
    ],
  );
}

function normalizeResultIndex(index: unknown) {
  return typeof index === "number" && index >= 0 ? index : null;
}

function getLatestResultEvent(ref: React.RefObject<TerminalSearchResultChangeEvent | null>) {
  return ref.current;
}

function validateSearchRegex(query: string, regex: boolean): boolean {
  if (!regex) {
    return true;
  }

  try {
    new RegExp(query);
    return true;
  } catch {
    return false;
  }
}
