import { emit } from "@/lib/backend/api";
import { getCurrentWindow } from "@/lib/backend/platform/window";
import { useTranslation } from "react-i18next";
import { MdCloudSync } from "react-icons/md";
import ChildWindowHeader from "@/components/layout/ChildWindowHeader";
import { Button } from "@/components/ui/button";
import { invoke } from "@/lib/invoke";
import { logger } from "@/lib/logger";
import { parseJsonSearchParam } from "@/lib/utils";

export interface AutoUploadDialogData {
  sessionId: string;
  localPath: string;
  remotePath: string;
}

export default function AutoUploadPage() {
  const { t } = useTranslation();
  const params = new URLSearchParams(window.location.search);
  const dataParam = params.get("data");
  const data = parseJsonSearchParam<AutoUploadDialogData>(dataParam);

  const handleClose = () => getCurrentWindow().close();

  const handleUpload = (always: boolean) => {
    if (!data) return;

    if (always) {
      emit("auto-upload-decision", {
        sessionId: data.sessionId,
        localPath: data.localPath,
        remotePath: data.remotePath,
        always,
      });
    }

    invoke("upload_local_file", {
      sessionId: data.sessionId,
      localPath: data.localPath,
      remotePath: data.remotePath,
    }).catch((e) => {
      logger.error({
        domain: "transfer.lifecycle",
        event: "upload.auto_upload_failed",
        message: "Upload failed",
        ids: { session_id: data.sessionId },
        error: e,
      });
    });

    handleClose();
  };

  if (!data) return null;

  return (
    <div className="h-full min-h-0 flex flex-col overflow-hidden bg-background text-foreground">
      <ChildWindowHeader
        title={t("fileExplorer.fileModified")}
        icon={<MdCloudSync className="text-base" />}
        onClose={handleClose}
      />

      <div className="flex-1 min-h-0 overflow-y-auto p-5">
        <p className="text-[13px] leading-relaxed text-foreground/90">
          {t("fileExplorer.uploadPrompt")}
        </p>
        <div
          className="mt-3 min-w-0 break-all rounded-md border bg-muted/40 px-3 py-2.5 font-mono text-xs text-muted-foreground whitespace-pre-wrap select-text"
          title={data.remotePath}
        >
          {data.remotePath}
        </div>
      </div>

      <div className="flex shrink-0 flex-row gap-2 border-t bg-muted/20 px-5 py-3 justify-end items-center">
        <Button variant="ghost" size="sm" className="text-xs px-4" onClick={handleClose}>
          {t("dialog.cancel")}
        </Button>
        <Button
          variant="outline"
          size="sm"
          className="text-xs px-4"
          onClick={() => handleUpload(true)}
        >
          {t("fileExplorer.alwaysUpload")}
        </Button>
        <Button size="sm" className="text-xs px-4" onClick={() => handleUpload(false)}>
          {t("fileExplorer.uploadOnce")}
        </Button>
      </div>
    </div>
  );
}
