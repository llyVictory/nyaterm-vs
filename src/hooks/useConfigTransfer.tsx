import {
  open as openFileDialog,
  save as saveFileDialog,
} from "@/lib/backend/platform/dialog";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
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
import { invoke } from "@/lib/invoke";
import { logger } from "@/lib/logger";
import { openSettings } from "@/lib/windowManager";

export function useConfigTransfer() {
  const { t } = useTranslation();
  const { appSettings } = useApp();
  const [showPasswordAlert, setShowPasswordAlert] = useState(false);

  const hasMasterPassword = !!appSettings.security.master_password;

  const ensureMasterPassword = () => {
    if (hasMasterPassword) return true;
    setShowPasswordAlert(true);
    return false;
  };

  const handleExport = async () => {
    if (!ensureMasterPassword()) return;

    const path = await saveFileDialog({
      filters: [{ name: "NyaTerm Backup", extensions: ["nya"] }],
    });

    if (!path) return;

    try {
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
    }
  };

  const handleImport = async () => {
    if (!ensureMasterPassword()) return;

    const path = await openFileDialog({
      multiple: false,
      filters: [{ name: "NyaTerm Backup", extensions: ["nya"] }],
    });

    if (!path) return;

    try {
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
    const path = await saveFileDialog({
      filters: [{ name: "NyaTerm Diagnostics", extensions: ["zip"] }],
      defaultPath: "nyaterm-diagnostics.zip",
    });

    if (!path) return;

    try {
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
    }
  };

  const passwordAlert = (
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
  );

  return {
    handleExport,
    handleImport,
    handleOpenLogs,
    handleExportDiagnostics,
    passwordAlert,
  };
}
