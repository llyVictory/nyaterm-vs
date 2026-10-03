import { supports, requireCapability } from "./backend/runtime";
import { Channel } from "@tauri-apps/api/core";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { invoke } from "@/lib/invoke";
import { logger } from "@/lib/logger";

export type UpdateStatus =
  | "idle"
  | "checking"
  | "available"
  | "downloading"
  | "ready"
  | "error";

export interface UpdateProgress {
  downloaded: number;
  total: number;
}

export interface UpdateInfo {
  version: string;
  date?: string;
  body?: string;
}

let cachedUpdate: Update | null = null;

export async function checkForUpdate(
  portable = false,
): Promise<UpdateInfo | null> {
  if (!supports("updater")) return null;
  logger.info({
    domain: "updater.flow",
    event: "update.check_started",
    message: "Checking for updates",
  });

  try {
    if (portable) {
      cachedUpdate = null;
      const update = await invoke<UpdateInfo | null>("check_portable_update");
      logger.info({
        domain: "updater.flow",
        event: update ? "update.available" : "update.check_succeeded",
        message: update ? "Portable update available" : "No portable update available",
        data: update
          ? {
              version: update.version,
              release_date: update.date,
              mode: "portable",
            }
          : { mode: "portable" },
      });
      return update;
    }

    const update = await check();
    if (!update) {
      cachedUpdate = null;
      logger.info({
        domain: "updater.flow",
        event: "update.check_succeeded",
        message: "No update available",
      });
      return null;
    }

    cachedUpdate = update;
    logger.info({
      domain: "updater.flow",
      event: "update.available",
      message: "Update available",
      data: {
        version: update.version,
        release_date: update.date,
      },
    });
    return {
      version: update.version,
      date: update.date,
      body: update.body,
    };
  } catch (error) {
    logger.error({
      domain: "updater.flow",
      event: "update.check_failed",
      message: "Update check failed",
      error,
    });
    throw error;
  }
}

export async function downloadAndInstallUpdate(
  portable: boolean,
  onProgress?: (progress: UpdateProgress) => void,
): Promise<void> {
  if (portable) {
    const progressChannel = new Channel<UpdateProgress>();
    progressChannel.onmessage = (progress) => onProgress?.(progress);
    logger.info({
      domain: "updater.flow",
      event: "update.download_started",
      message: "Starting portable update download",
      data: { mode: "portable" },
    });
    try {
      await invoke<void>("download_portable_update", { onProgress: progressChannel });
      logger.info({
        domain: "updater.flow",
        event: "update.download_finished",
        message: "Portable update download and verification finished",
        data: { mode: "portable" },
      });
      return;
    } catch (error) {
      logger.error({
        domain: "updater.flow",
        event: "update.install_failed",
        message: "Portable update staging failed",
        data: { mode: "portable" },
        error,
      });
      throw error;
    }
  }

  if (!cachedUpdate) throw new Error("No update available");

  let downloaded = 0;
  let total = 0;

  logger.info({
    domain: "updater.flow",
    event: "update.download_started",
    message: "Starting update download and install",
    data: {
      version: cachedUpdate.version,
    },
  });

  try {
    await cachedUpdate.downloadAndInstall((event) => {
      switch (event.event) {
        case "Started":
          total = event.data.contentLength ?? 0;
          onProgress?.({ downloaded: 0, total });
          break;
        case "Progress":
          downloaded += event.data.chunkLength;
          onProgress?.({ downloaded, total });
          break;
        case "Finished":
          onProgress?.({ downloaded: total, total });
          logger.info({
            domain: "updater.flow",
            event: "update.download_finished",
            message: "Update download finished",
            data: {
              version: cachedUpdate?.version,
              byte_size: total,
            },
          });
          break;
      }
    });
  } catch (error) {
    logger.error({
      domain: "updater.flow",
      event: "update.install_failed",
      message: "Update install failed",
      data: {
        version: cachedUpdate.version,
      },
      error,
    });
    throw error;
  }
}

export async function relaunchApp(portable = false): Promise<void> {
  requireCapability("updater");
  logger.info({
    domain: "updater.flow",
    event: "update.relaunch_requested",
    message: "Relaunching app after update",
  });
  if (portable) {
    await invoke<void>("apply_portable_update");
  } else {
    await relaunch();
  }
}
