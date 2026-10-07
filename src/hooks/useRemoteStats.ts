import { supports } from "@/lib/backend/runtime";
import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@/lib/invoke";
import type { RemoteStats } from "@/types/global";

const MAX_CONSECUTIVE_FAILURES = 3;

interface OwnedRemoteStatsState {
  sessionId: string;
  stats: RemoteStats | null;
  error: boolean;
  isManualRefreshing: boolean;
}

export interface RemoteStatsState {
  sessionId: string | null;
  stats: RemoteStats | null;
  error: boolean;
  isManualRefreshing: boolean;
  refresh: () => void;
}

export function useRemoteStats(
  activeSessionId: string | null,
  enabled: boolean,
  intervalSeconds: number,
  liveSessionIds: ReadonlySet<string> | null = null,
): RemoteStatsState {
  const [state, setState] = useState<OwnedRemoteStatsState | null>(null);
  const statsCacheRef = useRef(new Map<string, RemoteStats>());
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const warmupRefreshRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const warmupRetrySessionRef = useRef<string | null>(null);
  const activeSessionRef = useRef<string | null>(null);
  const generationRef = useRef(0);
  const fetchingGenerationRef = useRef<number | null>(null);
  const failCountRef = useRef(0);
  const pollIntervalMs = Math.max(1, intervalSeconds) * 1000;
  const requestedSessionId =
    enabled && activeSessionId && (liveSessionIds === null || liveSessionIds.has(activeSessionId))
      ? activeSessionId
      : null;

  // Let async completions see a session switch before the corresponding effect flushes.
  activeSessionRef.current = requestedSessionId;

  const isCurrentRequest = useCallback(
    (sessionId: string, generation: number) =>
      activeSessionRef.current === sessionId && generationRef.current === generation,
    [],
  );

  const fetchStats = useCallback(
    async (sessionId: string, generation: number, manual = false) => {
      if (
        !supports("remoteMonitoring") ||
        !isCurrentRequest(sessionId, generation)
      )
        return null;
      if (fetchingGenerationRef.current === generation) return null;
      fetchingGenerationRef.current = generation;
      if (manual) {
        setState((current) =>
          current?.sessionId === sessionId ? { ...current, isManualRefreshing: true } : current,
        );
      }

      try {
        const data = await invoke<RemoteStats>("get_remote_stats", {
          sessionId,
        });
        if (
          !supports("remoteMonitoring") ||
          !isCurrentRequest(sessionId, generation)
        )
          return null;

        statsCacheRef.current.set(sessionId, data);
        setState((current) => ({
          sessionId,
          stats: data,
          error: false,
          isManualRefreshing: current?.sessionId === sessionId ? current.isManualRefreshing : false,
        }));
        failCountRef.current = 0;

        if (warmupRefreshRef.current) {
          clearTimeout(warmupRefreshRef.current);
          warmupRefreshRef.current = null;
        }

        if (data.cpu.usage_source === "warming_up" && warmupRetrySessionRef.current !== sessionId) {
          warmupRetrySessionRef.current = sessionId;
          warmupRefreshRef.current = setTimeout(() => {
            warmupRefreshRef.current = null;
            if (!isCurrentRequest(sessionId, generation)) return;
            void fetchStats(sessionId, generation);
          }, 1000);
        } else if (data.cpu.usage_source !== "warming_up") {
          warmupRetrySessionRef.current = null;
        }
        return data;
      } catch {
        if (
          !supports("remoteMonitoring") ||
          !isCurrentRequest(sessionId, generation)
        )
          return null;

        failCountRef.current += 1;
        const clearStats = failCountRef.current >= MAX_CONSECUTIVE_FAILURES;
        if (clearStats) statsCacheRef.current.delete(sessionId);
        setState((current) => ({
          sessionId,
          stats: clearStats || current?.sessionId !== sessionId ? null : current.stats,
          error: true,
          isManualRefreshing: current?.sessionId === sessionId ? current.isManualRefreshing : false,
        }));
        return null;
      } finally {
        if (fetchingGenerationRef.current === generation) {
          fetchingGenerationRef.current = null;
        }
        if (manual && isCurrentRequest(sessionId, generation)) {
          setState((current) =>
            current?.sessionId === sessionId ? { ...current, isManualRefreshing: false } : current,
          );
        }
      }
    },
    [isCurrentRequest],
  );

  const refresh = useCallback(() => {
    if (!requestedSessionId) return;
    void fetchStats(requestedSessionId, generationRef.current, true);
  }, [fetchStats, requestedSessionId]);

  useEffect(() => {
    // A null list means session discovery has not completed yet.
    if (liveSessionIds === null) return;
    for (const sessionId of statsCacheRef.current.keys()) {
      if (!liveSessionIds.has(sessionId)) statsCacheRef.current.delete(sessionId);
    }
  }, [liveSessionIds]);

  useEffect(() => {
    const generation = generationRef.current + 1;
    generationRef.current = generation;
    fetchingGenerationRef.current = null;

    if (pollRef.current) {
      clearInterval(pollRef.current);
      pollRef.current = null;
    }
    if (warmupRefreshRef.current) {
      clearTimeout(warmupRefreshRef.current);
      warmupRefreshRef.current = null;
    }

    failCountRef.current = 0;
    warmupRetrySessionRef.current = null;

    if (!requestedSessionId) {
      setState(null);
      return () => {
        if (generationRef.current === generation) generationRef.current += 1;
      };
    }

    setState({
      sessionId: requestedSessionId,
      stats: statsCacheRef.current.get(requestedSessionId) ?? null,
      error: false,
      isManualRefreshing: false,
    });
    void fetchStats(requestedSessionId, generation);
    pollRef.current = setInterval(
      () => void fetchStats(requestedSessionId, generation),
      pollIntervalMs,
    );

    return () => {
      if (generationRef.current === generation) generationRef.current += 1;
      if (pollRef.current) {
        clearInterval(pollRef.current);
        pollRef.current = null;
      }
      if (warmupRefreshRef.current) {
        clearTimeout(warmupRefreshRef.current);
        warmupRefreshRef.current = null;
      }
      warmupRetrySessionRef.current = null;
    };
  }, [fetchStats, pollIntervalMs, requestedSessionId]);

  const visibleState = state?.sessionId === requestedSessionId ? state : null;
  const cachedStats = requestedSessionId
    ? (statsCacheRef.current.get(requestedSessionId) ?? null)
    : null;

  return {
    sessionId: requestedSessionId,
    stats: visibleState ? visibleState.stats : cachedStats,
    error: visibleState?.error ?? false,
    isManualRefreshing: visibleState?.isManualRefreshing ?? false,
    refresh,
  };
}
