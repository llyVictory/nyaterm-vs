import { emit } from "@/lib/backend/api";
import { getCurrentWindow } from "@/lib/backend/platform/window";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import ChildWindowHeader from "@/components/layout/ChildWindowHeader";
import { ProxyFormContent } from "@/components/network/ProxyFormContent";
import { getErrorMessage } from "@/lib/errors";
import { invoke } from "@/lib/invoke";
import type { NetworkGroup, ProxyConfig } from "@/types/global";

export default function ProxyPage() {
  const { t } = useTranslation();
  const params = new URLSearchParams(window.location.search);
  const editId = params.get("edit") ?? undefined;
  const [proxy, setProxy] = useState<ProxyConfig | null>(null);
  const [groups, setGroups] = useState<NetworkGroup[]>([]);
  const [saving, setSaving] = useState(false);
  const [loading, setLoading] = useState(true);
  const [fatalError, setFatalError] = useState("");
  const [saveError, setSaveError] = useState("");

  const handleClose = useCallback(() => {
    if (saving) return;
    getCurrentWindow().close();
  }, [saving]);

  useEffect(() => {
    let cancelled = false;

    async function load() {
      setLoading(true);
      setFatalError("");
      setSaveError("");

      try {
        const [nextGroups, proxies] = await Promise.all([
          invoke<NetworkGroup[]>("get_proxy_groups"),
          editId ? invoke<ProxyConfig[]>("get_proxies") : Promise.resolve([]),
        ]);

        if (cancelled) return;

        setGroups(nextGroups);
        if (editId) {
          const found = proxies.find((item) => item.id === editId) ?? null;
          setProxy(found);
          if (!found) {
            setFatalError(t("network.proxyNotFound"));
          }
        } else {
          setProxy(null);
        }
      } catch (loadError) {
        if (!cancelled) {
          setFatalError(getErrorMessage(loadError));
        }
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
      }
    }

    void load();

    return () => {
      cancelled = true;
    };
  }, [editId, t]);

  const handleSave = async (nextProxy: ProxyConfig) => {
    setSaving(true);
    setSaveError("");

    try {
      await invoke("save_proxy", { proxy: nextProxy });
      await emit("proxy-saved");
      getCurrentWindow().close();
    } catch (saveError) {
      setSaveError(getErrorMessage(saveError));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden bg-background text-foreground">
      <ChildWindowHeader
        title={t(editId ? "network.editProxy" : "network.newProxy")}
        onClose={handleClose}
      />
      {loading ? (
        <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
          {t("common.loading")}
        </div>
      ) : (
        <ProxyFormContent
          proxy={proxy}
          groups={groups}
          saving={saving}
          externalError={fatalError || saveError}
          saveDisabled={!!fatalError}
          onCancel={handleClose}
          onSave={handleSave}
        />
      )}
    </div>
  );
}
