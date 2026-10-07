import { randomUUID } from "@/lib/uuid";
import { getRemoteParentDirectory } from "@/components/panel/file-explorer/model";
import type { EnqueueUploadRequest } from "@/context/TransferContext";
import { invoke } from "@/lib/invoke";
import {
  showTransferDuplicatePrompt,
  type TransferDuplicatePromptChoice,
} from "@/lib/transferDuplicatePrompt";
import type { FileEntry, FileProperties } from "@/types/global";

export function findMissingRemoteEntries(
  sessionId: string,
  paths: string[],
): Promise<string[]> {
  return invoke<string[]>("find_missing_remote_entries", { sessionId, paths });
}

async function remotePathExists(
  sessionId: string,
  path: string,
): Promise<{ exists: boolean; isDirectory: boolean }> {
  try {
    const props = await invoke<FileProperties>("get_file_properties", {
      sessionId,
      path,
    });
    return { exists: true, isDirectory: props.is_dir };
  } catch {
    const parentDir = getRemoteParentDirectory(path);
    const fileName = path.split("/").filter(Boolean).pop() ?? "";
    if (!fileName) {
      return { exists: false, isDirectory: false };
    }

    try {
      const entries = await invoke<FileEntry[]>("list_remote_dir", {
        sessionId,
        path: parentDir,
      });
      const entry = entries.find((item) => item.name === fileName);
      if (entry) {
        return { exists: true, isDirectory: entry.is_dir };
      }
    } catch {
      // Fall through to "does not exist".
    }

    return { exists: false, isDirectory: false };
  }
}

async function resolveDuplicateChoice(params: {
  sessionId: string;
  remotePath: string;
  fileName: string;
  isDirectory: boolean;
  duplicateStrategy: string;
  allowApplyToTask?: boolean;
}): Promise<TransferDuplicatePromptChoice | "proceed" | "skip"> {
  const { sessionId, remotePath, fileName, isDirectory, duplicateStrategy, allowApplyToTask } =
    params;

  switch (duplicateStrategy) {
    case "skip":
      return "skip";
    case "overwrite":
    case "rename":
      return "proceed";
    case "ask":
      return showTransferDuplicatePrompt({
        requestId: randomUUID(),
        sessionId,
        remotePath,
        fileName,
        isDirectory,
        allowApplyToTask,
      });
    default:
      return "proceed";
  }
}

async function resolveRemoteUploadConflict(params: {
  sessionId: string;
  remotePath: string;
  fileName: string;
  isDirectory: boolean;
  duplicateStrategy: string;
  allowApplyToTask?: boolean;
}): Promise<"include" | "skip" | "includeAndOverwriteRemaining"> {
  const { exists, isDirectory } = await remotePathExists(params.sessionId, params.remotePath);
  if (!exists) {
    return "include";
  }

  const choice = await resolveDuplicateChoice({
    sessionId: params.sessionId,
    remotePath: params.remotePath,
    fileName: params.fileName,
    isDirectory: exists ? isDirectory : params.isDirectory,
    duplicateStrategy: params.duplicateStrategy,
    allowApplyToTask: params.allowApplyToTask,
  });

  if (choice === "skip") {
    return "skip";
  }
  if (choice === "overwriteAllForTask") {
    return "includeAndOverwriteRemaining";
  }

  return "include";
}

export async function filterEnqueueUploadRequests(
  requests: EnqueueUploadRequest[],
  duplicateStrategy: string,
): Promise<EnqueueUploadRequest[]> {
  if (duplicateStrategy !== "ask" && duplicateStrategy !== "skip") {
    return requests;
  }

  const filtered: EnqueueUploadRequest[] = [];
  const allowApplyToTask = duplicateStrategy === "ask" && requests.length > 1;
  let overwriteRemainingForTask = false;

  for (const request of requests) {
    if (overwriteRemainingForTask) {
      filtered.push({ ...request, duplicateStrategyOverride: "overwrite" });
      continue;
    }

    const decision = await resolveRemoteUploadConflict({
      sessionId: request.sessionId,
      remotePath: request.remotePath,
      fileName: request.fileName,
      isDirectory: request.kind === "directory",
      duplicateStrategy,
      allowApplyToTask,
    });

    if (decision === "skip") {
      continue;
    }
    if (decision === "includeAndOverwriteRemaining") {
      overwriteRemainingForTask = true;
    }

    filtered.push(
      duplicateStrategy === "ask"
        ? { ...request, duplicateStrategyOverride: "overwrite" }
        : request,
    );
  }

  return filtered;
}
