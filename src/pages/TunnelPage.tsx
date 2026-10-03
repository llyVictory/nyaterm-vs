import { emit } from "@/lib/backend/api";
import { getCurrentWindow } from "@/lib/backend/platform/window";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import ChildWindowHeader from "@/components/layout/ChildWindowHeader";
import { buildGroupPath, type ConnectionOption, sortLabel } from "@/components/network/shared";
import { TunnelFormContent } from "@/components/network/TunnelFormContent";
import { getErrorMessage } from "@/lib/errors";
import { invoke } from "@/lib/invoke";
import type { Group, NetworkGroup, SavedConnection, TunnelConfig } from "@/types/global";

export default function TunnelPage() {
  const { t } = useTranslation();
  const params = new URLSearchParams(window.location.search);
  const editId = params.get("edit") ?? undefined;
  const [tunnel, setTunnel] = useState<TunnelConfig | null>(null);
  const [groups, setGroups] = useState<NetworkGroup[]>([]);
  const [savedGroups, setSavedGroups] = useState<Group[]>([]);
  const [savedConnections, setSavedConnections] = useState<SavedConnection[]>([]);
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
        const [nextGroups, connections, connectionGroups, tunnels] = await Promise.all([
          invoke<NetworkGroup[]>("get_tunnel_groups"),
          invoke<SavedConnection[]>("get_saved_connections"),
          invoke<Group[]>("get_groups"),
          editId ? invoke<TunnelConfig[]>("get_tunnels") : Promise.resolve([]),
        ]);

        if (cancelled) return;

        setGroups(nextGroups);
        setSavedConnections(connections);
        setSavedGroups(connectionGroups);

        if (editId) {
          const found = tunnels.find((item) => item.id === editId) ?? null;
          setTunnel(found);
          if (!found) {
            setFatalError(t("network.tunnelNotFound"));
          }
        } else {
          setTunnel(null);
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

  const groupsById = useMemo(
    () => new Map(savedGroups.map((group) => [group.id, group])),
    [savedGroups],
  );

  const connectionOptions = useMemo<ConnectionOption[]>(() => {
    return [...savedConnections]
      .filter((connection) => connection.type === "ssh")
      .map((connection) => {
        const groupPath = buildGroupPath(connection.group_id, groupsById);
        const subtitle = groupPath
          ? `${groupPath} · ${connection.host}:${connection.port}`
          : `${connection.host}:${connection.port}`;

        return {
          connection,
          groupPath,
          subtitle,
          searchText: [connection.name, connection.host, connection.username, groupPath]
            .filter(Boolean)
            .join(" "),
          disabled: false,
        };
      })
      .sort((left, right) => {
        const pathSort = sortLabel(left.groupPath, right.groupPath);
        return pathSort !== 0 ? pathSort : sortLabel(left.connection.name, right.connection.name);
      });
  }, [groupsById, savedConnections]);

  const handleSave = async (nextTunnel: TunnelConfig) => {
    setSaving(true);
    setSaveError("");

    try {
      const payload = nextTunnel.id ? nextTunnel : { ...nextTunnel, id: crypto.randomUUID() };
      await invoke("save_tunnel", { tunnel: payload });
      await emit("tunnel-saved");
      getCurrentWindow().close();
    } catch (error) {
      setSaveError(getErrorMessage(error));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden bg-background text-foreground">
      <ChildWindowHeader
        title={t(editId ? "network.editTunnel" : "network.newTunnel")}
        onClose={handleClose}
      />
      {loading ? (
        <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
          {t("common.loading")}
        </div>
      ) : (
        <TunnelFormContent
          tunnel={tunnel}
          connectionOptions={connectionOptions}
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
