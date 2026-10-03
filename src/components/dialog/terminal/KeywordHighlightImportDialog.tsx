import { open as openFileDialog } from "@/lib/backend/platform/dialog";
import { openUrl } from "@/lib/backend/platform/opener";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { MdDataObject, MdOpenInNew, MdTerminal } from "react-icons/md";
import { toast } from "sonner";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { invoke } from "@/lib/invoke";
import { logger } from "@/lib/logger";
import type {
  AppSettings,
  KeywordHighlightImportResult,
  KeywordHighlightRule,
} from "@/types/global";

interface KeywordHighlightImportDialogProps {
  open: boolean;
  onClose: () => void;
  onImportedRules?: (rules: KeywordHighlightRule[]) => void;
}

const KEYWORD_HIGHLIGHT_DOC_URLS = {
  zh: "https://nyaterm.app/docs/guide/terminal#导入自定义高亮规则",
  en: "https://nyaterm.app/docs/guide/terminal#import-custom-highlight-rules",
};

export function KeywordHighlightImportDialog({
  open,
  onClose,
  onImportedRules,
}: KeywordHighlightImportDialogProps) {
  const { i18n, t } = useTranslation();
  const [importing, setImporting] = useState(false);
  const docsUrl = i18n.language.toLowerCase().startsWith("zh")
    ? KEYWORD_HIGHLIGHT_DOC_URLS.zh
    : KEYWORD_HIGHLIGHT_DOC_URLS.en;

  async function handleSelectJson() {
    if (importing) return;

    setImporting(true);
    try {
      const selected = await openFileDialog({
        multiple: false,
        filters: [{ name: t("settings.keywordHighlightImportJson"), extensions: ["json"] }],
      });
      if (!selected || Array.isArray(selected)) return;

      onClose();
      const result = await invoke<KeywordHighlightImportResult>("import_keyword_highlight_rules", {
        filePath: selected,
      });
      const nextSettings = await invoke<AppSettings>("get_app_settings");
      onImportedRules?.(nextSettings.terminal.keyword_highlights ?? []);
      toast.success(
        t("settings.keywordHighlightImportSuccess", {
          imported: result.imported_rules,
          updated: result.updated_rules,
          total: result.total_rules,
        }),
      );
    } catch (error) {
      logger.error({
        domain: "settings.persistence",
        event: "keyword_highlights.import_failed",
        message: "Import keyword highlight rules failed",
        error,
      });
      toast.error(t("settings.keywordHighlightImportFailed", { error: String(error) }));
    } finally {
      setImporting(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={(value) => !value && !importing && onClose()}>
      <DialogContent className="w-[min(380px,calc(100vw-2rem))] sm:max-w-[380px] p-6">
        <DialogHeader>
          <DialogTitle className="text-sm">{t("settings.keywordHighlightImportTitle")}</DialogTitle>
          <DialogDescription className="text-xs">
            {t("settings.keywordHighlightImportDesc")}
          </DialogDescription>
        </DialogHeader>
        <div className="grid grid-cols-1 gap-3 pt-2">
          <button
            type="button"
            disabled={importing}
            className="flex min-h-32 cursor-pointer flex-col items-center justify-center gap-2 rounded-lg border p-3 text-center transition-colors hover:border-[var(--df-primary)] hover:bg-[color-mix(in_srgb,var(--df-primary)_8%,transparent)] disabled:cursor-default disabled:opacity-60"
            style={{ borderColor: "var(--df-border)" }}
            onClick={handleSelectJson}
          >
            <MdDataObject className="h-10 w-10 text-[var(--df-primary)]" />
            <span className="text-xs font-medium" style={{ color: "var(--df-text)" }}>
              {t("settings.keywordHighlightImportJson")}
            </span>
            <span
              className="text-[0.65rem] leading-tight"
              style={{ color: "var(--df-text-dimmed)" }}
            >
              {importing
                ? t("settings.keywordHighlightImporting")
                : t("settings.keywordHighlightImportJsonHint")}
            </span>
          </button>
        </div>
        <div
          className="flex items-center justify-between gap-3 pt-1 text-[0.6875rem]"
          style={{ color: "var(--df-text-dimmed)" }}
        >
          <div className="flex min-w-0 flex-1 items-center gap-1.5">
            <MdTerminal className="shrink-0 text-[0.85rem]" />
            <span className="leading-tight">{t("settings.keywordHighlightImportMergeHint")}</span>
          </div>
          <button
            type="button"
            className="inline-flex shrink-0 items-center gap-1 rounded px-1.5 py-1 text-[0.6875rem] transition-colors hover:bg-[var(--df-bg-hover)]"
            style={{ color: "var(--df-primary)" }}
            onClick={() => void openUrl(encodeURI(docsUrl))}
          >
            {t("settings.keywordHighlightImportDocs")}
            <MdOpenInNew className="text-[0.75rem]" />
          </button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
