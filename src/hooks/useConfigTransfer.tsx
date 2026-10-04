import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { BrowserBackupDialog } from "@/components/dialog/app/BrowserBackupDialog";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { useApp } from "@/context/AppContext";
import { pickBrowserFile } from "@/lib/backend/browserArtifacts";
import {
  exportBrowserConfig,
  exportBrowserDiagnostics,
  importBrowserConfig,
} from "@/lib/backend/configTransfer";
import { open as openFileDialog, save as saveFileDialog } from "@/lib/backend/platform/dialog";
import { runtime } from "@/lib/backend/runtime";
import { invoke } from "@/lib/invoke";
import { logger } from "@/lib/logger";
import { openSettings } from "@/lib/windowManager";

export function useConfigTransfer() {
  const { t } = useTranslation();
  const { appSettings } = useApp();
  const [showPasswordAlert, setShowPasswordAlert] = useState(false);
  const [backupMode, setBackupMode] = useState<"import" | "export" | null>(null);
  const [backupFile, setBackupFile] = useState<File | null>(null);
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const [backupError, setBackupError] = useState("");
  const begin = () => {
    if (busyRef.current) return false;
    busyRef.current = true;
    setBusy(true);
    return true;
  };
  const finish = () => {
    busyRef.current = false;
    setBusy(false);
  };
  const closeBackup = () => {
    setBackupMode(null);
    setBackupFile(null);
    setBackupError("");
  };

  const hasMasterPassword = !!appSettings.security.master_password;

  const ensureMasterPassword = () => {
    if (hasMasterPassword) return true;
    setShowPasswordAlert(true);
    return false;
  };

  const handleExport = async () => {
    if (runtime === "web") {
      if (!busyRef.current) {
        setBackupError("");
        setBackupMode("export");
      }
      return;
    }
    if (!ensureMasterPassword()) return;
    if (!begin()) return;
    try {
      const path = await saveFileDialog({
        filters: [{ name: "NyaTerm Backup", extensions: ["nya"] }],
      });

      if (!path) return;

      await invoke("export_config", { outputPath: path });
      toast.success(t("settings.exportSuccess"));
    } catch (error) {
      logger.error({
        domain: "settings.persistence",
        event: "config.export_failed",
        message: "Export config failed",
        error,
      });
      toast.error(`${t("settings.exportFailed")}: ${error}`);
    } finally {
      finish();
    }
  };

  const handleImport = async () => {
    if (runtime === "web") {
      if (!begin()) return;
      try {
        const file = await pickBrowserFile(".nya");
        if (!file) return;
        if (file.size > 50 * 1024 * 1024) {
          toast.error(t("web.backupTooLarge"));
          return;
        }
        setBackupFile(file);
        setBackupError("");
        setBackupMode("import");
      } catch (error) {
        toast.error(`${t("settings.importFailed")}: ${error}`);
      } finally {
        finish();
      }
      return;
    }
    if (!ensureMasterPassword()) return;
    if (!begin()) return;
    try {
      const path = await openFileDialog({
        multiple: false,
        filters: [{ name: "NyaTerm Backup", extensions: ["nya"] }],
      });

      if (!path) return;

      await invoke("import_config", { filePath: path });
      toast.success(t("settings.importSuccess"));
    } catch (error) {
      logger.error({
        domain: "settings.persistence",
        event: "config.import_failed",
        message: "Import config failed",
        error,
      });
      toast.error(`${t("settings.importFailed")}: ${error}`);
    } finally {
      finish();
    }
  };

  const handleOpenLogs = async () => {
    try {
      await invoke("open_log_dir");
    } catch (error) {
      logger.error({
        domain: "ui.error",
        event: "logs.open_failed",
        message: "Failed to open logs",
        error,
      });
      toast.error(t("settings.openLogsFailed"));
    }
  };

  const handleExportDiagnostics = async () => {
    if (!begin()) return;
    try {
      if (runtime === "web") {
        await logger.flush();
        await exportBrowserDiagnostics();
        toast.success(t("settings.exportDiagnosticsSuccess"));
        return;
      }
      const path = await saveFileDialog({
        filters: [{ name: "NyaTerm Diagnostics", extensions: ["zip"] }],
        defaultPath: "nyaterm-diagnostics.zip",
      });

      if (!path) return;

      await invoke("export_diagnostics", { outputPath: path });
      toast.success(t("settings.exportDiagnosticsSuccess"));
    } catch (error) {
      logger.error({
        domain: "settings.persistence",
        event: "diagnostics.export_failed",
        message: "Export diagnostics failed",
        error,
      });
      toast.error(`${t("settings.exportDiagnosticsFailed")}: ${error}`);
    } finally {
      finish();
    }
  };

  const submitBackup = async (password: string) => {
    if (!backupMode || !begin()) return;
    setBackupError("");
    try {
      if (backupMode === "import" && backupFile) await importBrowserConfig(backupFile, password);
      else if (backupMode === "export") await exportBrowserConfig(password);
      toast.success(
        t(backupMode === "import" ? "settings.importSuccess" : "settings.exportSuccess"),
      );
      closeBackup();
    } catch (error) {
      logger.error({
        domain: "settings.persistence",
        event: `config.${backupMode}_failed`,
        message: "Browser backup operation failed",
        error,
      });
      const requestId =
        error && typeof error === "object" && "requestId" in error ? error.requestId : undefined;
      setBackupError(
        `${t(backupMode === "import" ? "web.backupImportFailed" : "settings.exportFailed")}${requestId ? ` (${requestId})` : ""}`,
      );
    } finally {
      finish();
    }
  };

  const passwordAlert = (
    <>
      {backupMode && (
        <BrowserBackupDialog
          key={backupMode}
          mode={backupMode}
          busy={busy}
          error={backupError}
          onClose={closeBackup}
          onSubmit={submitBackup}
        />
      )}
      <AlertDialog open={showPasswordAlert} onOpenChange={setShowPasswordAlert}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("settings.masterPasswordRequired")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("settings.masterPasswordRequiredDesc")}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                setShowPasswordAlert(false);
                openSettings("security");
              }}
            >
              {t("settings.security")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );

  return {
    handleExport,
    handleImport,
    handleOpenLogs,
    handleExportDiagnostics,
    passwordAlert,
    transferBusy: busy,
  };
}
