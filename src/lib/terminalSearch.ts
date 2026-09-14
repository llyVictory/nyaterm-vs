export type Disposable = { dispose: () => void };
export type TerminalSearchDirection = "next" | "previous";
export type TerminalSearchStatus =
  | "idle"
  | "pending"
  | "searching"
  | "found"
  | "not-found"
  | "error";
export type TerminalSearchPerformanceMode = "normal" | "busy" | "overloaded";
export type TerminalSearchMode = "buffer" | "history";

export const TERMINAL_SEARCH_DEBOUNCE_MS = 150;
export const TERMINAL_SEARCH_MIN_QUERY_LENGTH = 2;
export const TERMINAL_SEARCH_VISIBLE_MATCH_LIMIT = 1000;
export const TERMINAL_HISTORY_DEFAULT_LINES = 30_000;
export const TERMINAL_HISTORY_MAX_LINES = 100_000;
export const TERMINAL_HISTORY_RESULT_LIMIT = 100;

export interface TerminalSearchDecorations {
  matchBackground?: string;
  matchBorder?: string;
  matchOverviewRuler: string;
  activeMatchBackground?: string;
  activeMatchBorder?: string;
  activeMatchColorOverviewRuler: string;
}

export interface TerminalSearchOptions {
  caseSensitive?: boolean;
  regex?: boolean;
  wholeWord?: boolean;
  incremental?: boolean;
  decorations?: TerminalSearchDecorations;
}

export interface TerminalSearchResultChangeEvent {
  resultIndex: number;
  resultCount: number;
}

export interface TerminalSearchAddon {
  findNext: (query: string, options?: TerminalSearchOptions) => boolean;
  findPrevious: (query: string, options?: TerminalSearchOptions) => boolean;
  clearDecorations?: () => void;
  clearActiveDecoration?: () => void;
  onDidChangeResults?: (listener: (event: TerminalSearchResultChangeEvent) => void) => Disposable;
}

export interface TerminalSearchState {
  query: string;
  status: TerminalSearchStatus;
  activeIndex: number | null;
  resultCount: number | null;
  lastDirection: TerminalSearchDirection;
  error: string | null;
  isPreview: boolean;
  isRegexValid: boolean;
}

export interface TerminalSearchFlags {
  caseSensitive: boolean;
  regex: boolean;
  wholeWord: boolean;
}

export function shouldBlockTerminalSearchNavigation(
  state: TerminalSearchState,
  direction: TerminalSearchDirection,
  wrapAround: boolean,
): boolean {
  if (
    wrapAround ||
    state.status !== "found" ||
    state.activeIndex === null ||
    state.resultCount === null ||
    state.resultCount <= 0 ||
    state.resultCount >= TERMINAL_SEARCH_VISIBLE_MATCH_LIMIT
  ) {
    return false;
  }

  return direction === "next"
    ? state.activeIndex >= state.resultCount - 1
    : state.activeIndex === 0;
}

export type TerminalHistorySearchStatus = "idle" | "pending" | "searching" | "done" | "error";

export interface TerminalHistorySearchRequest {
  sessionId: string;
  query: string;
  caseSensitive: boolean;
  regex: boolean;
  wholeWord: boolean;
  limit: number;
  contextBefore: number;
  contextAfter: number;
  maxLines: number;
}

export interface TerminalHistorySearchResult {
  lineId: number;
  lineNumber: number;
  columnStart: number;
  columnEnd: number;
  preview: string;
  before: string[];
  after: string[];
  source: string;
}

export interface TerminalHistorySearchResponse {
  total: number;
  elapsedMs: number;
  truncated: boolean;
  results: TerminalHistorySearchResult[];
}

export interface TerminalHistorySearchState {
  status: TerminalHistorySearchStatus;
  query: string;
  total: number;
  elapsedMs: number | null;
  truncated: boolean;
  results: TerminalHistorySearchResult[];
  error: string | null;
}

export const DEFAULT_TERMINAL_SEARCH_DECORATIONS: TerminalSearchDecorations = {
  matchBackground: "#4f3f12",
  matchBorder: "#f5c542",
  matchOverviewRuler: "#f5c542",
  activeMatchBackground: "#ff9800",
  activeMatchBorder: "#ffb74d",
  activeMatchColorOverviewRuler: "#ff9800",
};

export function createDefaultTerminalSearchState(): TerminalSearchState {
  return {
    query: "",
    status: "idle",
    activeIndex: null,
    resultCount: null,
    lastDirection: "next",
    error: null,
    isPreview: false,
    isRegexValid: true,
  };
}

export function createDefaultTerminalHistorySearchState(): TerminalHistorySearchState {
  return {
    status: "idle",
    query: "",
    total: 0,
    elapsedMs: null,
    truncated: false,
    results: [],
    error: null,
  };
}
