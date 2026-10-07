import { type ComponentType, useState } from "react";
import { useTranslation } from "react-i18next";
import { MdDataObject, MdOpenInNew, MdTerminal } from "react-icons/md";
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
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useApp } from "@/context/AppContext";
import { useConfigTransfer } from "@/hooks/useConfigTransfer";
import { pickBrowserFile } from "@/lib/backend/browserArtifacts";
import { importBrowserConnections } from "@/lib/backend/configTransfer";
import { open as openFileDialog } from "@/lib/backend/platform/dialog";
import { openUrl } from "@/lib/backend/platform/opener";
import { runtime } from "@/lib/backend/runtime";
import { invoke } from "@/lib/invoke";
import { logger } from "@/lib/logger";

interface ImportDialogProps {
  open: boolean;
  onClose: () => void;
}

interface ImportSource {
  id: string;
  name: string;
  icon: string | ComponentType<{ className?: string }>;
  extensions?: string[];
  hint?: string;
  picker?: "file" | "directory";
  labelKey?: string;
}

const IMPORT_SOURCES: ImportSource[] = [
  {
    id: "xshell",
    name: "Xshell",
    icon: "/icons/brands/Xshell.svg",
    extensions: ["xts"],
    hint: ".xts",
  },
  {
    id: "mobaxterm",
    name: "MobaXterm",
    icon: "/icons/brands/MobaXterm.svg",
    extensions: ["mxtsessions"],
    hint: ".mxtsessions",
  },
  {
    id: "windterm",
    name: "WindTerm",
    icon: "/icons/brands/WindTerm.svg",
    extensions: ["sessions"],
    hint: ".sessions",
  },
  {
    id: "securecrt",
    name: "SecureCRT",
    icon: "/icons/brands/SecureCRT.svg",
    extensions: ["xml"],
    hint: ".xml",
  },
  {
    id: "finalshell",
    name: "FinalShell",
    icon: "/icons/brands/FinalShell.svg",
    hint: "conn directory",
    picker: "directory",
  },
  {
    id: "termius",
    name: "Termius",
    icon: "/icons/brands/Termius.svg",
    hint: "local IndexedDB",
    picker: "directory",
  },
  {
    id: "electerm",
    name: "Electerm",
    icon: "/icons/brands/electerm.svg",
    extensions: ["json"],
    hint: ".json",
  },
  {
    id: "nyaterm_json",
    name: "JSON",
    icon: MdDataObject,
    extensions: ["json"],
    hint: ".json",
  },
  {
    id: "ssh_config",
    name: "SSH Config",
    icon: MdTerminal,
    hint: "~/.ssh/config",
    labelKey: "savedConnections.sshConfigSource",
  },
];

const SESSION_IMPORT_DOC_URLS = {
  zh: "https://nyaterm.app/docs/guide/ssh-connection#导入其他客户端的会话",
  en: "https://nyaterm.app/docs/guide/ssh-connection#import-sessions-from-other-clients",
};

export default function ImportDialog({ open, onClose }: ImportDialogProps) {
  const { i18n, t } = useTranslation();
  const { refreshConnections } = useApp();
  const { handleImport, passwordAlert } = useConfigTransfer();
  const [windtermImportPath, setWindtermImportPath] = useState<string | null>(null);
  const [windtermMasterPassword, setWindtermMasterPassword] = useState("");
  const [windtermImporting, setWindtermImporting] = useState(false);
  const [confirmBackupRestore, setConfirmBackupRestore] = useState(false);
  const [browserImporting, setBrowserImporting] = useState(false);
  const docsUrl = i18n.language.toLowerCase().startsWith("zh")
    ? SESSION_IMPORT_DOC_URLS.zh
    : SESSION_IMPORT_DOC_URLS.en;

  const renderSourceIcon = (source: ImportSource) => {
    if (typeof source.icon === "string") {
      return (
        <img
          src={`${import.meta.env.BASE_URL}${source.icon.replace(/^\//, "")}`}
          alt={source.name}
          className="h-10 w-10"
          draggable={false}
        />
      );
    }

    const Icon = source.icon;
    return <Icon className="h-10 w-10 text-[var(--df-primary)]" />;
  };

  const finishSessionImport = (count: number) => {
    if (count > 0) {
      toast.success(t("savedConnections.importSuccess", { count }));
      refreshConnections();
    } else {
      toast.info(t("savedConnections.importSuccess", { count: 0 }));
    }
  };

  const isWindtermMasterPasswordRequired = (error: unknown) =>
    String(error).includes("WindTerm master password is required");

  const importSelectedSessions = async (
    source: ImportSource,
    selected: string,
    windtermPassword?: string,
  ) => {
    const count =
      source.id === "termius"
        ? await invoke<number>("import_termius_sessions", { indexedDbPath: selected })
        : await invoke<number>("import_sessions", {
            filePath: selected,
            windtermMasterPassword: windtermPassword,
          });
    finishSessionImport(count);
  };

  const handleSelect = async (source: ImportSource) => {
    if (runtime === "web") {
      if (browserImporting || source.picker === "directory" || source.id === "ssh_config") return;
      setBrowserImporting(true);
      try {
        const file = await pickBrowserFile(
          (source.extensions ?? []).map((ext) => `.${ext}`).join(","),
        );
        if (!file) return;
        const result = await importBrowserConnections(source.id, file);
        finishSessionImport(result.imported);
        for (const warning of result.warnings)
          if (warning === "credentials_not_in_export") toast.info(t("web.importCredentialsHint"));
        onClose();
      } catch (error) {
        logger.error({
          domain: "settings.persistence",
          event: "connections.import_failed",
          message: "Browser connection import failed",
          error,
        });
        toast.error(
          String(error).includes("WindTerm profile resources")
            ? t("web.windtermResourcesRequired")
            : t("savedConnections.importFailed", { error: String(error) }),
        );
      } finally {
        setBrowserImporting(false);
      }
      return;
    }
    onClose();

    if (source.id === "ssh_config") {
      try {
        const count = await invoke<number>("import_ssh_config_hosts");
        if (count > 0) {
          toast.success(t("savedConnections.importSuccess", { count }));
        } else {
          toast.info(t("savedConnections.importSuccess", { count: 0 }));
        }
        refreshConnections();
      } catch (e) {
        logger.error({
          domain: "settings.persistence",
          event: "sessions.import_ssh_config_failed",
          message: "Import SSH config failed",
          error: e,
        });
        toast.error(t("savedConnections.importFailed", { error: e }));
      }
      return;
    }

    if (source.id === "termius") {
      try {
        const count = await invoke<number>("import_termius_sessions", { indexedDbPath: null });
        if (count > 0) {
          toast.success(t("savedConnections.importSuccess", { count }));
          refreshConnections();
        } else {
          toast.info(t("savedConnections.importSuccess", { count: 0 }));
        }
        return;
      } catch (e) {
        const errorText = String(e);
        if (!errorText.includes("Termius IndexedDB directory was not found")) {
          logger.error({
            domain: "settings.persistence",
            event: "sessions.import_termius_failed",
            message: "Import Termius sessions failed",
            error: e,
          });
          toast.error(t("savedConnections.importFailed", { error: e }));
          return;
        }
      }
    }

    const selected =
      source.picker === "directory"
        ? await openFileDialog({ directory: true, multiple: false })
        : await openFileDialog({
            multiple: false,
            filters: [{ name: source.name, extensions: source.extensions ?? [] }],
          });
    if (!selected) return;
    const selectedPath = Array.isArray(selected) ? selected[0] : selected;
    if (!selectedPath) return;
    try {
      await importSelectedSessions(source, selectedPath);
    } catch (e) {
      if (source.id === "windterm" && isWindtermMasterPasswordRequired(e)) {
        setWindtermImportPath(selectedPath);
        setWindtermMasterPassword("");
        return;
      }
      logger.error({
        domain: "settings.persistence",
        event: "sessions.import_failed",
        message: "Import sessions failed",
        error: e,
      });
      toast.error(t("savedConnections.importFailed", { error: e }));
    }
  };

  const handleWindtermPasswordSubmit = async () => {
    if (!windtermImportPath || windtermImporting) return;
    setWindtermImporting(true);
    try {
      await importSelectedSessions(
        IMPORT_SOURCES.find((source) => source.id === "windterm")!,
        windtermImportPath,
        windtermMasterPassword,
      );
      setWindtermImportPath(null);
      setWindtermMasterPassword("");
    } catch (error) {
      logger.error({
        domain: "settings.persistence",
        event: "sessions.import_windterm_failed",
        message: "Import WindTerm sessions failed",
        error,
      });
      toast.error(t("savedConnections.importFailed", { error }));
    } finally {
      setWindtermImporting(false);
    }
  };

  return (
    <>
      <Dialog open={open} onOpenChange={(v) => !v && !browserImporting && onClose()}>
        <DialogContent className="w-[min(480px,calc(100vw-2rem))] sm:max-w-[480px] p-6">
          <DialogHeader>
            <DialogTitle className="text-sm">{t("savedConnections.importDialogTitle")}</DialogTitle>
            <DialogDescription className="text-xs">
              {t("savedConnections.importSelectSource")}
            </DialogDescription>
          </DialogHeader>
          <section className="space-y-3">
            <h3 className="text-xs font-medium">{t("savedConnections.sessionImportTitle")}</h3>
            <div className="grid grid-cols-2 gap-3 sm:grid-cols-3">
              {IMPORT_SOURCES.map((source) => (
                <button
                  key={source.id}
                  type="button"
                  disabled={
                    browserImporting ||
                    (runtime === "web" &&
                      (source.picker === "directory" || source.id === "ssh_config"))
                  }
                  className="flex min-h-32 flex-col items-center justify-center gap-2 rounded-lg border p-3 text-center transition-colors hover:border-[var(--df-primary)] hover:bg-[color-mix(in_srgb,var(--df-primary)_8%,transparent)] cursor-pointer disabled:cursor-default disabled:opacity-50"
                  style={{ borderColor: "var(--df-border)" }}
                  onClick={() => handleSelect(source)}
                >
                  {renderSourceIcon(source)}
                  <span className="text-xs font-medium" style={{ color: "var(--df-text)" }}>
                    {source.labelKey ? t(source.labelKey) : source.name}
                  </span>
                  {source.hint && (
                    <span
                      className="text-[0.6rem] leading-tight text-center break-all"
                      style={{ color: "var(--df-text-dimmed)" }}
                    >
                      {runtime === "web" &&
                      (source.picker === "directory" || source.id === "ssh_config")
                        ? t("web.desktopOnly")
                        : source.hint}
                    </span>
                  )}
                </button>
              ))}
            </div>
            <div
              className="flex items-center justify-between gap-3 text-[0.6875rem]"
              style={{ color: "var(--df-text-dimmed)" }}
            >
              <div className="flex min-w-0 flex-1 items-center gap-1.5">
                <MdTerminal className="shrink-0 text-[0.85rem]" />
                <span className="leading-tight">{t("savedConnections.importMergeHint")}</span>
              </div>
              <button
                type="button"
                className="inline-flex shrink-0 items-center gap-1 rounded px-1.5 py-1 text-[0.6875rem] transition-colors hover:bg-[var(--df-bg-hover)]"
                style={{ color: "var(--df-primary)" }}
                onClick={() => void openUrl(encodeURI(docsUrl))}
              >
                {t("savedConnections.importDocs")}
                <MdOpenInNew className="text-[0.75rem]" />
              </button>
            </div>
          </section>
          <section className="space-y-2 border-t pt-4" style={{ borderColor: "var(--df-border)" }}>
            <h3 className="text-xs font-medium">{t("savedConnections.restoreBackupTitle")}</h3>
            <button
              type="button"
              className="flex w-full items-center gap-3 rounded-md border p-3 text-left transition-colors hover:border-[var(--df-primary)] hover:bg-[var(--df-bg-hover)] cursor-pointer"
              style={{ borderColor: "var(--df-border)" }}
              onClick={() => {
                onClose();
                setConfirmBackupRestore(true);
              }}
            >
              <img
                src={`${import.meta.env.BASE_URL}icons/app/nyaterm.svg`}
                alt=""
                className="h-8 w-8 shrink-0"
                draggable={false}
              />
              <span className="min-w-0">
                <span className="block text-xs font-medium">NyaTerm (.nya)</span>
                <span
                  className="block text-xs leading-snug"
                  style={{ color: "var(--df-text-dimmed)" }}
                >
                  {t("savedConnections.restoreBackupDesc")}
                </span>
              </span>
            </button>
          </section>
        </DialogContent>
      </Dialog>
      <AlertDialog open={confirmBackupRestore} onOpenChange={setConfirmBackupRestore}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("savedConnections.restoreBackupConfirmTitle")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("savedConnections.restoreBackupConfirmDesc")}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction variant="destructive" onClick={() => void handleImport()}>
              {t("savedConnections.restoreBackupConfirmAction")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
      <Dialog
        disablePointerDismissal
        open={windtermImportPath !== null}
        onOpenChange={(v) => {
          if (!v && !windtermImporting) {
            setWindtermImportPath(null);
            setWindtermMasterPassword("");
          }
        }}
      >
        <DialogContent className="w-[min(420px,calc(100vw-2rem))] sm:max-w-[420px]">
          <DialogHeader>
            <DialogTitle className="text-sm">
              {t("savedConnections.windtermMasterPasswordTitle")}
            </DialogTitle>
            <DialogDescription className="text-xs">
              {t("savedConnections.windtermMasterPasswordDesc")}
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-2">
            <Label htmlFor="windterm-master-password" className="text-xs">
              {t("savedConnections.windtermMasterPasswordLabel")}
            </Label>
            <Input
              id="windterm-master-password"
              type="password"
              value={windtermMasterPassword}
              disabled={windtermImporting}
              autoFocus
              onChange={(event) => setWindtermMasterPassword(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  void handleWindtermPasswordSubmit();
                }
              }}
            />
          </div>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              disabled={windtermImporting}
              onClick={() => {
                setWindtermImportPath(null);
                setWindtermMasterPassword("");
              }}
            >
              {t("common.cancel")}
            </Button>
            <Button
              type="button"
              disabled={windtermImporting}
              onClick={() => void handleWindtermPasswordSubmit()}
            >
              {windtermImporting
                ? t("savedConnections.windtermMasterPasswordImporting")
                : t("savedConnections.windtermMasterPasswordConfirm")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      {passwordAlert}
    </>
  );
}
