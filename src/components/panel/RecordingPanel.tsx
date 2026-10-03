import { listen } from "@/lib/backend/api";
import { memo, useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  MdFolderOpen,
  MdMoreVert,
  MdOutlineDescription,
  MdSave,
  MdSearch,
  MdStop,
  MdWarning,
} from "react-icons/md";
import { PiRecordFill } from "react-icons/pi";
import PanelHeader from "@/components/layout/PanelHeader";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import { invoke } from "@/lib/invoke";
import type { RecordingMode, RecordingStatus, SessionInfo } from "@/types/global";

interface RecordingPanelProps {
  activeSessionId: string | null;
  recordingStatuses: RecordingStatus[];
  onSessionClick: (sessionId: string) => void;
  onToggleRecording: (session: SessionInfo, mode?: RecordingMode) => Promise<void> | void;
  onSaveTranscript: (session: SessionInfo) => Promise<void> | void;
}

function shortSessionId(sessionId: string) {
  return sessionId.length <= 8 ? sessionId : sessionId.slice(0, 8);
}

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GB`;
}

function formatElapsed(startedAt: string, _tick: number) {
  const started = Date.parse(startedAt.replace(" ", "T"));
  if (!Number.isFinite(started)) return "";
  const total = Math.max(0, Math.floor((Date.now() - started) / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  return h > 0
    ? `${h}:${m.toString().padStart(2, "0")}:${s.toString().padStart(2, "0")}`
    : `${m}:${s.toString().padStart(2, "0")}`;
}

function modeLabel(mode: RecordingMode) {
  return mode === "raw" ? "Raw" : "Transcript";
}

function RecordingPanel({
  activeSessionId,
  recordingStatuses,
  onSessionClick,
  onToggleRecording,
  onSaveTranscript,
}: RecordingPanelProps) {
  const { t } = useTranslation();
  const [sessions, setSessions] = useState<SessionInfo[]>([]);
  const [search, setSearch] = useState("");
  const [busyActions, setBusyActions] = useState<Record<string, "record" | "save">>({});
  const [tick, setTick] = useState(0);

  const statusBySession = useMemo(
    () => new Map(recordingStatuses.map((status) => [status.sessionId, status])),
    [recordingStatuses],
  );

  const fetchSessions = useCallback(async () => {
    try {
      const sess = await invoke<SessionInfo[]>("list_sessions");
      sess.sort(
        (a, b) =>
          a.name.localeCompare(b.name, undefined, { sensitivity: "base" }) ||
          a.session_type.localeCompare(b.session_type),
      );
      setSessions(sess);
    } catch {
      // Backend might not be ready yet.
    }
  }, []);

  useEffect(() => {
    fetchSessions();
    const unlisten = listen("sessions-changed", () => {
      fetchSessions();
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [fetchSessions]);

  useEffect(() => {
    const id = window.setInterval(() => setTick((value) => value + 1), 1000);
    return () => window.clearInterval(id);
  }, []);

  const runAction = useCallback(
    async (session: SessionInfo, action: "record" | "save", mode?: RecordingMode) => {
      setBusyActions((prev) => ({ ...prev, [session.id]: action }));
      try {
        if (action === "record") {
          await onToggleRecording(session, mode);
        } else {
          await onSaveTranscript(session);
        }
      } finally {
        setBusyActions((prev) => {
          const next = { ...prev };
          delete next[session.id];
          return next;
        });
      }
    },
    [onSaveTranscript, onToggleRecording],
  );

  const openPath = useCallback(async (command: string, filePath: string) => {
    try {
      await invoke(command, { filePath });
    } catch {
      // The main action toast/logging is handled by the caller layer for recording writes.
    }
  }, []);

  const query = search.trim().toLowerCase();
  const filteredSessions = useMemo(
    () =>
      query
        ? sessions.filter((session) =>
            `${session.name} ${session.session_type} ${session.id}`.toLowerCase().includes(query),
          )
        : sessions,
    [query, sessions],
  );

  return (
    <div className="h-full flex flex-col overflow-hidden">
      <PanelHeader
        title={t("recording.panelTitle")}
        actions={
          <span className="text-[0.6875rem]" style={{ color: "var(--df-text-dimmed)" }}>
            {query ? `${filteredSessions.length}/${sessions.length}` : sessions.length}
          </span>
        }
      />

      <div
        className="nyaterm-wallpaper-transparent-surface border-b px-2 py-1.5"
        style={{ borderColor: "var(--df-border)", backgroundColor: "var(--df-bg-panel)" }}
      >
        <div className="relative shrink-0">
          <MdSearch
            className="pointer-events-none absolute left-2 top-1/2 -translate-y-1/2 text-[0.875rem]"
            style={{ color: "var(--df-text-dimmed)" }}
          />
          <Input
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            placeholder={t("recording.searchPlaceholder")}
            className="h-7 border-0 pl-7 text-xs shadow-none placeholder:text-[var(--df-text-dimmed)] text-[var(--df-text)] bg-[var(--df-bg-hover)] focus-visible:ring-1 focus-visible:ring-[var(--df-primary)] focus-visible:bg-transparent"
          />
        </div>
      </div>

      <div className="terminal-scroll flex-1 overflow-y-auto p-2 text-xs space-y-1">
        {sessions.length === 0 ? (
          <div
            className="text-center py-4 text-[0.6875rem]"
            style={{ color: "var(--df-text-dimmed)" }}
          >
            {t("panel.noActiveSessions")}
          </div>
        ) : filteredSessions.length === 0 ? (
          <div
            className="text-center py-4 text-[0.6875rem]"
            style={{ color: "var(--df-text-dimmed)" }}
          >
            {t("activeSessions.noMatches")}
          </div>
        ) : (
          filteredSessions.map((session) => {
            const status = statusBySession.get(session.id);
            const isCurrent = activeSessionId === session.id;
            const isRecording = !!status;
            const isProblem = status?.state === "failed" || status?.state === "degraded";
            return (
              <div
                key={session.id}
                className={`flex items-start gap-2 rounded-md p-2 transition-colors df-hover ${
                  isCurrent ? "ring-1 ring-[var(--df-primary)]/45" : ""
                }`}
                onClick={() => onSessionClick(session.id)}
              >
                <div
                  className={`mt-1.5 h-2 w-2 rounded-full shrink-0 ${
                    isRecording && !isProblem ? "animate-pulse" : ""
                  }`}
                  style={{
                    backgroundColor: isProblem ? "#f59e0b" : isRecording ? "#ef4444" : "#22c55e",
                  }}
                />
                <div className="min-w-0 flex-1">
                  <div className="flex min-w-0 items-center gap-2">
                    <span className="truncate" style={{ color: "var(--df-text)" }}>
                      {session.name}
                    </span>
                    <span
                      className="rounded px-1.5 py-0.5 text-[0.625rem] uppercase tracking-wide"
                      style={{
                        color: "var(--df-text-dimmed)",
                        backgroundColor: "var(--df-bg-hover)",
                      }}
                    >
                      {session.session_type}
                    </span>
                  </div>
                  <div
                    className="mt-0.5 min-w-0 font-mono text-[0.625rem]"
                    title={status?.filePath ?? session.id}
                    style={{ color: "var(--df-text-dimmed)" }}
                  >
                    {status ? (
                      <>
                        <div className="truncate">
                          {modeLabel(status.mode)} ·{" "}
                          {formatElapsed(status.startedAt, tick)} ·{" "}
                          {formatBytes(status.writtenBytes)}
                        </div>
                        <div className="truncate">{status.filePath}</div>
                        {isProblem && (
                          <div className="mt-1 flex items-center gap-1 text-[0.625rem] text-amber-500">
                            <MdWarning className="shrink-0" />
                            <span className="truncate">
                              {status.lastError || t("recording.degraded")}
                            </span>
                          </div>
                        )}
                      </>
                    ) : (
                      <span>{shortSessionId(session.id)}</span>
                    )}
                  </div>
                </div>
                <div className="flex items-center gap-1 shrink-0">
                  <TooltipProvider delayDuration={300}>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-7 w-7 rounded-md text-muted-foreground hover:text-foreground disabled:opacity-40"
                          disabled={!!busyActions[session.id]}
                          onClick={(event) => {
                            event.stopPropagation();
                            void runAction(session, "record", "transcript");
                          }}
                          aria-label={isRecording ? t("recording.stop") : t("recording.start")}
                        >
                          {isRecording ? (
                            <MdStop className="h-4 w-4" />
                          ) : (
                            <PiRecordFill className="h-4 w-4" />
                          )}
                        </Button>
                      </TooltipTrigger>
                      <TooltipContent>
                        {isRecording ? t("recording.stop") : t("recording.start")}
                      </TooltipContent>
                    </Tooltip>
                  </TooltipProvider>

                  <DropdownMenu>
                    <DropdownMenuTrigger asChild>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7 rounded-md text-muted-foreground hover:text-foreground"
                        onClick={(event) => event.stopPropagation()}
                      >
                        <MdMoreVert className="h-4 w-4" />
                      </Button>
                    </DropdownMenuTrigger>
                    <DropdownMenuContent align="end" onClick={(event) => event.stopPropagation()}>
                      {!isRecording && (
                        <>
                          <DropdownMenuItem
                            onClick={() => void runAction(session, "record", "transcript")}
                          >
                            <MdOutlineDescription className="mr-2 h-4 w-4" />
                            {t("recording.startTranscriptLog")}
                          </DropdownMenuItem>
                          <DropdownMenuItem onClick={() => void runAction(session, "record", "raw")}>
                            <PiRecordFill className="mr-2 h-4 w-4" />
                            {t("recording.startRawLog")}
                          </DropdownMenuItem>
                          <DropdownMenuSeparator />
                        </>
                      )}
                      {status && (
                        <>
                          <DropdownMenuItem
                            onClick={() => void openPath("open_recording_file", status.filePath)}
                          >
                            <MdOutlineDescription className="mr-2 h-4 w-4" />
                            {t("recording.openLog")}
                          </DropdownMenuItem>
                          <DropdownMenuItem
                            onClick={() =>
                              void openPath("show_recording_in_folder", status.filePath)
                            }
                          >
                            <MdFolderOpen className="mr-2 h-4 w-4" />
                            {t("recording.showInFolder")}
                          </DropdownMenuItem>
                          <DropdownMenuSeparator />
                        </>
                      )}
                      <DropdownMenuItem onClick={() => void runAction(session, "save")}>
                        <MdSave className="mr-2 h-4 w-4" />
                        {t("recording.saveTranscript")}
                      </DropdownMenuItem>
                    </DropdownMenuContent>
                  </DropdownMenu>
                </div>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}

export default memo(RecordingPanel);
