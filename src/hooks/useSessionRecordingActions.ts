import { downloadDir } from "@/lib/backend/platform/path";
import type { TFunction } from "i18next";
import { useCallback } from "react";
import { toast } from "sonner";
import { invoke } from "@/lib/invoke";
import { logger } from "@/lib/logger";
import type { AppSettings, RecordingMode, SessionInfo } from "@/types/global";

function safeRecordingName(name: string) {
  return (
    name.normalize("NFC").replace(/[^\p{L}\p{M}\p{N}._-]+/gu, "_") || "session"
  );
}

function joinPath(dir: string, fileName: string) {
  return `${dir}${dir.endsWith("\\") || dir.endsWith("/") ? "" : "/"}${fileName}`;
}

interface SessionRecordingActionsOptions {
  recording: AppSettings["recording"];
  recordingSessions: Set<string>;
  liveSessionsById: Map<string, SessionInfo> | null;
  refreshRecordingStatuses: () => Promise<void>;
  t: TFunction;
}

export function useSessionRecordingActions({
  recording,
  recordingSessions,
  liveSessionsById,
  refreshRecordingStatuses,
  t,
}: SessionRecordingActionsOptions) {
  const buildRecordingFilePath = useCallback(
    async (prefix: "recording" | "session", sessionName: string) => {
      const dir = recording.base_path || (await downloadDir());
      const timestamp = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
      return joinPath(dir, `${prefix}-${safeRecordingName(sessionName)}-${timestamp}.log`);
    },
    [recording.base_path],
  );

  const handleToggleSessionRecordingById = useCallback(
    async (sessionId: string, mode: RecordingMode = "transcript") => {
      const isActive = recordingSessions.has(sessionId);

      if (isActive) {
        try {
          const savedPath = await invoke<string>("stop_recording", {
            sessionId,
          });
          await refreshRecordingStatuses();
          toast.success(t("recording.saved", { path: savedPath }));
        } catch (error) {
          logger.error({
            domain: "session.lifecycle",
            event: "recording.stop_failed",
            message: "Failed to stop recording",
            ids: { session_id: sessionId },
            error,
          });
          toast.error(t("recording.stopFailed"));
        }
        return;
      }

      try {
        await invoke<string>("start_recording", {
          request: {
            sessionId,
            mode,
            explicitPath: null,
          },
        });
        await refreshRecordingStatuses();
        toast.success(t("recording.started"));
      } catch (error) {
        logger.error({
          domain: "session.lifecycle",
          event: "recording.start_failed",
          message: "Failed to start recording",
          ids: { session_id: sessionId },
          error,
        });
        toast.error(t("recording.startFailed"));
      }
    },
    [refreshRecordingStatuses, recordingSessions, t],
  );

  const handleToggleSessionRecording = useCallback(
    async (session: SessionInfo, mode: RecordingMode = "transcript") => {
      await handleToggleSessionRecordingById(session.id, mode);
    },
    [handleToggleSessionRecordingById],
  );

  const handleSaveSessionTranscript = useCallback(
    async (session: SessionInfo) => {
      try {
        const filePath = await buildRecordingFilePath("session", session.name);
        const savedPath = await invoke<string>("save_session_transcript", {
          sessionId: session.id,
          filePath,
          includeIoLabels: recording.include_io_labels,
          includeTimestamps: recording.include_timestamps ?? true,
        });
        toast.success(t("recording.transcriptSaved", { path: savedPath }));
      } catch (error) {
        logger.error({
          domain: "session.lifecycle",
          event: "recording.transcript_save_failed",
          message: "Failed to save session transcript",
          ids: { session_id: session.id },
          error,
        });
        toast.error(t("recording.saveFailed"));
      }
    },
    [recording.include_io_labels, recording.include_timestamps, buildRecordingFilePath, t],
  );

  const handleSaveSessionTranscriptById = useCallback(
    async (sessionId: string, sessionName?: string) => {
      const session = liveSessionsById?.get(sessionId);
      await handleSaveSessionTranscript({
        id: sessionId,
        name: session?.name ?? sessionName ?? sessionId,
        session_type: session?.session_type ?? "Local",
        started_at: session?.started_at ?? new Date().toISOString(),
        connection_id: session?.connection_id ?? null,
        connected: session?.connected ?? true,
        owner_window_label: session?.owner_window_label ?? null,
        ai_execution_profile: session?.ai_execution_profile ?? "auto",
        injection_active: session?.injection_active ?? false,
        dynamic_title_enabled: session?.dynamic_title_enabled ?? false,
        dynamic_title_integration_active: session?.dynamic_title_integration_active ?? false,
        trusted_initial_title: session?.trusted_initial_title ?? null,
        remote_file_browser_enabled: session?.remote_file_browser_enabled ?? false,
        remote_stats_enabled: session?.remote_stats_enabled ?? false,
        ssh_profile: session?.ssh_profile ?? null,
      });
    },
    [handleSaveSessionTranscript, liveSessionsById],
  );

  return {
    handleToggleSessionRecordingById,
    handleToggleSessionRecording,
    handleSaveSessionTranscript,
    handleSaveSessionTranscriptById,
  };
}
