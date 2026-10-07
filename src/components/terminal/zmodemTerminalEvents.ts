import type { Terminal } from "@xterm/xterm";
import { toast } from "sonner";
import { getLocalPathName } from "@/components/panel/file-explorer/model";
import { invoke } from "@/lib/invoke";
import {
  completePendingZmodemUpload,
  failPendingZmodemUpload,
  getPendingZmodemUploadConflictMode,
  getPendingZmodemUploadPaths,
  hasPendingZmodemUpload,
  markPendingZmodemUploadActive,
  probeAndResolveRemoteConflicts,
} from "@/lib/terminalZmodemUpload";

const PROGRESS_RENDER_INTERVAL_MS = 100;

type ZmodemDirection = "download" | "upload";
type ZmodemWireDirection = ZmodemDirection | "Download" | "Upload";

export type ZmodemEventPayload =
  | { type: "detected"; direction: ZmodemWireDirection }
  | {
      type: "progress";
      fileName?: string;
      file_name?: string;
      bytesTransferred?: number;
      bytes_transferred?: number;
      totalSize?: number;
      total_size?: number;
      localPath?: string;
      local_path?: string;
      direction: ZmodemWireDirection;
    }
  | {
      type: "complete";
      direction: ZmodemWireDirection;
      fileCount?: number;
      file_count?: number;
    }
  | { type: "failed"; reason: string };

type Translate = (key: string, opts?: Record<string, unknown>) => string;

export interface ZmodemEventHandler {
  handle(payload: ZmodemEventPayload): void;
  dispose(): void;
}

export interface ZmodemTransferProgressSink {
  upsertProgress: (progress: {
    id: string;
    sessionId: string;
    fileName: string;
    direction: ZmodemDirection;
    bytesTransferred: number;
    totalSize: number;
    localPath?: string;
  }) => void;
  complete: (id: string) => void;
  fail: (id: string, reason: string) => void;
}

type TerminalStatusWriter = (data: string) => void;

interface CurrentZmodemTransferFile {
  id: string;
  fileName: string;
  direction: ZmodemDirection;
}

type NormalizedZmodemPayload =
  | { type: "detected"; direction: ZmodemDirection }
  | (Extract<ZmodemEventPayload, { type: "progress" }> & {
      direction: ZmodemDirection;
    })
  | (Extract<ZmodemEventPayload, { type: "complete" }> & {
      direction: ZmodemDirection;
    })
  | { type: "failed"; reason: string };

function normalizeZmodemDirection(direction: string): ZmodemDirection | null {
  const normalized = direction.toLowerCase();
  return normalized === "download" || normalized === "upload" ? normalized : null;
}

function normalizeZmodemPayload(payload: ZmodemEventPayload): NormalizedZmodemPayload | null {
  if (payload.type === "failed") {
    return payload;
  }

  const direction = normalizeZmodemDirection(payload.direction);
  if (!direction) {
    return null;
  }

  return { ...payload, direction };
}

function getUploadFailureDescription(reason: string, t: Translate): string {
  if (reason.includes("destination may be unwritable")) {
    return t("zmodem.remoteRefusedUpload");
  }
  return reason;
}

export function createZmodemEventHandler(
  terminal: Terminal,
  sessionId: string,
  getT: () => Translate,
  getDuplicateStrategy: () => string = () => "ask",
  progressSink?: ZmodemTransferProgressSink,
  writeTerminalStatus: TerminalStatusWriter = (data) => terminal.write(data),
): ZmodemEventHandler {
  let pendingProgress: Extract<ZmodemEventPayload, { type: "progress" }> | null = null;
  let progressRaf: number | null = null;
  let progressTimer: number | null = null;
  let lastProgressWriteAt = 0;
  let uploadStarted = false;
  let uploadFileName = "";
  let uploadToastId: string | number | null = null;
  let currentTransferFile: CurrentZmodemTransferFile | null = null;
  let lastDownloadLocalPath = "";
  let transferFileSequence = 0;
  let disposed = false;

  const clearProgressTimer = () => {
    if (progressTimer !== null) {
      window.clearTimeout(progressTimer);
      progressTimer = null;
    }
  };

  const clearProgressRaf = () => {
    if (progressRaf !== null) {
      window.cancelAnimationFrame(progressRaf);
      progressRaf = null;
    }
  };

  const renderProgress = () => {
    progressRaf = null;
    clearProgressTimer();
    if (disposed || !pendingProgress) return;

    const payload = pendingProgress;
    pendingProgress = null;
    lastProgressWriteAt = Date.now();

    if (payload.direction === "upload") {
      const fileName = payload.fileName ?? payload.file_name ?? uploadFileName;
      if (fileName) {
        uploadFileName = fileName;
      }
      if (!uploadStarted) {
        uploadStarted = true;
        uploadToastId = toast.message(
          getT()("fileTransfer.uploadStarted", { name: uploadFileName || fileName }),
        );
      }
      return;
    }

    const fileName = payload.fileName ?? payload.file_name ?? "";
    const bytesTransferred = payload.bytesTransferred ?? payload.bytes_transferred ?? 0;
    const totalSize = payload.totalSize ?? payload.total_size ?? 0;
    const percent = totalSize > 0 ? Math.round((bytesTransferred / totalSize) * 100) : 0;
    const t = getT();
    writeTerminalStatus(`\r\x1b[36m[ZMODEM] ${t("zmodem.downloading", { fileName, percent })}\x1b[K`);
  };

  const scheduleProgressRender = () => {
    if (disposed) return;
    if (progressRaf !== null || progressTimer !== null) return;

    const elapsed = Date.now() - lastProgressWriteAt;
    if (elapsed >= PROGRESS_RENDER_INTERVAL_MS) {
      progressRaf = window.requestAnimationFrame(renderProgress);
      return;
    }

    progressTimer = window.setTimeout(() => {
      progressTimer = null;
      progressRaf = window.requestAnimationFrame(renderProgress);
    }, PROGRESS_RENDER_INTERVAL_MS - elapsed);
  };

  const flushProgress = () => {
    clearProgressRaf();
    clearProgressTimer();
    renderProgress();
  };

  const dismissUploadToast = () => {
    if (uploadToastId !== null) {
      toast.dismiss(uploadToastId);
      uploadToastId = null;
    }
  };

  const showUploadCompletedToast = () => {
    dismissUploadToast();
    const t = getT();
    const description = uploadFileName;
    toast.success(t("fileTransfer.uploadCompleted"), { description });
  };

  const buildTransferId = (direction: ZmodemDirection, fileName: string) => {
    transferFileSequence += 1;
    return `zmodem-${sessionId}-${direction}-${transferFileSequence}-${encodeURIComponent(fileName)}`;
  };

  const revealDownloadedFile = async (localPath: string) => {
    try {
      const { revealItemInDir } = await import("@/lib/backend/platform/opener");
      await revealItemInDir(localPath);
    } catch {
      try {
        const [{ openPath }, { dirname }] = await Promise.all([
          import("@/lib/backend/platform/opener"),
          import("@/lib/backend/platform/path"),
        ]);
        const parentPath = await dirname(localPath);
        if (!parentPath) return;
        await openPath(parentPath);
      } catch {
        // Best-effort convenience only; the transfer itself has already completed.
      }
    }
  };

  const syncTransferProgress = (
    payload: Extract<NormalizedZmodemPayload, { type: "progress" }>,
  ) => {
    if (!progressSink) return;

    const payloadFileName = payload.fileName ?? payload.file_name;
    const fileName = payloadFileName || uploadFileName || "zmodem_file";
    const bytesTransferred = payload.bytesTransferred ?? payload.bytes_transferred ?? 0;
    const totalSize = payload.totalSize ?? payload.total_size ?? 0;
    const localPath = payload.localPath ?? payload.local_path;
    if (payload.direction === "download" && localPath) {
      lastDownloadLocalPath = localPath;
    }
    const changedFile =
      !currentTransferFile ||
      currentTransferFile.fileName !== fileName ||
      currentTransferFile.direction !== payload.direction;

    if (changedFile) {
      if (currentTransferFile) {
        progressSink.complete(currentTransferFile.id);
      }
      currentTransferFile = {
        id: buildTransferId(payload.direction, fileName),
        fileName,
        direction: payload.direction,
      };
    }

    const activeTransferFile = currentTransferFile;
    if (!activeTransferFile) return;

    progressSink.upsertProgress({
      id: activeTransferFile.id,
      sessionId,
      fileName,
      direction: payload.direction,
      bytesTransferred,
      totalSize,
      localPath,
    });
  };

  const completeCurrentTransferFile = () => {
    if (!progressSink || !currentTransferFile) return;
    progressSink.complete(currentTransferFile.id);
    currentTransferFile = null;
  };

  const failCurrentTransferFile = (reason: string) => {
    if (!progressSink || !currentTransferFile) return;
    progressSink.fail(currentTransferFile.id, reason);
    currentTransferFile = null;
  };

  const handleDetected = async (
    payload: Extract<NormalizedZmodemPayload, { type: "detected" }>,
  ) => {
    const t = getT();
    if (disposed) return;

    if (payload.direction === "download") {
      writeTerminalStatus(`\r\n\x1b[36m[ZMODEM] ${t("zmodem.selectSaveDir")}\x1b[0m\r\n`);
      const dir = await invoke<string | null>("zmodem_pick_download_dir");
      if (disposed) return;
      if (dir) {
        await invoke("zmodem_accept_download", {
          sessionId,
          saveDir: dir,
        });
      } else {
        await invoke("zmodem_cancel", { sessionId });
        writeTerminalStatus(`\r\n\x1b[33m[ZMODEM] ${t("zmodem.cancelled")}\x1b[0m\r\n`);
      }
      return;
    }

    uploadStarted = false;
    uploadFileName = "";
    uploadToastId = null;

    const pendingPaths = getPendingZmodemUploadPaths(sessionId);
    if (pendingPaths) {
      markPendingZmodemUploadActive(sessionId);
      // Files were already resolved in uploadFilesViaZmodem before rz was sent —
      // accept immediately so the remote rz won't time out.
      if (pendingPaths.length === 1) {
        uploadFileName = getLocalPathName(pendingPaths[0] ?? "", "uploaded_file");
        uploadToastId = toast.message(t("fileTransfer.uploadStarted", { name: uploadFileName }));
        uploadStarted = true;
      }
      await invoke("zmodem_accept_upload", {
        sessionId,
        filePaths: pendingPaths,
        conflictMode: getPendingZmodemUploadConflictMode(sessionId),
      });
      return;
    }

    const selected = await invoke<string[] | null>("zmodem_pick_upload_files");
    if (disposed) return;
    if (selected && selected.length > 0) {
      const filePaths = selected.map(String);

      // Probe remote directory and delete conflicting files so plain rz
      // never sees an existing file to prompt about.
      const { paths: resolvedPaths, conflictMode } = await probeAndResolveRemoteConflicts(
        sessionId,
        filePaths,
        getDuplicateStrategy(),
      );

      if (resolvedPaths.length === 0) {
        await invoke("zmodem_cancel", { sessionId });
        writeTerminalStatus(`\r\n\x1b[33m[ZMODEM] ${t("zmodem.cancelled")}\x1b[0m\r\n`);
        return;
      }

      if (resolvedPaths.length === 1) {
        uploadFileName = getLocalPathName(resolvedPaths[0] ?? "", "uploaded_file");
        uploadToastId = toast.message(t("fileTransfer.uploadStarted", { name: uploadFileName }));
        uploadStarted = true;
      }
      await invoke("zmodem_accept_upload", {
        sessionId,
        filePaths: resolvedPaths,
        conflictMode,
      });
    } else {
      await invoke("zmodem_cancel", { sessionId });
      writeTerminalStatus(`\r\n\x1b[33m[ZMODEM] ${t("zmodem.cancelled")}\x1b[0m\r\n`);
    }
  };

  return {
    handle(payload) {
      if (disposed) return;

      const normalizedPayload = normalizeZmodemPayload(payload);
      if (!normalizedPayload) {
        return;
      }

      switch (normalizedPayload.type) {
        case "detected":
          if (normalizedPayload.direction === "upload" && hasPendingZmodemUpload(sessionId)) {
            markPendingZmodemUploadActive(sessionId);
          }
          void handleDetected(normalizedPayload);
          break;
        case "progress":
          if (normalizedPayload.direction === "upload") {
            markPendingZmodemUploadActive(sessionId);
          }
          syncTransferProgress(normalizedPayload);
          pendingProgress = normalizedPayload;
          scheduleProgressRender();
          break;
        case "complete": {
          flushProgress();
          completeCurrentTransferFile();
          if (normalizedPayload.direction === "upload") {
            showUploadCompletedToast();
            completePendingZmodemUpload(sessionId);
          } else {
            writeTerminalStatus(`\r\n\x1b[32m[ZMODEM] ${getT()("zmodem.complete")}\x1b[0m\r\n`);
            if (lastDownloadLocalPath) {
              void revealDownloadedFile(lastDownloadLocalPath);
            }
          }
          uploadStarted = false;
          uploadFileName = "";
          uploadToastId = null;
          lastDownloadLocalPath = "";
          break;
        }
        case "failed": {
          flushProgress();
          failCurrentTransferFile(normalizedPayload.reason);
          const isUpload = hasPendingZmodemUpload(sessionId) || uploadStarted;
          if (isUpload) {
            dismissUploadToast();
            if (normalizedPayload.reason !== "cancelled") {
              const t = getT();
              toast.error(
                t("fileTransfer.uploadFailed", {
                  name: uploadFileName || "file",
                }),
                {
                  description: getUploadFailureDescription(normalizedPayload.reason, t),
                },
              );
            }
            failPendingZmodemUpload(sessionId, normalizedPayload.reason, {
              uiHandled: normalizedPayload.reason !== "cancelled",
            });
          } else {
            writeTerminalStatus(
              `\r\n\x1b[31m[ZMODEM] ${getT()("zmodem.failed", {
                reason: normalizedPayload.reason,
              })}\x1b[0m\r\n`,
            );
          }
          uploadStarted = false;
          uploadFileName = "";
          uploadToastId = null;
          lastDownloadLocalPath = "";
          break;
        }
      }
    },
    dispose() {
      disposed = true;
      pendingProgress = null;
      uploadStarted = false;
      uploadFileName = "";
      uploadToastId = null;
      currentTransferFile = null;
      lastDownloadLocalPath = "";
      clearProgressRaf();
      clearProgressTimer();
    },
  };
}
