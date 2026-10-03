import { emit } from "@/lib/backend/api";
import { open as openFileDialog } from "@/lib/backend/platform/dialog";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  MdAdd,
  MdDelete,
  MdEdit,
  MdQrCodeScanner,
  MdRefresh,
  MdSend,
  MdVisibility,
  MdVisibilityOff,
} from "react-icons/md";
import { toast } from "sonner";
import { OtpDeleteDialog } from "@/components/dialog/security-auth/OtpDeleteDialog";
import { OtpEditorDialog } from "@/components/dialog/security-auth/OtpEditorDialog";
import { OtpCodePanel } from "@/components/panel/security-auth/OtpCodePanel";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { invoke } from "@/lib/invoke";
import { sendSessionInput } from "@/lib/sessionInput";
import type { OtpCodeResult, OtpEntry } from "@/types/global";

interface OtpManagementTabProps {
  activeSessionId?: string | null;
  onCountChange?: (count: number) => void;
}

function otpTypeTagClass(otpType: string) {
  return otpType === "hotp" ? "text-amber-600 dark:text-amber-400" : "text-primary";
}

export function OtpManagementTab({ activeSessionId = null, onCountChange }: OtpManagementTabProps) {
  const { t } = useTranslation();
  const [entries, setEntries] = useState<OtpEntry[]>([]);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editEntry, setEditEntry] = useState<Partial<OtpEntry>>({});
  const [secretLoading, setSecretLoading] = useState(false);
  const [isNew, setIsNew] = useState(false);
  const [qrImporting, setQrImporting] = useState(false);
  const [deletingEntry, setDeletingEntry] = useState<OtpEntry | null>(null);
  const [visibleOtpIds, setVisibleOtpIds] = useState<Set<string>>(() => new Set());
  const editRequestRef = useRef(0);

  const loadEntries = useCallback(async () => {
    try {
      const result = await invoke<OtpEntry[]>("get_otp_entries");
      setEntries(result);
      setVisibleOtpIds((prev) => {
        const next = new Set<string>();
        for (const entry of result) {
          if (prev.has(entry.id)) next.add(entry.id);
        }
        return next;
      });
      onCountChange?.(result.length);
    } catch {
      /* ignore */
    }
  }, [onCountChange]);

  useEffect(() => {
    void loadEntries();
  }, [loadEntries]);

  const resetEdit = useCallback(() => {
    editRequestRef.current += 1;
    setEditingId(null);
    setEditEntry({});
    setSecretLoading(false);
    setIsNew(false);
  }, []);

  const handleAdd = useCallback(() => {
    resetEdit();
    setEditingId("__new__");
    setEditEntry({
      algorithm: "SHA1",
      counter: 0,
      digits: 6,
      otp_type: "totp",
      period: 30,
    });
    setIsNew(true);
  }, [resetEdit]);

  const handleImportQr = useCallback(async () => {
    if (qrImporting || editingId !== null) return;

    setQrImporting(true);
    try {
      const selected = await openFileDialog({
        multiple: false,
        title: t("otpManager.selectQrImage"),
        filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "bmp", "gif", "webp"] }],
      });
      const selectedPath = Array.isArray(selected) ? selected[0] : selected;
      if (!selectedPath) return;

      const parsed = await invoke<OtpEntry>("import_otp_from_qr", { path: selectedPath });
      resetEdit();
      setEditingId("__new__");
      setEditEntry({
        algorithm: parsed.algorithm,
        counter: parsed.counter,
        digits: parsed.digits,
        issuer: parsed.issuer,
        otp_type: parsed.otp_type,
        period: parsed.period,
        secret: parsed.secret ?? "",
        username: parsed.username,
      });
      setIsNew(true);
    } catch (error) {
      toast.error(t("otpManager.qrImportFailed"), { description: String(error) });
    } finally {
      setQrImporting(false);
    }
  }, [editingId, qrImporting, resetEdit, t]);

  const handleEdit = useCallback(async (entry: OtpEntry) => {
    const requestId = ++editRequestRef.current;
    setEditingId(entry.id);
    setEditEntry({
      ...entry,
      secret: "",
    });
    setSecretLoading(true);
    setIsNew(false);

    try {
      const secret = await invoke<string | null>("get_otp_secret_value", { id: entry.id });
      if (editRequestRef.current !== requestId) return;
      setEditEntry((prev) => ({
        ...prev,
        secret: secret ?? "",
      }));
    } catch {
      if (editRequestRef.current !== requestId) return;
      setEditEntry((prev) => ({
        ...prev,
        secret: "",
      }));
    } finally {
      if (editRequestRef.current === requestId) {
        setSecretLoading(false);
      }
    }
  }, []);

  const handleSave = useCallback(async () => {
    if (!editEntry.issuer?.trim()) return;
    if (isNew && !editEntry.secret) return;

    try {
      await invoke("save_otp_entry", {
        entry: {
          algorithm: editEntry.algorithm ?? "SHA1",
          counter: editEntry.counter ?? 0,
          digits: editEntry.digits ?? 6,
          id: isNew ? "" : editingId,
          issuer: editEntry.issuer.trim(),
          otp_type: editEntry.otp_type ?? "totp",
          period: editEntry.period ?? 30,
          secret: editEntry.secret || undefined,
          username: editEntry.username?.trim() ?? "",
        },
      });
      resetEdit();
      await loadEntries();
    } catch {
      /* ignore */
    }
  }, [editEntry, editingId, isNew, loadEntries, resetEdit]);

  const handleDeleteConfirm = useCallback(async () => {
    if (!deletingEntry) return;

    try {
      await invoke("delete_otp_entry", { id: deletingEntry.id });
      await loadEntries();
    } catch {
      /* ignore */
    }

    setDeletingEntry(null);
  }, [deletingEntry, loadEntries]);

  const handleChange = useCallback((patch: Partial<OtpEntry>) => {
    setEditEntry((prev) => ({ ...prev, ...patch }));
  }, []);

  const toggleCodeVisibility = useCallback((id: string) => {
    setVisibleOtpIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const handleSendToTerminal = useCallback(
    async (entry: OtpEntry) => {
      if (!activeSessionId) {
        return;
      }

      try {
        const result = await invoke<OtpCodeResult>("generate_otp_code", { id: entry.id });
        await sendSessionInput(activeSessionId, result.code, {
          preview: null,
          registerSubmission: null,
          origin: "otp_autofill",
          sensitivity: "secret",
        });
        await emit(`focus-terminal-${activeSessionId}`);
        if (entry.otp_type === "hotp") {
          await loadEntries();
        }
      } catch (error) {
        toast.error(t("otpManager.sendToTerminalFailed"), { description: String(error) });
      }
    },
    [activeSessionId, loadEntries, t],
  );

  const actionsDisabled = editingId !== null || qrImporting;

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="min-h-0 flex-1 overflow-y-auto px-3 pb-3 terminal-scroll">
        <div className="space-y-2">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <Label className="min-w-0 text-sm font-medium">{t("otpManager.title")}</Label>
            <div className="flex shrink-0 items-center gap-1">
              <Button
                variant="ghost"
                size="sm"
                className="h-7 w-7 p-0 text-primary"
                onClick={() => void handleImportQr()}
                disabled={actionsDisabled}
                title={t("otpManager.scanQr")}
                aria-label={t("otpManager.scanQr")}
              >
                {qrImporting ? (
                  <MdRefresh className="text-base animate-spin" />
                ) : (
                  <MdQrCodeScanner className="text-base" />
                )}
              </Button>
              <Button
                variant="ghost"
                size="sm"
                className="h-7 w-7 p-0 text-primary"
                onClick={handleAdd}
                disabled={actionsDisabled}
                title={t("otpManager.add")}
                aria-label={t("otpManager.add")}
              >
                <MdAdd className="text-base" />
              </Button>
            </div>
          </div>

          <div className="overflow-hidden rounded-md border">
            {entries.map((entry) => {
              const codeVisible = visibleOtpIds.has(entry.id);

              return (
                <div
                  key={entry.id}
                  className="border-b px-3 py-3 last:border-0 transition-colors hover:bg-accent/35"
                >
                  <div className="flex min-w-0 items-start justify-between gap-3">
                    <div className="min-w-0 flex-1">
                      <div className="flex min-w-0 items-center justify-between gap-3">
                        <div className="min-w-0 truncate text-sm font-medium">{entry.issuer}</div>
                        <span
                          className={`shrink-0 font-mono text-[0.625rem] font-semibold uppercase tracking-[0.18em] ${otpTypeTagClass(entry.otp_type)}`}
                        >
                          [{entry.otp_type.toUpperCase()}]
                        </span>
                      </div>
                      <div className="mt-1 truncate text-[0.6875rem] text-muted-foreground">
                        {entry.username}
                      </div>

                      <div className="mt-2 grid grid-cols-4 items-center gap-1">
                        <Button
                          variant="ghost"
                          size="sm"
                          className="h-7 w-full p-0 text-muted-foreground hover:text-foreground"
                          onClick={() => toggleCodeVisibility(entry.id)}
                          disabled={editingId !== null}
                          title={
                            codeVisible ? t("otpManager.hideCodes") : t("otpManager.showCodes")
                          }
                          aria-label={
                            codeVisible ? t("otpManager.hideCodes") : t("otpManager.showCodes")
                          }
                        >
                          {codeVisible ? (
                            <MdVisibilityOff className="text-sm" />
                          ) : (
                            <MdVisibility className="text-sm" />
                          )}
                        </Button>
                        <Button
                          variant="ghost"
                          size="sm"
                          className="h-7 w-full p-0 text-muted-foreground hover:text-foreground"
                          onClick={() => {
                            void handleEdit(entry);
                          }}
                          disabled={editingId !== null}
                          title={t("common.edit")}
                          aria-label={t("common.edit")}
                        >
                          <MdEdit className="text-sm" />
                        </Button>
                        <Button
                          variant="ghost"
                          size="sm"
                          className="h-7 w-full p-0 text-muted-foreground hover:text-foreground"
                          onClick={() => {
                            void handleSendToTerminal(entry);
                          }}
                          disabled={editingId !== null || !activeSessionId}
                          title={t("otp.sendToTerminal")}
                          aria-label={t("otp.sendToTerminal")}
                        >
                          <MdSend className="text-sm" />
                        </Button>
                        <Button
                          variant="ghost"
                          size="sm"
                          className="h-7 w-full p-0 text-destructive hover:bg-destructive/10 hover:text-destructive"
                          onClick={() => setDeletingEntry(entry)}
                          disabled={editingId !== null}
                          title={t("common.delete")}
                          aria-label={t("common.delete")}
                        >
                          <MdDelete className="text-sm" />
                        </Button>
                      </div>
                    </div>
                  </div>

                  {codeVisible ? (
                    <OtpCodePanel
                      className="mt-3"
                      otpEntryId={entry.id}
                      otpType={entry.otp_type}
                      period={entry.period}
                      variant="list"
                    />
                  ) : null}
                </div>
              );
            })}

            {entries.length === 0 ? (
              <div className="py-6 text-center text-xs text-muted-foreground">
                {t("otpManager.noEntries")}
              </div>
            ) : null}
          </div>
        </div>
      </div>

      <OtpEditorDialog
        open={editingId !== null}
        entry={editEntry}
        isEditing={!isNew}
        onCancel={resetEdit}
        onChange={handleChange}
        onSave={handleSave}
        saveDisabled={secretLoading || !editEntry.issuer?.trim() || (isNew && !editEntry.secret)}
        secretLoading={secretLoading}
      />

      <OtpDeleteDialog
        entry={deletingEntry}
        onOpenChange={(open) => !open && setDeletingEntry(null)}
        onCancel={() => setDeletingEntry(null)}
        onConfirm={handleDeleteConfirm}
      />
    </div>
  );
}
