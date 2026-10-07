import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useTransfer } from "@/context/TransferContext";
import { getErrorMessage } from "@/lib/errors";
import { supports } from "@/lib/backend/runtime";
import { invoke } from "@/lib/invoke";
import { showPasteConfirm } from "@/lib/pasteConfirmPrompt";
import {
  beginClipboardCapture,
  isLatestClipboardCapture,
  getFileClipboard,
  isRemoteClipboardDescendant,
  normalizeRemoteClipboardPath,
  observeFileClipboard,
  removeClipboardEntries,
  setFileClipboard,
  subscribeFileClipboard,
  type FileClipboardEntry,
  type FileClipboardMode,
} from "@/lib/sftpClipboard";
import type { FileProperties, SessionInfo } from "@/types/global";
import type { ResolvedLocalDropPathEntry } from "./model";
import { findMissingRemoteEntries } from "@/lib/transferDuplicateResolution";

export function useFileExplorerClipboard(
  sessionId: string | null,
  enabled: boolean,
  currentPath: string,
) {
  const { t } = useTranslation();
  const { enqueueCopies, enqueueUploads, transfers } = useTransfer();
  const clipboard = useSyncExternalStore(subscribeFileClipboard, getFileClipboard);
  const [hasLocalFiles, setHasLocalFiles] = useState(false);
  const busy = useRef(false);
  const endpoint = useRef({ sessionId, enabled, currentPath });
  endpoint.current = { sessionId, enabled, currentPath };
  useEffect(() => {
    if (!enabled) return;
    let disposed = false;
    let reading = false;
    const observe = async () => {
      if (reading || !document.hasFocus()) return;
      reading = true;
      try {
        const result = await observeFileClipboard();
        if (!disposed) setHasLocalFiles(result.paths.length > 0);
      } catch {
        /* Clipboard contention is transient; keep the last observation. */
      } finally {
        reading = false;
      }
    };
    void observe();
    const timer = setInterval(() => void observe(), 1500);
    window.addEventListener("focus", observe);
    return () => {
      disposed = true;
      clearInterval(timer);
      window.removeEventListener("focus", observe);
    };
  }, [enabled]);

  const copyEntries = useCallback(
    async (entries: FileClipboardEntry[], mode: FileClipboardMode) => {
      if (!sessionId || !enabled || !entries.length) return;
      if (
        !supports("recursiveTransfers") &&
        mode === "copy" &&
        entries.some((entry) => entry.isDirectory)
      ) {
        toast.error(t("fileExplorer.webRecursiveTransferUnavailable"));
        return;
      }
      const generation = beginClipboardCapture();
      try {
        // Establish an OS baseline before stamping the remote copy, so stale local files cannot win.
        await observeFileClipboard();
        const sessions = await invoke<SessionInfo[]>("list_sessions");
        const source = sessions.find((session) => session.id === sessionId && session.connected);
        if (!source) throw new Error(t("fileExplorer.pasteSessionUnavailable"));
        if (!isLatestClipboardCapture(generation) || endpoint.current.sessionId !== sessionId)
          return;
        setFileClipboard({
          sourceSessionId: sessionId,
          sourceStartedAt: source.started_at,
          mode,
          entries,
        });
      } catch (error) {
        toast.error(getErrorMessage(error));
      }
    },
    [enabled, sessionId, t],
  );

  const paste = useCallback(async () => {
    if (!sessionId || !enabled || !currentPath || busy.current) return;
    busy.current = true;
    const targetDir = normalizeRemoteClipboardPath(currentPath);
    const stillActive = () =>
      endpoint.current.sessionId === sessionId &&
      endpoint.current.enabled &&
      normalizeRemoteClipboardPath(endpoint.current.currentPath) === targetDir;
    try {
      const os = await observeFileClipboard();
      const remote = getFileClipboard();
      const sessions = await invoke<SessionInfo[]>("list_sessions");
      const target = sessions.find(
        (session) =>
          session.id === sessionId && session.connected && session.remote_file_browser_enabled,
      );
      if (!target) throw new Error(t("fileExplorer.pasteSessionUnavailable"));
      const destination = await invoke<FileProperties>("get_file_properties", {
        sessionId,
        path: targetDir,
      });
      if (!destination.is_dir) throw new Error(t("fileExplorer.pasteInvalidDestination"));
      if (os.preferLocal || (!remote && os.paths.length)) {
        const entries = await invoke<ResolvedLocalDropPathEntry[]>("resolve_local_drop_paths", {
          paths: os.paths,
        });
        if (!entries.length) throw new Error(t("fileExplorer.pasteClipboardEmpty"));
        if (
          !(await showPasteConfirm({
            action: "upload",
            count: entries.length,
            targetDir,
          })) ||
          !stillActive()
        )
          return;
        enqueueUploads(
          entries.map((entry) => {
            const fileName = entry.path.split(/[\\/]/).pop() ?? "";
            return {
              sessionId,
              fileName,
              localPath: entry.path,
              remotePath: `${targetDir.replace(/\/$/, "")}/${fileName}`,
              kind: entry.isDir ? "directory" : "file",
            };
          }),
        );
        return;
      }
      if (!remote?.entries.length) throw new Error(t("fileExplorer.pasteClipboardEmpty"));
      const source = sessions.find(
        (session) =>
          session.id === remote.sourceSessionId &&
          session.connected &&
          session.remote_file_browser_enabled,
      );
      if (!source || (remote.sourceStartedAt && source.started_at !== remote.sourceStartedAt)) {
        throw new Error(t("fileExplorer.pasteSessionUnavailable"));
      }
      const pendingPaths = new Set(
        transfers
          .filter(
            (transfer) =>
              transfer.moveSource &&
              transfer.sourceSessionId === remote.sourceSessionId &&
              ["queued", "transferring", "paused"].includes(transfer.status),
          )
          .map((transfer) => transfer.sourcePath),
      );
      let entries = remote.entries.filter(
        (entry) => remote.mode !== "cut" || !pendingPaths.has(entry.path),
      );
      if (!entries.length) return;
      if (
        !supports("recursiveTransfers") &&
        entries.some((entry) => entry.isDirectory) &&
        (remote.mode !== "cut" || source.id !== sessionId)
      ) {
        throw new Error(t("fileExplorer.webRecursiveTransferUnavailable"));
      }
      if (
        remote.sourceSessionId === sessionId &&
        entries.some(
          (entry) =>
            (entry.isDirectory && isRemoteClipboardDescendant(entry.path, targetDir)) ||
            normalizeRemoteClipboardPath(entry.path) ===
              normalizeRemoteClipboardPath(`${targetDir}/${entry.name}`),
        )
      ) {
        throw new Error(t("fileExplorer.pasteInsideSource"));
      }
      const missingPaths = new Set(
        await findMissingRemoteEntries(
          source.id,
          entries.map((entry) => entry.path),
        ),
      );
      const missing = entries.filter((entry) => missingPaths.has(entry.path));
      if (missing.length) {
        toast.error(t("fileExplorer.pasteSourceMissing", { count: missing.length }));
        if (remote.mode === "cut")
          removeClipboardEntries(
            source.id,
            remote.timestamp,
            missing.map((entry) => entry.path),
          );
        entries = entries.filter((entry) => !missing.includes(entry));
      }
      if (!entries.length || !stillActive()) return;
      if (
        !(await showPasteConfirm({
          action: remote.mode === "cut" ? "move" : "copy",
          count: entries.length,
          targetDir,
        })) ||
        !stillActive()
      )
        return;
      enqueueCopies(
        entries.map((entry) => ({
          fileName: entry.name,
          kind: entry.isDirectory ? "directory" : "file",
          source: { sessionId: source.id, kind: "remote", path: entry.path },
          target: { sessionId, kind: "remote", path: targetDir },
          moveSource: remote.mode === "cut",
          clipboardTimestamp: remote.timestamp,
          sourceStartedAt: remote.sourceStartedAt,
          targetStartedAt: target.started_at,
        })),
      );
    } catch (error) {
      toast.error(getErrorMessage(error));
    } finally {
      busy.current = false;
    }
  }, [currentPath, enabled, enqueueCopies, enqueueUploads, sessionId, t, transfers]);
  return {
    copyEntries,
    paste,
    canPaste: enabled && (!!clipboard?.entries.length || hasLocalFiles),
  };
}
