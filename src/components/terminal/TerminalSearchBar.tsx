import { Repeat2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { MdClose, MdKeyboardArrowDown, MdKeyboardArrowUp } from "react-icons/md";
import type {
  TerminalHistorySearchState,
  TerminalSearchFlags,
  TerminalSearchMode,
  TerminalSearchState,
} from "@/lib/terminalSearch";
import {
  TERMINAL_HISTORY_RESULT_LIMIT,
  TERMINAL_SEARCH_VISIBLE_MATCH_LIMIT,
} from "@/lib/terminalSearch";

interface TerminalSearchBarProps {
  show: boolean;
  inputRef: React.RefObject<HTMLInputElement | null>;
  searchQuery: string;
  searchState: TerminalSearchState;
  searchFlags: TerminalSearchFlags;
  wrapAround: boolean;
  activeMode: TerminalSearchMode;
  historyState: TerminalHistorySearchState;
  setSearchQuery: (val: string) => void;
  onModeChange: (mode: TerminalSearchMode) => void;
  onSearchFlagChange: (flag: keyof TerminalSearchFlags, value: boolean) => void;
  onWrapAroundChange: (value: boolean) => void;
  onNext: () => void;
  onPrev: () => void;
  onClose: () => void;
}

export default function TerminalSearchBar({
  show,
  inputRef,
  searchQuery,
  searchState,
  searchFlags,
  wrapAround,
  activeMode,
  historyState,
  setSearchQuery,
  onModeChange,
  onSearchFlagChange,
  onWrapAroundChange,
  onNext,
  onPrev,
  onClose,
}: TerminalSearchBarProps) {
  const { t } = useTranslation();

  useEffect(() => {
    if (!show) return;

    const focusInput = () => {
      inputRef.current?.focus();
    };
    const animationFrameId = window.requestAnimationFrame(focusInput);
    const timeoutId = window.setTimeout(focusInput, 0);

    return () => {
      window.cancelAnimationFrame(animationFrameId);
      window.clearTimeout(timeoutId);
    };
  }, [show, inputRef]);

  const statusLabel = useMemo(
    () =>
      activeMode === "history"
        ? getHistoryStatusLabel(historyState, t)
        : getBufferStatusLabel(searchState, t),
    [activeMode, historyState, searchState, t],
  );

  if (!show) return null;

  return (
    <div
      className="absolute top-1 right-1 flex w-[420px] max-w-[calc(100%-0.5rem)] flex-col gap-1 rounded border px-2 py-1 shadow-lg z-50"
      style={{
        backgroundColor: "var(--df-bg-panel)",
        borderColor: "var(--df-border)",
        color: "var(--df-text)",
      }}
      onClick={(e) => e.stopPropagation()}
    >
      <div className="flex items-center gap-1">
        <ModeButton
          active={activeMode === "buffer"}
          label={t("terminalCtx.searchCurrentBuffer")}
          onClick={() => onModeChange("buffer")}
        />
        <ModeButton
          active={activeMode === "history"}
          label={t("terminalCtx.searchDeepHistory")}
          onClick={() => onModeChange("history")}
        />
        <div className="flex-1" />
        {statusLabel && (
          <span
            className="max-w-32 truncate text-right text-[11px]"
            style={{
              color:
                searchState.status === "error" || historyState.status === "error"
                  ? "var(--df-danger)"
                  : "var(--df-text-muted)",
            }}
            title={
              activeMode === "history"
                ? (historyState.error ?? statusLabel)
                : (searchState.error ?? statusLabel)
            }
          >
            {statusLabel}
          </span>
        )}
        <MdClose
          className="text-sm cursor-pointer hover:opacity-80"
          style={{ color: "var(--df-text-muted)" }}
          onClick={onClose}
          title={t("about.close")}
        />
      </div>

      <div className="flex items-center gap-1">
        <input
          ref={inputRef}
          type="text"
          className="min-w-0 flex-1 bg-transparent outline-none text-xs px-1 py-1"
          style={{ color: "var(--df-text)" }}
          placeholder={t("terminalCtx.find")}
          value={searchQuery}
          autoCapitalize="none"
          autoCorrect="off"
          spellCheck={false}
          autoComplete="off"
          onChange={(e) => {
            setSearchQuery(e.target.value);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              if (activeMode === "history") return;
              if (e.shiftKey) onPrev();
              else onNext();
            } else if (e.key === "Escape") {
              onClose();
            }
          }}
        />
        <FlagButton
          active={searchFlags.caseSensitive}
          label="Aa"
          title={t("terminalCtx.searchCaseSensitive")}
          onClick={() => onSearchFlagChange("caseSensitive", !searchFlags.caseSensitive)}
        />
        <FlagButton
          active={searchFlags.regex}
          label=".*"
          title={t("terminalCtx.searchRegex")}
          onClick={() => onSearchFlagChange("regex", !searchFlags.regex)}
        />
        <FlagButton
          active={searchFlags.wholeWord}
          label="Word"
          title={t("terminalCtx.searchWholeWord")}
          onClick={() => onSearchFlagChange("wholeWord", !searchFlags.wholeWord)}
        />
        {activeMode === "buffer" && (
          <>
            <button
              type="button"
              className="flex h-6 w-6 items-center justify-center rounded-sm hover:opacity-90"
              style={{
                backgroundColor: wrapAround ? "var(--df-accent)" : "transparent",
                color: wrapAround ? "var(--df-bg)" : "var(--df-text-muted)",
                border: "1px solid var(--df-border)",
              }}
              title={t("terminalCtx.searchWrapAround")}
              aria-label={t("terminalCtx.searchWrapAround")}
              aria-pressed={wrapAround}
              onClick={() => onWrapAroundChange(!wrapAround)}
            >
              <Repeat2 className="h-3.5 w-3.5" />
            </button>
            <MdKeyboardArrowUp
              className="text-sm cursor-pointer hover:opacity-80"
              style={{ color: "var(--df-text-muted)" }}
              onClick={() => onPrev()}
              title={t("terminalCtx.findPrevious")}
            />
            <MdKeyboardArrowDown
              className="text-sm cursor-pointer hover:opacity-80"
              style={{ color: "var(--df-text-muted)" }}
              onClick={() => onNext()}
              title={t("terminalCtx.findNext")}
            />
          </>
        )}
      </div>

      {activeMode === "history" && (
        <HistoryResults historyState={historyState} query={searchQuery} t={t} />
      )}
    </div>
  );
}

function ModeButton({
  active,
  label,
  onClick,
}: {
  active: boolean;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className="rounded-sm px-2 py-0.5 text-[11px] hover:opacity-90"
      style={{
        backgroundColor: active ? "var(--df-accent)" : "transparent",
        color: active ? "var(--df-bg)" : "var(--df-text-muted)",
      }}
      onClick={onClick}
    >
      {label}
    </button>
  );
}

function FlagButton({
  active,
  label,
  title,
  onClick,
}: {
  active: boolean;
  label: string;
  title: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className="h-6 rounded-sm px-1.5 text-[11px] hover:opacity-90"
      style={{
        backgroundColor: active ? "var(--df-accent)" : "transparent",
        color: active ? "var(--df-bg)" : "var(--df-text-muted)",
        border: "1px solid var(--df-border)",
      }}
      title={title}
      onClick={onClick}
    >
      {label}
    </button>
  );
}

function HistoryResults({
  historyState,
  query,
  t,
}: {
  historyState: TerminalHistorySearchState;
  query: string;
  t: (key: string, options?: Record<string, unknown>) => string;
}) {
  const [expandedLineId, setExpandedLineId] = useState<number | null>(null);

  if (!query) {
    return null;
  }

  if (historyState.status === "pending" || historyState.status === "searching") {
    return (
      <div className="py-2 text-xs" style={{ color: "var(--df-text-muted)" }}>
        {t("terminalCtx.findSearching")}
      </div>
    );
  }

  if (historyState.status === "error") {
    return (
      <div className="py-2 text-xs" style={{ color: "var(--df-danger)" }}>
        {historyState.error ?? t("terminalCtx.findError")}
      </div>
    );
  }

  if (historyState.status !== "done") {
    return null;
  }

  if (historyState.total === 0) {
    return (
      <div className="py-2 text-xs" style={{ color: "var(--df-text-muted)" }}>
        {t("terminalCtx.findNoResults")}
      </div>
    );
  }

  return (
    <div
      className="max-h-64 overflow-y-auto border-t pt-1"
      style={{ borderColor: "var(--df-border)" }}
    >
      <div className="mb-1 text-[11px]" style={{ color: "var(--df-text-muted)" }}>
        {t("terminalCtx.searchHistorySummary", {
          total: historyState.total,
          elapsed: historyState.elapsedMs ?? 0,
        })}
      </div>
      <div className="flex flex-col gap-1">
        {historyState.results.map((result) => (
          <button
            key={result.lineId}
            type="button"
            className="w-full rounded-sm px-1 py-1 text-left text-xs hover:opacity-90"
            style={{
              backgroundColor: "color-mix(in srgb, var(--df-bg-panel) 80%, var(--df-text) 20%)",
              color: "var(--df-text)",
            }}
            title={[...result.before, result.preview, ...result.after].join("\n")}
            onClick={() =>
              setExpandedLineId((current) => (current === result.lineId ? null : result.lineId))
            }
          >
            <div className="mb-0.5 text-[11px]" style={{ color: "var(--df-text-muted)" }}>
              {t("terminalCtx.searchHistoryLine", { line: result.lineNumber })}
            </div>
            <div className="truncate">{result.preview}</div>
            {expandedLineId === result.lineId && (
              <div
                className="mt-1 space-y-0.5 whitespace-pre-wrap border-t pt-1 font-mono text-[11px]"
                style={{ borderColor: "var(--df-border)", color: "var(--df-text-muted)" }}
              >
                {result.before.map((line, index) => (
                  <div key={`before-${result.lineId}-${index}`}>{line}</div>
                ))}
                <div style={{ color: "var(--df-text)" }}>{result.preview}</div>
                {result.after.map((line, index) => (
                  <div key={`after-${result.lineId}-${index}`}>{line}</div>
                ))}
              </div>
            )}
          </button>
        ))}
      </div>
      {historyState.total > TERMINAL_HISTORY_RESULT_LIMIT && (
        <div className="mt-1 text-[11px]" style={{ color: "var(--df-text-muted)" }}>
          {t("terminalCtx.searchHistoryShowing", {
            shown: historyState.results.length,
            total: historyState.total,
          })}
        </div>
      )}
    </div>
  );
}

function getBufferStatusLabel(searchState: TerminalSearchState, t: (key: string) => string) {
  if (!searchState.query) {
    return null;
  }

  if (searchState.status === "pending") {
    return null;
  }

  if (searchState.status === "searching") {
    return t("terminalCtx.findSearching");
  }

  if (searchState.status === "error") {
    return searchState.isRegexValid
      ? t("terminalCtx.findError")
      : t("terminalCtx.findInvalidRegex");
  }

  if (searchState.status === "not-found") {
    return t("terminalCtx.findNoResults");
  }

  if (searchState.status !== "found") {
    return null;
  }

  if (searchState.resultCount === null) {
    return t("terminalCtx.findFound");
  }

  if (searchState.resultCount >= TERMINAL_SEARCH_VISIBLE_MATCH_LIMIT) {
    return `${TERMINAL_SEARCH_VISIBLE_MATCH_LIMIT}+`;
  }

  if (searchState.activeIndex === null) {
    return String(searchState.resultCount);
  }

  return `${searchState.activeIndex + 1}/${searchState.resultCount}`;
}

function getHistoryStatusLabel(
  historyState: TerminalHistorySearchState,
  t: (key: string, options?: Record<string, unknown>) => string,
) {
  if (historyState.status === "pending" || historyState.status === "searching") {
    return t("terminalCtx.findSearching");
  }

  if (historyState.status === "error") {
    return t("terminalCtx.findError");
  }

  if (historyState.status !== "done") {
    return null;
  }

  return t("terminalCtx.searchHistoryCount", { total: historyState.total });
}
