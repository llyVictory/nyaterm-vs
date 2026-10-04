import { runtime } from "@/lib/backend/runtime";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { ActionButton, ActionFooter } from "@/components/ui/action-footer";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { NumberInput } from "@/components/ui/number-input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import type { NetworkGroup, ProxyConfig } from "@/types/global";

interface ProxyForm {
  id: string;
  name: string;
  protocol: string;
  host: string;
  port: number;
  command: string;
  username: string;
  password: string;
  group_id: string;
}

const DEFAULT_FORM: ProxyForm = {
  id: "",
  name: "",
  protocol: "socks5",
  host: "127.0.0.1",
  port: 1080,
  command: "",
  username: "",
  password: "",
  group_id: "",
};

function toForm(proxy: ProxyConfig | null): ProxyForm {
  if (!proxy) return { ...DEFAULT_FORM };
  return {
    id: proxy.id,
    name: proxy.name,
    protocol: proxy.protocol,
    host: proxy.host,
    port: proxy.port,
    command: proxy.command ?? "",
    username: proxy.username ?? "",
    password: "",
    group_id: proxy.group_id ?? "",
  };
}

export function ProxyFormContent({
  proxy,
  saving,
  groups,
  externalError,
  saveDisabled,
  onCancel,
  onSave,
}: {
  proxy: ProxyConfig | null;
  saving: boolean;
  groups: NetworkGroup[];
  externalError?: string;
  saveDisabled?: boolean;
  onCancel: () => void;
  onSave: (proxy: ProxyConfig) => Promise<void>;
}) {
  const { t } = useTranslation();
  const editing = !!proxy;
  const [form, setForm] = useState<ProxyForm>(DEFAULT_FORM);
  const [error, setError] = useState("");

  useEffect(() => {
    setForm(toForm(proxy));
    setError("");
  }, [proxy]);

  const handleSubmit = async () => {
    if (runtime === "web" && form.protocol === "proxycommand") { setError(t("web.proxyCommandUnavailable")); return; }
    if (!form.name.trim()) {
      setError(t("network.proxyNameRequired"));
      return;
    }
    if (form.protocol === "proxycommand") {
      if (!form.command.trim()) {
        setError(t("network.proxyCommandRequired"));
        return;
      }
    } else {
      if (!form.host.trim()) {
        setError(t("network.proxyHostRequired"));
        return;
      }
      if (!form.port || form.port < 1 || form.port > 65535) {
        setError(t("network.proxyPortRequired"));
        return;
      }
    }

    setError("");
    await onSave({
      id: form.id,
      name: form.name.trim(),
      protocol: form.protocol,
      host: form.host.trim(),
      port: form.port,
      command: form.protocol === "proxycommand" ? form.command.trim() : undefined,
      username: form.username.trim() || undefined,
      password: form.password || undefined,
      group_id: form.group_id || undefined,
    });
  };

  const displayError = error || externalError || "";

  return (
    <>
      <div className="flex-1 min-h-0 overflow-y-auto p-4 pb-20 sm:p-5 sm:pb-20">
        <p className="mb-4 text-xs leading-5 text-muted-foreground">
          {t("network.proxyDialogDescription")}
        </p>

        <div className="space-y-4">
          <div className="grid gap-4 sm:grid-cols-[9rem_minmax(0,1fr)]">
            <div className="space-y-1.5">
              <Label className="text-sm">{t("settings.proxyProtocol")}</Label>
              <Select
                value={form.protocol}
                onValueChange={(value) => setForm((prev) => ({ ...prev, protocol: value }))}
              >
                <SelectTrigger className="h-9 text-sm">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="socks5">SOCKS5</SelectItem>
                  <SelectItem value="http">HTTP</SelectItem>
                  {runtime === "desktop" && <SelectItem value="proxycommand">ProxyCommand</SelectItem>}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-1.5">
              <Label className="text-sm">{t("network.proxyName")}</Label>
              <Input
                className="h-9 text-sm"
                placeholder={t("network.proxyNamePlaceholder")}
                value={form.name}
                onChange={(event) => setForm((prev) => ({ ...prev, name: event.target.value }))}
              />
            </div>
          </div>

          <div className="space-y-1.5">
            <Label className="text-sm">{t("network.group")}</Label>
            <Select
              value={form.group_id || "__ungrouped__"}
              onValueChange={(value) =>
                setForm((prev) => ({
                  ...prev,
                  group_id: value === "__ungrouped__" ? "" : value,
                }))
              }
            >
              <SelectTrigger className="h-9 text-sm">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="__ungrouped__">{t("network.ungrouped")}</SelectItem>
                {groups.map((group) => (
                  <SelectItem key={group.id} value={group.id}>
                    {group.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          {form.protocol === "proxycommand" ? (
            <div className="space-y-1.5">
              <Label className="text-sm">{t("network.proxyCommand")}</Label>
              <Textarea
                className="min-h-24 resize-y font-mono text-sm"
                placeholder={t("network.proxyCommandPlaceholder")}
                value={form.command}
                onChange={(event) => setForm((prev) => ({ ...prev, command: event.target.value }))}
              />
              <div className="text-xs text-muted-foreground">{t("network.proxyCommandHint")}</div>
            </div>
          ) : (
            <>
              <div className="grid gap-4 sm:grid-cols-[minmax(0,1fr)_9rem]">
                <div className="space-y-1.5">
                  <Label className="text-sm">{t("settings.proxyHost")}</Label>
                  <Input
                    className="h-9 text-sm"
                    placeholder="127.0.0.1"
                    value={form.host}
                    onChange={(event) => setForm((prev) => ({ ...prev, host: event.target.value }))}
                  />
                </div>
                <div className="space-y-1.5">
                  <Label className="text-sm">{t("settings.proxyPort")}</Label>
                  <NumberInput
                    className="h-9 text-sm [&_button]:h-9 [&_button]:w-9 [&_input]:h-9 [&_input]:text-sm"
                    min={1}
                    max={65535}
                    value={form.port}
                    onChange={(value) => setForm((prev) => ({ ...prev, port: value || 0 }))}
                  />
                </div>
              </div>

              <div className="grid gap-4 sm:grid-cols-2">
                <div className="space-y-1.5">
                  <Label className="text-sm">{t("network.proxyUsername")}</Label>
                  <Input
                    className="h-9 text-sm"
                    placeholder={t("network.proxyUsernamePlaceholder")}
                    value={form.username}
                    autoComplete="off"
                    onChange={(event) =>
                      setForm((prev) => ({ ...prev, username: event.target.value }))
                    }
                  />
                </div>
                <div className="space-y-1.5">
                  <Label className="text-sm">{t("network.proxyPassword")}</Label>
                  <Input
                    className="h-9 text-sm"
                    type="password"
                    placeholder={
                      editing
                        ? t("network.proxyPasswordKeep")
                        : t("network.proxyPasswordPlaceholder")
                    }
                    value={form.password}
                    autoComplete="off"
                    onChange={(event) =>
                      setForm((prev) => ({ ...prev, password: event.target.value }))
                    }
                  />
                </div>
              </div>
            </>
          )}

          {displayError ? (
            <div className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive">
              {displayError}
            </div>
          ) : null}
        </div>
      </div>

      <ActionFooter>
        <ActionButton variant="outline" onClick={onCancel} disabled={saving}>
          {t("common.cancel")}
        </ActionButton>
        <ActionButton onClick={handleSubmit} disabled={saving || saveDisabled}>
          {saving ? t("common.saving") : t("common.save")}
        </ActionButton>
      </ActionFooter>
    </>
  );
}
