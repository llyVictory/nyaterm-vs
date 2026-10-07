import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
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

export function BrowserBackupDialog({
  mode,
  busy,
  error,
  onClose,
  onSubmit,
}: {
  mode: "import" | "export";
  busy: boolean;
  error: string;
  onClose: () => void;
  onSubmit: (password: string) => Promise<void>;
}) {
  const { t } = useTranslation();
  const [password, setPassword] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const [overwrite, setOverwrite] = useState(false);
  const importing = mode === "import";
  const valid =
    !!password &&
    new TextEncoder().encode(password).length <= 1024 &&
    (importing ? overwrite : password === confirmation);
  return (
    <Dialog
      open
      disablePointerDismissal
      onOpenChange={(open) => {
        if (!open && !busy) onClose();
      }}
    >
      <DialogContent className="sm:max-w-[400px]" showCloseButton={!busy}>
        <DialogHeader>
          <DialogTitle>
            {t(importing ? "web.backupImportTitle" : "web.backupExportTitle")}
          </DialogTitle>
          <DialogDescription>
            {t(importing ? "web.backupImportDescription" : "web.backupExportDescription")}
          </DialogDescription>
        </DialogHeader>
        <form
          className="space-y-4"
          onSubmit={(event) => {
            event.preventDefault();
            if (valid && !busy) void onSubmit(password);
          }}
        >
          <div className="space-y-2">
            <Label htmlFor="backup-password">{t("web.backupPassword")}</Label>
            <Input
              id="backup-password"
              type="password"
              autoFocus
              autoComplete={importing ? "off" : "new-password"}
              value={password}
              disabled={busy}
              onChange={(event) => setPassword(event.target.value)}
            />
          </div>
          {!importing && (
            <div className="space-y-2">
              <Label htmlFor="backup-confirmation">{t("web.backupPasswordConfirm")}</Label>
              <Input
                id="backup-confirmation"
                type="password"
                autoComplete="new-password"
                value={confirmation}
                disabled={busy}
                onChange={(event) => setConfirmation(event.target.value)}
              />
              {confirmation && confirmation !== password && (
                <p className="text-sm text-destructive">{t("web.backupPasswordMismatch")}</p>
              )}
            </div>
          )}
          {importing && (
            <div className="flex items-start gap-2">
              <Checkbox
                id="backup-overwrite"
                checked={overwrite}
                disabled={busy}
                onCheckedChange={(checked) => setOverwrite(checked === true)}
              />
              <Label htmlFor="backup-overwrite" className="text-sm leading-relaxed">
                {t("web.backupOverwriteConfirm")}
              </Label>
            </div>
          )}
          {error && (
            <p role="alert" className="text-sm text-destructive">
              {error}
            </p>
          )}
          <DialogFooter>
            <Button type="button" variant="outline" disabled={busy} onClick={onClose}>
              {t("common.cancel")}
            </Button>
            <Button
              type="submit"
              variant={importing ? "destructive" : "default"}
              disabled={!valid || busy}
            >
              {t(
                busy
                  ? "web.backupWorking"
                  : importing
                    ? "web.backupImportTitle"
                    : "web.backupExportTitle",
              )}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
