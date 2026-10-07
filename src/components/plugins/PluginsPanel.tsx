import { supports } from "@/lib/backend/runtime";
import { open } from "@/lib/backend/platform/dialog";
import { Puzzle } from "lucide-react";
import { useEffect, useState } from "react";
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
import { Badge } from "@/components/ui/badge";
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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { usePlugins } from "@/context/PluginContext";
import { getErrorMessage } from "@/lib/errors";
import { activeManifest, openPlugin, pluginApi } from "@/lib/plugins";
import type { InstalledPlugin, PluginPackagePreview } from "@/types/plugins";
import { PluginDiagnostics } from "./PluginDiagnostics";
import { PluginMarketplace } from "./PluginMarketplace";

export function PluginsPanel({ sessionId }: { sessionId: string | null }) {
  const { t } = useTranslation();
  return (
    <Tabs
      defaultValue={supports("nativePlugins") ? "marketplace" : "installed"}
      className="h-full min-h-0 gap-0"
    >
      <TabsList className="m-3 shrink-0">
        <TabsTrigger value="marketplace" disabled={!supports("nativePlugins")}>
          {t("plugins.store.marketplace")}
        </TabsTrigger>
        <TabsTrigger value="installed">
          {t("plugins.store.installedTab")}
        </TabsTrigger>
      </TabsList>
      <TabsContent value="marketplace" className="min-h-0 overflow-auto">
        <PluginMarketplace />
      </TabsContent>
      <TabsContent value="installed" className="min-h-0 overflow-auto">
        <InstalledPluginsPanel sessionId={sessionId} />
      </TabsContent>
    </Tabs>
  );
}

function InstalledPluginsPanel({ sessionId }: { sessionId: string | null }) {
  const { t } = useTranslation();
  const { plugins, loaded, error, refresh, locked } = usePlugins();
  const [busy, setBusy] = useState(false);
  const [preview, setPreview] = useState<{
    path: string;
    package: PluginPackagePreview;
  } | null>(null);
  const [grant, setGrant] = useState<{
    plugin: InstalledPlugin;
    permissions: string[];
  } | null>(null);
  const [probeScripts, setProbeScripts] = useState<Record<string, string> | null>(null);
  const [remove, setRemove] = useState<InstalledPlugin | null>(null);
  const action = async (operation: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await operation();
      await refresh();
    } catch (error) {
      toast.error(getErrorMessage(error));
    } finally {
      setBusy(false);
    }
  };
  const inspect = () =>
    void action(async () => {
      const path = await open({
        multiple: false,
        directory: false,
        filters: [{ name: "NyaTerm Plugin", extensions: ["nyap"] }],
      });
      if (typeof path === "string") setPreview({ path, package: await pluginApi.inspect(path) });
    });
  const permissionLabel = (permission: string) =>
    permission.startsWith("network:")
      ? t("plugins.permissions.network", { origin: permission.slice(8) })
      : t(`plugins.permissions.${permission.split(".").join("_")}`);
  const grantManifest = grant && activeManifest(grant.plugin);
  const grantPluginId = grant?.plugin.id;
  const grantVersion = grant?.plugin.activeVersion;
  useEffect(() => {
    setProbeScripts(null);
    if (!grantPluginId || !grantVersion) return;
    let disposed = false;
    void pluginApi
      .probeScripts(grantPluginId, grantVersion)
      .then((scripts) => {
        if (!disposed) setProbeScripts(scripts);
      })
      .catch((error) => {
        if (!disposed) toast.error(getErrorMessage(error));
      });
    return () => {
      disposed = true;
    };
  }, [grantPluginId, grantVersion]);

  return (
    <div className="h-full min-h-0 overflow-auto p-3 space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h3 className="flex items-center gap-2 text-sm font-semibold">
          <Puzzle size={16} />
          {t("plugins.title")}
        </h3>
        <Button
          size="sm"
          disabled={busy || locked || !supports("nativePlugins")}
          onClick={inspect}
        >
          {t("plugins.install")}
        </Button>
      </div>
      <p className="text-sm text-muted-foreground">{t("plugins.description")}</p>
      {error && (
        <div role="alert" className="space-y-2 text-sm text-destructive">
          <p>{error}</p>
          <Button variant="outline" onClick={() => void refresh()}>
            {t("plugins.retry")}
          </Button>
        </div>
      )}
      {!loaded && (
        <p className="text-sm text-muted-foreground">{t("plugins.loading")}</p>
      )}
      {loaded && !plugins.length && !error && (
        <p className="rounded-md border p-5 text-sm text-muted-foreground">
          {t("plugins.noInstalled")}
        </p>
      )}
      {plugins.map((plugin) => {
        const manifest = activeManifest(plugin);
        if (!manifest) return null;
        return (
          <div key={plugin.id} className="space-y-3 rounded-lg border bg-card p-4">
            <div className="flex flex-wrap items-center gap-2">
              <h3 className="font-semibold">{manifest.name}</h3>
              <Badge variant={plugin.enabled ? "default" : "secondary"}>
                {t(plugin.enabled ? "plugins.enabled" : "plugins.disabled")}
              </Badge>
              {manifest.backend && (
                <Badge variant="outline">{t("plugins.native")}</Badge>
              )}
              <Badge variant="outline">
                {t(
                  plugin.versions[plugin.activeVersion]?.provenance?.source === "marketplace"
                    ? "plugins.store.marketplace"
                    : "plugins.store.local",
                )}
              </Badge>
              <Badge variant="outline">
                {t(
                  `plugins.store.${plugin.versions[plugin.activeVersion]?.signature?.status ?? "unsigned"}`,
                )}
              </Badge>
            </div>
            <p className="break-words text-sm text-muted-foreground">{manifest.description}</p>
            <p className="break-all text-xs text-muted-foreground">
              {plugin.id} ·{" "}
              {t("plugins.publisher", { name: manifest.publisher })}
            </p>
            <div className="flex flex-wrap items-center gap-2">
              <Select
                value={plugin.activeVersion}
                disabled={busy || locked}
                onValueChange={(version) =>
                  void action(() => pluginApi.activate(plugin.id, version))
                }
              >
                <SelectTrigger className="w-36" aria-label={t("plugins.version")}>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {Object.keys(plugin.versions).map((version) => (
                    <SelectItem key={version} value={version}>
                      {version}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <Button
                variant="outline"
                disabled={busy || locked}
                onClick={() =>
                  plugin.enabled
                    ? void action(() =>
                        pluginApi.configure(
                          plugin.id,
                          plugin.activeVersion,
                          false,
                          plugin.grantedPermissions,
                        ),
                      )
                    : setGrant({
                        plugin,
                        permissions: [...plugin.grantedPermissions],
                      })
                }
              >
                {t(plugin.enabled ? "plugins.disable" : "plugins.enable")}
              </Button>
              <Button
                variant="ghost"
                disabled={busy || locked}
                onClick={() =>
                  setGrant({
                    plugin,
                    permissions: [...plugin.grantedPermissions],
                  })
                }
              >
                {t("plugins.permissionsTitle")}
              </Button>
              <Button
                variant="ghost"
                className="text-destructive"
                disabled={busy || locked}
                onClick={() => setRemove(plugin)}
              >
                {t("plugins.uninstall")}
              </Button>
            </div>
            <PluginDiagnostics
              pluginId={plugin.id}
              key={`${plugin.id}:${plugin.activeVersion}:${plugin.enabled}`}
              name={manifest.name}
              enabled={plugin.enabled}
              native={Boolean(manifest.backend)}
              locked={locked}
              disabled={busy}
            />
            {plugin.enabled &&
              (manifest.contributions.panels.length > 0 ||
                manifest.contributions.commands.some((command) => command.method)) && (
                <div className="space-y-2 border-t pt-3">
                  {manifest.contributions.panels.map((panel) => (
                    <Button
                      key={panel.id}
                      className="w-full justify-start whitespace-normal text-left"
                      variant="outline"
                      disabled={busy || locked}
                      onClick={() =>
                        openPlugin({
                          pluginId: plugin.id,
                          panelId: panel.id,
                          sessionId,
                        })
                      }
                    >
                      {panel.title}
                    </Button>
                  ))}
                  {manifest.contributions.commands
                    .filter((command) => command.method)
                    .map((command) => (
                      <Button
                        key={command.id}
                        className="w-full justify-start whitespace-normal text-left"
                        variant="outline"
                        disabled={busy || locked}
                        onClick={() =>
                          openPlugin({
                            pluginId: plugin.id,
                            commandId: command.id,
                            sessionId,
                          })
                        }
                      >
                        {command.title}
                      </Button>
                    ))}
                </div>
              )}
          </div>
        );
      })}
      <Dialog open={preview !== null} onOpenChange={(open) => !open && !busy && setPreview(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t("plugins.reviewPackage")}</DialogTitle>
            <DialogDescription>{t("plugins.reviewDescription")}</DialogDescription>
          </DialogHeader>
          {preview && (
            <div className="space-y-3 text-sm">
              <p className="font-medium">
                {preview.package.manifest.name}{" "}
                {preview.package.manifest.version}
              </p>
              <p>{preview.package.manifest.description}</p>
              <p className="break-all text-muted-foreground">
                {t("plugins.publisher", {
                  name: preview.package.manifest.publisher,
                })}
              </p>
              <p className="text-xs text-muted-foreground">{t("plugins.publisherUnverified")}</p>
              <Badge variant="outline">{t("plugins.store.local")}</Badge>
              <Badge variant="outline">
                {t(`plugins.store.${preview.package.signature.status}`)}
              </Badge>
              <div className="space-y-1">
                <p className="font-medium">{t("plugins.permissionsTitle")}</p>
                {preview.package.manifest.permissions.map((permission) => (
                  <p key={permission}>{permissionLabel(permission)}</p>
                ))}
              </div>
            </div>
          )}
          <DialogFooter>
            <Button variant="outline" disabled={busy} onClick={() => setPreview(null)}>
              {t("plugins.cancel")}
            </Button>
            <Button
              disabled={busy || locked}
              onClick={() =>
                preview &&
                void action(async () => {
                  await pluginApi.install(preview.path, preview.package.digest);
                  setPreview(null);
                  toast.success(t("plugins.installed"));
                })
              }
            >
              {t("plugins.install")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      <Dialog open={grant !== null} onOpenChange={(open) => !open && !busy && setGrant(null)}>
        <DialogContent className="max-h-[85vh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle>{t("plugins.permissionsTitle")}</DialogTitle>
            <DialogDescription>
              {t("plugins.permissionsDescription", {
                name: grantManifest?.name,
              })}
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-3">
            {grantManifest?.permissions.map((permission) => (
              <label key={permission} className="flex items-start gap-3 text-sm">
                <Checkbox
                  checked={grant?.permissions.includes(permission)}
                  disabled={busy}
                  onCheckedChange={(checked) =>
                    setGrant(
                      (current) =>
                        current && {
                          ...current,
                          permissions:
                            checked === true
                              ? [...new Set([...current.permissions, permission])]
                              : current.permissions.filter((value) => value !== permission),
                        },
                    )
                  }
                />
                <span>{permissionLabel(permission)}</span>
              </label>
            ))}
            {Boolean(grantManifest?.contributions.probes?.length) && (
              <div className="space-y-2">
                <p className="text-sm">{t("plugins.probeTrust")}</p>
                {!probeScripts && (
                  <p className="text-sm">{t("plugins.loading")}</p>
                )}
                {grantManifest?.contributions.probes?.map((probe) => (
                  <details key={probe.id} className="rounded-md border p-2" open>
                    <summary className="text-sm font-medium">
                      {probe.title} · {probe.entry}
                    </summary>
                    <pre className="mt-2 max-h-48 overflow-auto whitespace-pre-wrap break-all text-xs">
                      {probeScripts?.[probe.id] ?? ""}
                    </pre>
                  </details>
                ))}
              </div>
            )}
            {grantManifest?.backend && (
              <p className="rounded-md border border-destructive/30 bg-destructive/5 p-3 text-sm">
                {t("plugins.nativeTrust")}
              </p>
            )}
          </div>
          <DialogFooter>
            <Button variant="outline" disabled={busy} onClick={() => setGrant(null)}>
              {t("plugins.cancel")}
            </Button>
            <Button
              disabled={
                busy ||
                locked ||
                Boolean(grantManifest?.backend && !grant?.permissions.includes("native")) ||
                Boolean(grant?.permissions.includes("remote.probe") && !probeScripts)
              }
              onClick={() =>
                grant &&
                void action(async () => {
                  await pluginApi.configure(
                    grant.plugin.id,
                    grant.plugin.activeVersion,
                    true,
                    grant.permissions,
                  );
                  setGrant(null);
                })
              }
            >
              {t("plugins.saveAndEnable")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      <AlertDialog open={remove !== null} onOpenChange={(open) => !open && setRemove(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("plugins.uninstallTitle")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("plugins.uninstallDescription", {
                name: remove && activeManifest(remove)?.name,
              })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("plugins.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              onClick={() =>
                remove &&
                void action(async () => {
                  await pluginApi.uninstall(remove.id);
                  setRemove(null);
                })
              }
            >
              {t("plugins.uninstall")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
