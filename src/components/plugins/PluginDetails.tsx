import { openUrl } from "@/lib/backend/platform/opener";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { getErrorMessage } from "@/lib/errors";
import type { MarketplacePlugin, MarketplacePreview } from "@/types/plugins";

export function PluginDetails({
  plugin,
  preview,
  busy,
  locked,
  onCancel,
  onInstall,
}: {
  plugin: MarketplacePlugin;
  preview: MarketplacePreview;
  busy: boolean;
  locked: boolean;
  onCancel: () => void;
  onInstall: () => void;
}) {
  const { t } = useTranslation();
  const { manifest, signature } = preview.package;
  const version = plugin.versions.find((v) => v.version === manifest.version);
  return (
    <Dialog open onOpenChange={(open) => !open && !busy && onCancel()}>
      <DialogContent className="max-h-[85vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>
            {manifest.name} · {manifest.version}
          </DialogTitle>
          <DialogDescription>{t("plugins.reviewDescription")}</DialogDescription>
        </DialogHeader>
        <div className="space-y-3 text-sm">
          <p>{manifest.description}</p>
          <p>
            {t("plugins.publisher", { name: manifest.publisher })}{" "}
            {plugin.verified && (
              <Badge variant="secondary">{t("plugins.store.publisherVerified")}</Badge>
            )}
          </p>
          <p>
            {t("plugins.store.repository")}: {t("plugins.store.official")}
          </p>
          <Badge variant="outline">{t(`plugins.store.${signature.status}`)}</Badge>
          <Button
            variant="link"
            className="h-auto max-w-full whitespace-normal break-all p-0 text-left"
            onClick={() =>
              void openUrl(plugin.source).catch((error) => toast.error(getErrorMessage(error)))
            }
          >
            {t("plugins.store.source")}: {plugin.source}
          </Button>
          <p className="text-muted-foreground">{plugin.license}</p>
          <div className="space-y-1">
            <p className="font-medium">{t("plugins.permissionsTitle")}</p>
            {manifest.permissions.map((permission) => (
              <p key={permission}>
                {permission.startsWith("network:")
                  ? t("plugins.permissions.network", { origin: permission.slice(8) })
                  : t(`plugins.permissions.${permission.split(".").join("_")}`)}
              </p>
            ))}
          </div>
          {manifest.backend && (
            <p className="rounded-md border border-destructive/30 bg-destructive/5 p-3">
              {t("plugins.nativeTrust")}
            </p>
          )}
          {version?.releaseNotes && (
            <p className="whitespace-pre-wrap text-muted-foreground">{version.releaseNotes}</p>
          )}
        </div>
        <DialogFooter>
          <Button variant="outline" disabled={busy} onClick={onCancel}>
            {t("plugins.cancel")}
          </Button>
          <Button disabled={busy || locked || signature.status !== "verified"} onClick={onInstall}>
            {t("plugins.install")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
