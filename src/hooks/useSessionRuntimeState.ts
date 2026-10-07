import { supports } from "@/lib/backend/runtime";
import { listen } from "@/lib/backend/api";
import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@/lib/invoke";
import { logger } from "@/lib/logger";
import type { RecordingStatus, SessionInfo } from "@/types/global";

export function useSessionRuntimeState(
  memoryLimitBytes: number | undefined,
  settingsLoaded: boolean,
) {
  const [recordingStatuses, setRecordingStatuses] = useState<RecordingStatus[]>([]);
  const recordingSessions = useMemo(
    () => new Set(recordingStatuses.map((status) => status.sessionId)),
    [recordingStatuses],
  );
  const [liveSessionIds, setLiveSessionIds] = useState<Set<string> | null>(null);
  const [liveSessionsById, setLiveSessionsById] = useState<Map<string, SessionInfo> | null>(null);

  const refreshRecordingStatuses = useCallback(async () => {
    if (!supports("recording")) return;
    try {
      const statuses = await invoke<RecordingStatus[]>("list_recording_statuses");
      setRecordingStatuses(statuses);
    } catch (error) {
      logger.error({
        domain: "session.lifecycle",
        event: "recording.list_failed",
        message: "Failed to list recording sessions",
        error,
      });
    }
  }, []);

  useEffect(() => {
    void refreshRecordingStatuses();
    const unlistenSessions = listen("sessions-changed", () => {
      void refreshRecordingStatuses();
    });
    const unlistenRecording = listen<RecordingStatus>("recording-status-changed", () => {
      void refreshRecordingStatuses();
    });
    return () => {
      unlistenSessions.then((dispose) => dispose());
      unlistenRecording.then((dispose) => dispose());
    };
  }, [refreshRecordingStatuses]);

  useEffect(() => {
    let disposed = false;

    const refreshLiveSessions = async () => {
      try {
        const sessions = await invoke<SessionInfo[]>("list_sessions");
        if (!disposed) {
          setLiveSessionIds(new Set(sessions.map((session) => session.id)));
          setLiveSessionsById(new Map(sessions.map((session) => [session.id, session])));
        }
      } catch (error) {
        logger.error({
          domain: "session.lifecycle",
          event: "session.live_list_failed",
          message: "Failed to refresh live session ids",
          error,
        });
      }
    };

    void refreshLiveSessions();
    const unlisten = listen("sessions-changed", () => {
      void refreshLiveSessions();
    });

    return () => {
      disposed = true;
      unlisten.then((dispose) => dispose());
    };
  }, []);

  useEffect(() => {
    if (!settingsLoaded || !supports("recording")) return;
    void invoke("set_recording_memory_limit", {
      maxBytes: Math.max(1, memoryLimitBytes || 5 * 1024 * 1024),
    }).catch((error) => {
      logger.error({
        domain: "settings.persistence",
        event: "recording.memory_limit_sync_failed",
        message: "Failed to sync recording memory limit",
        error,
      });
    });
  }, [memoryLimitBytes, settingsLoaded]);

  return {
    recordingStatuses,
    recordingSessions,
    liveSessionIds,
    liveSessionsById,
    refreshRecordingStatuses,
  };
}
