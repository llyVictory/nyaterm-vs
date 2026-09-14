import {
  ChevronDown,
  ChevronRight,
  EllipsisVertical,
  Loader2,
  RefreshCw,
  Search,
  Trash2,
} from "lucide-react";
import type { ReactNode } from "react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { SiDocker } from "react-icons/si";
import { toast } from "sonner";
import DockerConfirmDialog, {
  type DockerPendingAction,
} from "@/components/dialog/docker/DockerConfirmDialog";
import DockerContainerDetailsDialog from "@/components/dialog/docker/DockerContainerDetailsDialog";
import PanelHeader from "@/components/layout/PanelHeader";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useApp } from "@/context/AppContext";
import { useVirtualList } from "@/hooks/useVirtualList";
import { canStartDockerContainer, canStopDockerContainer } from "@/lib/dockerContainerActions";
import { getErrorMessage } from "@/lib/errors";
import { invoke } from "@/lib/invoke";
import { buildTerminalCommandInput, sendSessionInput } from "@/lib/sessionInput";
import { getTerminalContextProvider } from "@/lib/terminalContext";
import { cn } from "@/lib/utils";
import type {
  DockerComposeProject,
  DockerComposeService,
  DockerContainer,
  DockerImage,
  DockerNetwork,
  DockerVolume,
  RemoteDockerOverview,
} from "@/types/global";

type DockerTab = "containers" | "images" | "volumes" | "networks" | "compose";
type DockerResourceTab = Exclude<DockerTab, "containers">;

const MAX_CONSECUTIVE_FAILURES = 3;
const CONTAINER_ROW_HEIGHT = 66;
const SIMPLE_ROW_HEIGHT = 64;
const COMPOSE_ROW_HEIGHT = 74;
const COMPOSE_SERVICE_ROW_HEIGHT = 58;
const TAB_GAP_PX = 4;
const TAB_LIST_PADDING_PX = 8;
const MIN_TAB_WIDTH_PX = 62;

type DockerStateKind = "danger" | "running" | "stopped" | "transition" | "unknown";

const EMPTY_DOCKER_OVERVIEW: RemoteDockerOverview = {
  available: false,
  version: "",
  compose_available: false,
  containers: [],
  images: [],
  volumes: [],
  networks: [],
  compose_projects: [],
};

interface DockerManagerProps {
  activeSessionId: string | null;
}

interface DockerTabItem {
  value: DockerTab;
  label: string;
  count: number | null;
}

interface ComposeServicesState {
  error: string | null;
  loading: boolean;
  services: DockerComposeService[] | null;
}

interface PreparedDockerTerminalCommand {
  command: string;
  stdin?: string | null;
}

export default function DockerManager({ activeSessionId }: DockerManagerProps) {
  const { t } = useTranslation();
  const { appSettings } = useApp();
  const [overview, setOverview] = useState<RemoteDockerOverview | null>(null);
  const [error, setError] = useState(false);
  const [query, setQuery] = useState("");
  const [tab, setTab] = useState<DockerTab>("containers");
  const [selectedContainer, setSelectedContainer] = useState<DockerContainer | null>(null);
  const [pendingAction, setPendingAction] = useState<DockerPendingAction | null>(null);
  const [isManualRefreshing, setIsManualRefreshing] = useState(false);
  const [loadedTabs, setLoadedTabs] = useState<Set<DockerResourceTab>>(() => new Set());
  const [loadingTabs, setLoadingTabs] = useState<Set<DockerResourceTab>>(() => new Set());
  const [failedTabs, setFailedTabs] = useState<Set<DockerResourceTab>>(() => new Set());
  const [pendingOperations, setPendingOperations] = useState<Set<string>>(() => new Set());
  const [expandedComposeProjects, setExpandedComposeProjects] = useState<Set<string>>(
    () => new Set(),
  );
  const [composeServicesByProject, setComposeServicesByProject] = useState<
    Record<string, ComposeServicesState>
  >({});
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const fetchingRef = useRef(false);
  const failCountRef = useRef(0);

  const enabled = appSettings.ui.show_docker_manager ?? false;
  const pollIntervalMs = Math.max(3, appSettings.ui.docker_manager_interval ?? 10) * 1000;

  const fetchOverview = useCallback(async (sessionId: string, manual = false) => {
    if (fetchingRef.current) return;
    fetchingRef.current = true;
    if (manual) setIsManualRefreshing(true);

    try {
      const data = await invoke<RemoteDockerOverview>("get_remote_docker_overview", { sessionId });
      setOverview((current) => ({
        ...data,
        images: current?.images ?? [],
        volumes: current?.volumes ?? [],
        networks: current?.networks ?? [],
        compose_projects: data.compose_available ? (current?.compose_projects ?? []) : [],
      }));
      if (!data.compose_available) {
        setLoadedTabs((current) => {
          const next = new Set(current);
          next.delete("compose");
          return next;
        });
        setComposeServicesByProject({});
        setExpandedComposeProjects(new Set());
      }
      setError(false);
      failCountRef.current = 0;
    } catch {
      failCountRef.current += 1;
      setError(true);
      if (failCountRef.current >= MAX_CONSECUTIVE_FAILURES) {
        setOverview(null);
      }
    } finally {
      fetchingRef.current = false;
      if (manual) setIsManualRefreshing(false);
    }
  }, []);

  const fetchResourceTab = useCallback(
    async (sessionId: string, resourceTab: DockerResourceTab, manual = false) => {
      if (loadingTabs.has(resourceTab)) return;

      setLoadingTabs((current) => new Set(current).add(resourceTab));
      try {
        switch (resourceTab) {
          case "images": {
            const images = await invoke<DockerImage[]>("get_remote_docker_images", { sessionId });
            setOverview((current) => (current ? { ...current, images } : current));
            break;
          }
          case "volumes": {
            const volumes = await invoke<DockerVolume[]>("get_remote_docker_volumes", {
              sessionId,
            });
            setOverview((current) => (current ? { ...current, volumes } : current));
            break;
          }
          case "networks": {
            const networks = await invoke<DockerNetwork[]>("get_remote_docker_networks", {
              sessionId,
            });
            setOverview((current) => (current ? { ...current, networks } : current));
            break;
          }
          case "compose": {
            const composeProjects = await invoke<DockerComposeProject[]>(
              "get_remote_docker_compose_projects",
              { sessionId },
            );
            setOverview((current) =>
              current ? { ...current, compose_projects: composeProjects } : current,
            );
            break;
          }
        }
        setLoadedTabs((current) => new Set(current).add(resourceTab));
        setFailedTabs((current) => {
          const next = new Set(current);
          next.delete(resourceTab);
          return next;
        });
      } catch (error) {
        setFailedTabs((current) => new Set(current).add(resourceTab));
        if (manual) toast.error(getErrorMessage(error));
      } finally {
        setLoadingTabs((current) => {
          const next = new Set(current);
          next.delete(resourceTab);
          return next;
        });
      }
    },
    [loadingTabs],
  );
  const fetchResourceTabRef = useRef(fetchResourceTab);

  useEffect(() => {
    fetchResourceTabRef.current = fetchResourceTab;
  }, [fetchResourceTab]);

  const tabRef = useRef(tab);

  useEffect(() => {
    tabRef.current = tab;
  }, [tab]);

  const refresh = useCallback(() => {
    if (!enabled || !activeSessionId) return;
    void fetchOverview(activeSessionId, true);
    if (isResourceTab(tab)) {
      void fetchResourceTab(activeSessionId, tab, true);
    }
  }, [activeSessionId, enabled, fetchOverview, fetchResourceTab, tab]);

  useEffect(() => {
    if (pollRef.current) {
      clearInterval(pollRef.current);
      pollRef.current = null;
    }

    if (!enabled || !activeSessionId) {
      setOverview(null);
      setLoadedTabs(new Set());
      setLoadingTabs(new Set());
      setFailedTabs(new Set());
      setComposeServicesByProject({});
      setExpandedComposeProjects(new Set());
      setError(false);
      failCountRef.current = 0;
      return;
    }

    fetchOverview(activeSessionId);
    setLoadedTabs(new Set());
    setLoadingTabs(new Set());
    setFailedTabs(new Set());
    setComposeServicesByProject({});
    setExpandedComposeProjects(new Set());
    pollRef.current = setInterval(() => {
      void fetchOverview(activeSessionId);
      const currentTab = tabRef.current;
      if (isResourceTab(currentTab)) {
        void fetchResourceTabRef.current(activeSessionId, currentTab);
      }
    }, pollIntervalMs);

    return () => {
      if (pollRef.current) clearInterval(pollRef.current);
    };
  }, [enabled, activeSessionId, pollIntervalMs, fetchOverview]);

  useEffect(() => {
    if (!enabled || !activeSessionId || !overview?.available || !isResourceTab(tab)) return;
    if (tab === "compose" && !overview.compose_available) return;
    if (!loadedTabs.has(tab) && !loadingTabs.has(tab) && !failedTabs.has(tab)) {
      void fetchResourceTab(activeSessionId, tab);
    }
  }, [
    activeSessionId,
    enabled,
    failedTabs,
    fetchResourceTab,
    loadedTabs,
    loadingTabs,
    overview,
    tab,
  ]);

  useEffect(() => {
    if (tab === "compose" && overview && !overview.compose_available) {
      setTab("containers");
    }
  }, [overview, tab]);

  const normalizedQuery = query.trim().toLowerCase();
  const filtered = useMemo(
    () => filterOverview(overview, normalizedQuery),
    [normalizedQuery, overview],
  );
  const summary = useMemo(() => getOverviewSummary(overview), [overview]);
  const tabItems = useMemo(() => {
    if (!overview) return [];
    const items: DockerTabItem[] = [
      {
        value: "containers",
        label: t("dockerManager.containers"),
        count: overview.containers.length,
      },
      {
        value: "images",
        label: t("dockerManager.images"),
        count: loadedTabs.has("images") ? overview.images.length : null,
      },
      {
        value: "volumes",
        label: t("dockerManager.volumes"),
        count: loadedTabs.has("volumes") ? overview.volumes.length : null,
      },
      {
        value: "networks",
        label: t("dockerManager.networks"),
        count: loadedTabs.has("networks") ? overview.networks.length : null,
      },
    ];
    if (overview.compose_available) {
      items.push({
        value: "compose",
        label: t("dockerManager.compose"),
        count: loadedTabs.has("compose") ? overview.compose_projects.length : null,
      });
    }
    return items;
  }, [loadedTabs, overview, t]);

  const runOperation = useCallback(
    async (
      operationKey: string,
      operation: () => Promise<unknown>,
      successKey = "dockerManager.actionSuccess",
      refreshAfterSuccess?: () => void | Promise<void>,
    ) => {
      if (!activeSessionId) return;
      if (pendingOperations.has(operationKey)) return;

      setPendingOperations((current) => new Set(current).add(operationKey));
      const toastId = toast.loading(t("dockerManager.actionRunning"));
      try {
        await operation();
        toast.success(t(successKey), { id: toastId });
        if (refreshAfterSuccess) {
          await refreshAfterSuccess();
        } else {
          await fetchOverview(activeSessionId, true);
        }
      } catch (error) {
        toast.error(getErrorMessage(error), { id: toastId });
      } finally {
        setPendingOperations((current) => {
          const next = new Set(current);
          next.delete(operationKey);
          return next;
        });
      }
    },
    [activeSessionId, fetchOverview, pendingOperations, t],
  );

  const invokeContainerAction = useCallback(
    (container: DockerContainer, action: string) =>
      runOperation(dockerOperationKey("container", container.id, action), () =>
        invoke("docker_container_action", {
          sessionId: activeSessionId,
          containerId: container.id,
          action,
        }),
      ),
    [activeSessionId, runOperation],
  );

  const executeTerminalCommand = useCallback(
    async (prepared: PreparedDockerTerminalCommand) => {
      if (!activeSessionId) return;
      try {
        const { command, stdin } = prepared;
        const provider = getTerminalContextProvider(activeSessionId);
        provider?.focus();
        if (provider?.executeCommand) {
          await provider.executeCommand(command);
        } else {
          await sendSessionInput(activeSessionId, buildTerminalCommandInput(command), {
            preview: { kind: "reset" },
            registerSubmission: command,
          });
        }
        if (stdin) {
          await sendSessionInput(activeSessionId, stdin, {
            preview: null,
            registerSubmission: null,
          });
        }
        toast.success(t("dockerManager.commandSent"));
      } catch (error) {
        toast.error(getErrorMessage(error));
      }
    },
    [activeSessionId, t],
  );

  const sendContainerLogs = useCallback(
    async (container: DockerContainer) => {
      if (!activeSessionId) return;
      try {
        const prepared = await invoke<PreparedDockerTerminalCommand>(
          "prepare_docker_container_logs_command",
          { sessionId: activeSessionId, containerId: container.id, tail: 100 },
        );
        await executeTerminalCommand(prepared);
      } catch (error) {
        toast.error(getErrorMessage(error));
      }
    },
    [activeSessionId, executeTerminalCommand],
  );

  const enterContainer = useCallback(
    async (containerId: string) => {
      if (!activeSessionId) return;
      try {
        const prepared = await invoke<PreparedDockerTerminalCommand>(
          "prepare_docker_container_shell_command",
          { sessionId: activeSessionId, containerId },
        );
        await executeTerminalCommand(prepared);
      } catch (error) {
        toast.error(getErrorMessage(error));
      }
    },
    [activeSessionId, executeTerminalCommand],
  );

  const loadComposeServices = useCallback(
    async (project: DockerComposeProject) => {
      if (!activeSessionId) return;
      const key = composeProjectKey(project);
      setComposeServicesByProject((current) => ({
        ...current,
        [key]: { error: null, loading: true, services: current[key]?.services ?? null },
      }));

      try {
        const services = await invoke<DockerComposeService[]>("get_docker_compose_services", {
          sessionId: activeSessionId,
          projectName: project.name,
          configFiles: project.config_files,
        });
        setComposeServicesByProject((current) => ({
          ...current,
          [key]: { error: null, loading: false, services },
        }));
      } catch (error) {
        const message = getErrorMessage(error);
        setComposeServicesByProject((current) => ({
          ...current,
          [key]: { error: message, loading: false, services: null },
        }));
        toast.error(message);
      }
    },
    [activeSessionId],
  );

  const toggleComposeProject = useCallback(
    (project: DockerComposeProject) => {
      const key = composeProjectKey(project);
      const willExpand = !expandedComposeProjects.has(key);
      setExpandedComposeProjects((current) => {
        const next = new Set(current);
        if (next.has(key)) next.delete(key);
        else next.add(key);
        return next;
      });

      if (willExpand && !composeServicesByProject[key]) {
        void loadComposeServices(project);
      }
    },
    [composeServicesByProject, expandedComposeProjects, loadComposeServices],
  );

  const invokeComposeServiceAction = useCallback(
    (project: DockerComposeProject, service: DockerComposeService, action: string) =>
      runOperation(
        dockerOperationKey("compose-service", composeProjectKey(project), service.name, action),
        () =>
          invoke("docker_compose_service_action", {
            sessionId: activeSessionId,
            projectName: project.name,
            configFiles: project.config_files,
            serviceName: service.name,
            action,
          }),
        "dockerManager.actionSuccess",
        async () => {
          if (!activeSessionId) return;
          await fetchResourceTab(activeSessionId, "compose", true);
          await loadComposeServices(project);
        },
      ),
    [activeSessionId, fetchResourceTab, loadComposeServices, runOperation],
  );

  const sendComposeServiceLogs = useCallback(
    async (project: DockerComposeProject, service: DockerComposeService) => {
      if (!activeSessionId) return;
      try {
        const prepared = await invoke<PreparedDockerTerminalCommand>(
          "prepare_docker_compose_service_logs_command",
          {
            sessionId: activeSessionId,
            projectName: project.name,
            configFiles: project.config_files,
            serviceName: service.name,
            tail: 100,
          },
        );
        await executeTerminalCommand(prepared);
      } catch (error) {
        toast.error(getErrorMessage(error));
      }
    },
    [activeSessionId, executeTerminalCommand],
  );

  const enterComposeService = useCallback(
    (service: DockerComposeService) => {
      const container = getFirstRunningComposeContainer(service);
      if (container) void enterContainer(container.id);
    },
    [enterContainer],
  );

  const confirmDanger = useCallback((action: DockerPendingAction | null) => {
    setPendingAction(action);
  }, []);

  return (
    <div
      className="nyaterm-wallpaper-transparent-surface h-full flex flex-col"
      style={{ backgroundColor: "var(--df-bg-panel)" }}
    >
      <PanelHeader
        title={t("panel.dockerManager")}
        meta={
          overview?.available ? (
            <span className="font-mono">
              {t("dockerManager.engine")} {overview.version || "-"}
            </span>
          ) : null
        }
        actions={
          <div className="flex items-center gap-1">
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-7 w-7 rounded-md text-muted-foreground hover:text-foreground disabled:opacity-40"
                  onClick={refresh}
                  disabled={!enabled || !activeSessionId || isManualRefreshing}
                  aria-label={t("common.refresh")}
                >
                  <RefreshCw className={cn("h-4 w-4", isManualRefreshing && "animate-spin")} />
                </Button>
              </TooltipTrigger>
              <TooltipContent side="top">{t("common.refresh")}</TooltipContent>
            </Tooltip>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-7 w-7 rounded-md text-muted-foreground hover:text-foreground disabled:opacity-40"
                  disabled={!enabled || !activeSessionId || !overview?.available}
                  aria-label={t("dockerManager.moreActions")}
                >
                  <EllipsisVertical className="h-4 w-4" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuItem
                  variant="destructive"
                  onSelect={() =>
                    confirmDanger({
                      title: t("dockerManager.pruneTitle"),
                      description: t("dockerManager.pruneDesc"),
                      command: "docker system prune -f --volumes",
                      run: () =>
                        runOperation(dockerOperationKey("system", "prune"), () =>
                          invoke("docker_system_prune", {
                            sessionId: activeSessionId,
                            volumes: true,
                          }),
                        ),
                    })
                  }
                >
                  <Trash2 className="h-3.5 w-3.5" />
                  {t("dockerManager.prune")}
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        }
      />

      <div className="flex-1 min-h-0 p-2.5">
        {!activeSessionId ? (
          <EmptyState icon={<SiDocker />} text={t("dockerManager.noSession")} />
        ) : !enabled ? (
          <EmptyState icon={<SiDocker />} text={t("dockerManager.disabled")} />
        ) : error && !overview ? (
          <EmptyState icon={<SiDocker />} text={t("dockerManager.error")} />
        ) : overview && !overview.available ? (
          <EmptyState icon={<SiDocker />} text={t("dockerManager.unavailable")} />
        ) : overview ? (
          <div className="flex h-full min-h-0 flex-col space-y-2.5">
            <OverviewStrip
              imageCount={loadedTabs.has("images") ? summary.images : null}
              runningCount={summary.running}
              stoppedCount={summary.stopped}
            />

            <div className="relative">
              <Search className="pointer-events-none absolute top-1/2 left-2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
              <Input
                className="h-8 pl-7 text-xs"
                value={query}
                placeholder={t("dockerManager.search")}
                onChange={(event) => setQuery(event.target.value)}
              />
            </div>

            <Tabs
              value={tab}
              onValueChange={(value) => setTab(value as DockerTab)}
              className="min-h-0 flex-1"
            >
              <AdaptiveDockerTabsList items={tabItems} activeTab={tab} onSelect={setTab} />

              <TabsContent value="containers" className="mt-2 min-h-0">
                <VirtualListPane
                  items={filtered.containers}
                  rowHeight={CONTAINER_ROW_HEIGHT}
                  renderRow={(container) => (
                    <ContainerRow
                      key={container.id}
                      container={container}
                      pendingAction={getPendingOperationAction(
                        pendingOperations,
                        dockerOperationKey("container", container.id),
                      )}
                      onDetails={() => setSelectedContainer(container)}
                      onEnter={() => void enterContainer(container.id)}
                      onLogs={() => void sendContainerLogs(container)}
                      onAction={(action) => {
                        if (action === "kill" || action === "remove") {
                          confirmDanger({
                            title: t("dockerManager.confirmActionTitle"),
                            description: t("dockerManager.confirmActionDesc", {
                              action,
                              target: container.name,
                            }),
                            command: `docker ${action === "remove" ? "rm" : action} ${container.id}`,
                            run: () => invokeContainerAction(container, action),
                          });
                        } else {
                          void invokeContainerAction(container, action);
                        }
                      }}
                    />
                  )}
                />
              </TabsContent>

              <TabsContent value="images" className="mt-2 min-h-0">
                <ResourceTabState
                  failed={failedTabs.has("images")}
                  loaded={loadedTabs.has("images")}
                  loading={loadingTabs.has("images")}
                  onRetry={() =>
                    activeSessionId && void fetchResourceTab(activeSessionId, "images", true)
                  }
                >
                  <VirtualListPane
                    items={filtered.images}
                    rowHeight={SIMPLE_ROW_HEIGHT}
                    renderRow={(image) => (
                      <DockerObjectRow
                        key={image.id}
                        title={`${image.repository}:${image.tag}`}
                        meta={image.created_since}
                        detail={image.size}
                        identifier={shortId(image.id)}
                        pending={pendingOperations.has(
                          dockerOperationKey("image", image.id, "remove"),
                        )}
                        onRemove={() =>
                          confirmDanger({
                            title: t("dockerManager.removeImage"),
                            description: t("dockerManager.confirmActionDesc", {
                              action: "remove",
                              target: `${image.repository}:${image.tag}`,
                            }),
                            command: `docker image rm ${image.id}`,
                            run: () =>
                              runOperation(
                                dockerOperationKey("image", image.id, "remove"),
                                () =>
                                  invoke("docker_image_remove", {
                                    sessionId: activeSessionId,
                                    imageId: image.id,
                                    force: false,
                                  }),
                                "dockerManager.actionSuccess",
                                () =>
                                  activeSessionId
                                    ? fetchResourceTab(activeSessionId, "images", true)
                                    : undefined,
                              ),
                          })
                        }
                      />
                    )}
                  />
                </ResourceTabState>
              </TabsContent>

              <TabsContent value="volumes" className="mt-2 min-h-0">
                <ResourceTabState
                  failed={failedTabs.has("volumes")}
                  loaded={loadedTabs.has("volumes")}
                  loading={loadingTabs.has("volumes")}
                  onRetry={() =>
                    activeSessionId && void fetchResourceTab(activeSessionId, "volumes", true)
                  }
                >
                  <VirtualListPane
                    items={filtered.volumes}
                    rowHeight={SIMPLE_ROW_HEIGHT}
                    renderRow={(volume) => (
                      <DockerObjectRow
                        key={volume.name}
                        title={volume.name}
                        meta={t("dockerManager.volumeDriver", { driver: volume.driver })}
                        detail={t("dockerManager.volume")}
                        pending={pendingOperations.has(
                          dockerOperationKey("volume", volume.name, "remove"),
                        )}
                        onRemove={() =>
                          confirmDanger({
                            title: t("dockerManager.removeVolume"),
                            description: t("dockerManager.confirmActionDesc", {
                              action: "remove",
                              target: volume.name,
                            }),
                            command: `docker volume rm ${volume.name}`,
                            run: () =>
                              runOperation(
                                dockerOperationKey("volume", volume.name, "remove"),
                                () =>
                                  invoke("docker_volume_remove", {
                                    sessionId: activeSessionId,
                                    volumeName: volume.name,
                                    force: false,
                                  }),
                                "dockerManager.actionSuccess",
                                () =>
                                  activeSessionId
                                    ? fetchResourceTab(activeSessionId, "volumes", true)
                                    : undefined,
                              ),
                          })
                        }
                      />
                    )}
                  />
                </ResourceTabState>
              </TabsContent>

              <TabsContent value="networks" className="mt-2 min-h-0">
                <ResourceTabState
                  failed={failedTabs.has("networks")}
                  loaded={loadedTabs.has("networks")}
                  loading={loadingTabs.has("networks")}
                  onRetry={() =>
                    activeSessionId && void fetchResourceTab(activeSessionId, "networks", true)
                  }
                >
                  <VirtualListPane
                    items={filtered.networks}
                    rowHeight={SIMPLE_ROW_HEIGHT}
                    renderRow={(network) => (
                      <DockerObjectRow
                        key={network.id}
                        title={network.name}
                        meta={`${network.driver} · ${network.scope}`}
                        detail={t("dockerManager.network")}
                        identifier={shortId(network.id)}
                        pending={pendingOperations.has(
                          dockerOperationKey("network", network.id, "remove"),
                        )}
                        onRemove={() =>
                          confirmDanger({
                            title: t("dockerManager.removeNetwork"),
                            description: t("dockerManager.confirmActionDesc", {
                              action: "remove",
                              target: network.name,
                            }),
                            command: `docker network rm ${network.id}`,
                            run: () =>
                              runOperation(
                                dockerOperationKey("network", network.id, "remove"),
                                () =>
                                  invoke("docker_network_remove", {
                                    sessionId: activeSessionId,
                                    networkId: network.id,
                                  }),
                                "dockerManager.actionSuccess",
                                () =>
                                  activeSessionId
                                    ? fetchResourceTab(activeSessionId, "networks", true)
                                    : undefined,
                              ),
                          })
                        }
                      />
                    )}
                  />
                </ResourceTabState>
              </TabsContent>

              {overview.compose_available ? (
                <TabsContent value="compose" className="mt-2 min-h-0">
                  <ResourceTabState
                    failed={failedTabs.has("compose")}
                    loaded={loadedTabs.has("compose")}
                    loading={loadingTabs.has("compose")}
                    onRetry={() =>
                      activeSessionId && void fetchResourceTab(activeSessionId, "compose", true)
                    }
                  >
                    <VirtualListPane
                      items={filtered.compose_projects}
                      rowHeight={COMPOSE_ROW_HEIGHT}
                      getRowHeight={(project) =>
                        getComposeProjectRowHeight(
                          project,
                          expandedComposeProjects,
                          composeServicesByProject,
                        )
                      }
                      renderRow={(project) => (
                        <ComposeRow
                          key={project.name}
                          project={project}
                          expanded={expandedComposeProjects.has(composeProjectKey(project))}
                          pendingAction={getPendingOperationAction(
                            pendingOperations,
                            dockerOperationKey("compose", composeProjectKey(project)),
                          )}
                          servicesState={composeServicesByProject[composeProjectKey(project)]}
                          getServicePendingAction={(service) =>
                            getPendingOperationAction(
                              pendingOperations,
                              dockerOperationKey(
                                "compose-service",
                                composeProjectKey(project),
                                service.name,
                              ),
                            )
                          }
                          onToggle={() => toggleComposeProject(project)}
                          onEnterService={enterComposeService}
                          onLogsService={(service) => void sendComposeServiceLogs(project, service)}
                          onRetryServices={() => void loadComposeServices(project)}
                          onServiceAction={(service, action) =>
                            void invokeComposeServiceAction(project, service, action)
                          }
                          onAction={(action) => {
                            const run = () =>
                              runOperation(
                                dockerOperationKey("compose", composeProjectKey(project), action),
                                () =>
                                  invoke("docker_compose_action", {
                                    sessionId: activeSessionId,
                                    projectName: project.name,
                                    configFiles: project.config_files,
                                    action,
                                  }),
                                "dockerManager.actionSuccess",
                                () =>
                                  activeSessionId
                                    ? fetchResourceTab(activeSessionId, "compose", true)
                                    : undefined,
                              );
                            if (action === "down") {
                              confirmDanger({
                                title: t("dockerManager.composeDown"),
                                description: t("dockerManager.confirmActionDesc", {
                                  action,
                                  target: project.name,
                                }),
                                command: `docker compose -p ${project.name} down`,
                                run,
                              });
                            } else {
                              void run();
                            }
                          }}
                        />
                      )}
                    />
                  </ResourceTabState>
                </TabsContent>
              ) : null}
            </Tabs>
          </div>
        ) : (
          <LoadingSpinner label={t("common.loading")} />
        )}
      </div>

      <DockerContainerDetailsDialog
        container={selectedContainer}
        pollIntervalMs={pollIntervalMs}
        sessionId={activeSessionId}
        onOpenChange={(open) => !open && setSelectedContainer(null)}
      />
      <DockerConfirmDialog
        action={pendingAction}
        onOpenChange={(open) => !open && setPendingAction(null)}
        onConfirm={() => {
          if (pendingAction) void pendingAction.run();
          setPendingAction(null);
        }}
      />
    </div>
  );
}

function filterOverview(
  overview: RemoteDockerOverview | null,
  query: string,
): RemoteDockerOverview {
  if (!overview || !query) {
    return overview
      ? { ...overview, containers: sortContainers(overview.containers) }
      : EMPTY_DOCKER_OVERVIEW;
  }

  const matches = (...values: string[]) => values.join(" ").toLowerCase().includes(query);
  return {
    ...overview,
    containers: sortContainers(
      overview.containers.filter((item) =>
        matches(item.id, item.name, item.image, item.status, item.ports),
      ),
    ),
    images: overview.images.filter((item) => matches(item.id, item.repository, item.tag)),
    volumes: overview.volumes.filter((item) => matches(item.driver, item.name)),
    networks: overview.networks.filter((item) =>
      matches(item.id, item.name, item.driver, item.scope),
    ),
    compose_projects: overview.compose_projects.filter((item) =>
      matches(item.name, item.status, item.config_files),
    ),
  };
}

function isResourceTab(tab: DockerTab): tab is DockerResourceTab {
  return tab !== "containers";
}

function dockerOperationKey(...parts: string[]) {
  return parts.join(":");
}

function getPendingOperationAction(pendingOperations: Set<string>, prefix: string) {
  for (const operation of pendingOperations) {
    if (operation === prefix || operation.startsWith(`${prefix}:`)) {
      return operation;
    }
  }
  return null;
}

function composeProjectKey(project: DockerComposeProject) {
  return `${project.name}\n${project.config_files}`;
}

function getComposeProjectRowHeight(
  project: DockerComposeProject,
  expandedProjects: Set<string>,
  servicesByProject: Record<string, ComposeServicesState>,
) {
  if (!expandedProjects.has(composeProjectKey(project))) {
    return COMPOSE_ROW_HEIGHT;
  }

  const state = servicesByProject[composeProjectKey(project)];
  const serviceCount = state?.services?.length ?? 0;
  if (state?.loading || state?.error || serviceCount === 0) {
    return COMPOSE_ROW_HEIGHT + 54;
  }

  return (
    COMPOSE_ROW_HEIGHT +
    12 +
    serviceCount * COMPOSE_SERVICE_ROW_HEIGHT +
    Math.max(0, serviceCount - 1) * 4
  );
}

function getFirstRunningComposeContainer(service: DockerComposeService) {
  const containers = [...service.containers]
    .filter((container) => container.state.toLowerCase() === "running")
    .sort((left, right) => left.name.localeCompare(right.name) || left.id.localeCompare(right.id));
  return containers[0];
}

function getOverviewSummary(overview: RemoteDockerOverview | null) {
  const containers = overview?.containers ?? [];
  const running = containers.filter(
    (container) => getDockerStateKind(container.state) === "running",
  ).length;
  return {
    images: overview?.images.length ?? 0,
    running,
    stopped: Math.max(0, containers.length - running),
  };
}

function sortContainers(containers: DockerContainer[]) {
  return [...containers].sort((left, right) => {
    const stateDelta = getDockerStateRank(left) - getDockerStateRank(right);
    if (stateDelta !== 0) return stateDelta;

    return left.name.localeCompare(right.name);
  });
}

function getDockerStateRank(container: DockerContainer) {
  switch (getDockerStateKind(container.state)) {
    case "danger":
      return 0;
    case "running":
      return 1;
    case "transition":
      return 2;
    case "stopped":
      return 3;
    default:
      return 4;
  }
}

function getDockerStateKind(state: string): DockerStateKind {
  const normalized = state.toLowerCase();
  if (normalized === "running") return "running";
  if (normalized === "exited" || normalized === "created") return "stopped";
  if (normalized === "paused" || normalized === "restarting" || normalized === "removing") {
    return "transition";
  }
  if (normalized === "dead") return "danger";
  return "unknown";
}

function getDockerStateLabelKey(state: string) {
  const normalized = state.toLowerCase();
  if (
    normalized === "created" ||
    normalized === "dead" ||
    normalized === "exited" ||
    normalized === "paused" ||
    normalized === "removing" ||
    normalized === "restarting" ||
    normalized === "running"
  ) {
    return normalized;
  }
  return "unknown";
}

function stateAccentClass(kind: DockerStateKind) {
  switch (kind) {
    case "danger":
      return "border-l-red-500";
    case "running":
      return "border-l-emerald-400";
    case "transition":
      return "border-l-amber-400";
    case "stopped":
      return "border-l-slate-500";
    default:
      return "border-l-zinc-500";
  }
}

function stateBadgeClass(kind: DockerStateKind) {
  switch (kind) {
    case "danger":
      return "border-red-500/35 bg-red-500/10 text-red-300";
    case "running":
      return "border-emerald-500/35 bg-emerald-500/10 text-emerald-300";
    case "transition":
      return "border-amber-500/35 bg-amber-500/10 text-amber-300";
    case "stopped":
      return "border-slate-500/35 bg-slate-500/10 text-slate-300";
    default:
      return "border-border bg-background text-muted-foreground";
  }
}

function OverviewStrip({
  imageCount,
  runningCount,
  stoppedCount,
}: {
  imageCount: number | null;
  runningCount: number;
  stoppedCount: number;
}) {
  const { t } = useTranslation();
  return (
    <div className="flex h-8 items-center justify-between gap-1 rounded-md border bg-muted/20 px-2">
      <OverviewStat label={t("dockerManager.running")} tone="running" value={runningCount} />
      <OverviewStat label={t("dockerManager.stopped")} tone="stopped" value={stoppedCount} />
      <OverviewStat label={t("dockerManager.images")} value={imageCount ?? "-"} />
    </div>
  );
}

function OverviewStat({
  label,
  tone,
  value,
}: {
  label: string;
  tone?: "running" | "stopped";
  value: ReactNode;
}) {
  return (
    <div
      className={cn(
        "flex min-w-0 items-center gap-1 text-[0.625rem]",
        tone === "running" ? "text-emerald-300" : tone === "stopped" ? "text-slate-300" : "",
      )}
      title={typeof value === "string" || typeof value === "number" ? String(value) : undefined}
    >
      <span className="shrink-0 text-muted-foreground">{label}</span>
      <span className="shrink-0 font-mono text-[0.6875rem] font-semibold">{value}</span>
    </div>
  );
}

function AdaptiveDockerTabsList({
  activeTab,
  items,
  onSelect,
}: {
  activeTab: DockerTab;
  items: DockerTabItem[];
  onSelect: (tab: DockerTab) => void;
}) {
  const { t } = useTranslation();
  const [visibleCount, setVisibleCount] = useState(items.length);
  const listRef = useRef<HTMLDivElement | null>(null);
  const itemMeasureRefs = useRef(new Map<DockerTab, HTMLDivElement>());
  const moreMeasureRef = useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    const element = listRef.current;
    if (!element || items.length === 0) return;

    const syncVisibleCount = () => {
      const availableWidth = element.clientWidth;
      const widths = items.map(
        (item) => itemMeasureRefs.current.get(item.value)?.offsetWidth ?? MIN_TAB_WIDTH_PX,
      );
      const moreWidth = moreMeasureRef.current?.offsetWidth ?? MIN_TAB_WIDTH_PX;

      for (let count = items.length; count >= 1; count -= 1) {
        const hiddenCount = items.length - count;
        const tabsWidth = widths.slice(0, count).reduce((total, width) => total + width, 0);
        const gaps = Math.max(0, count - 1) + (hiddenCount > 0 ? 1 : 0);
        const totalWidth =
          tabsWidth + (hiddenCount > 0 ? moreWidth : 0) + gaps * TAB_GAP_PX + TAB_LIST_PADDING_PX;

        if (totalWidth <= availableWidth) {
          setVisibleCount(count);
          return;
        }
      }

      setVisibleCount(1);
    };

    syncVisibleCount();

    if (typeof ResizeObserver === "undefined") {
      return;
    }

    const observer = new ResizeObserver(syncVisibleCount);
    observer.observe(element);
    return () => observer.disconnect();
  }, [items]);

  const safeVisibleCount = Math.min(Math.max(visibleCount, 1), items.length);
  const visibleItems = items.slice(0, safeVisibleCount);
  const hiddenItems = items.slice(safeVisibleCount);
  const moreActive = hiddenItems.some((item) => item.value === activeTab);

  return (
    <TabsList
      ref={listRef}
      className="relative flex h-8 w-full items-center gap-1 rounded-md bg-muted/25 p-1"
    >
      {visibleItems.map((item) => (
        <TabsTrigger
          key={item.value}
          value={item.value}
          className="min-w-0 flex-none px-2 text-[0.6875rem]"
        >
          <span className="min-w-0 truncate">{item.label}</span>
          <span className="shrink-0 font-mono text-[0.625rem] text-muted-foreground">
            {item.count ?? "-"}
          </span>
        </TabsTrigger>
      ))}
      {hiddenItems.length > 0 ? (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="xs"
              className={cn(
                "h-full shrink-0 px-2 text-[0.6875rem]",
                moreActive && "bg-background text-foreground shadow-sm",
              )}
            >
              {t("common.more")}
              <ChevronDown className="h-3 w-3" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            {hiddenItems.map((item) => (
              <DockerTabMenuItem
                key={item.value}
                count={item.count}
                label={item.label}
                onSelect={() => onSelect(item.value)}
              />
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      ) : null}
      <div
        className="pointer-events-none absolute -top-96 left-0 flex gap-1 opacity-0"
        aria-hidden="true"
      >
        {items.map((item) => (
          <div
            key={item.value}
            ref={(node) => {
              if (node) itemMeasureRefs.current.set(item.value, node);
              else itemMeasureRefs.current.delete(item.value);
            }}
            className="inline-flex h-7 items-center justify-center gap-1.5 whitespace-nowrap border border-transparent px-2 text-[0.6875rem] font-medium"
          >
            <span>{item.label}</span>
            <span className="font-mono text-[0.625rem]">{item.count ?? "-"}</span>
          </div>
        ))}
        <Button
          ref={moreMeasureRef}
          variant="ghost"
          size="xs"
          className="h-6 px-2 text-[0.6875rem]"
          tabIndex={-1}
        >
          {t("common.more")}
          <ChevronDown className="h-3 w-3" />
        </Button>
      </div>
    </TabsList>
  );
}

function DockerTabMenuItem({
  count,
  label,
  onSelect,
}: {
  count: number | null;
  label: string;
  onSelect: () => void;
}) {
  return (
    <DropdownMenuItem className="justify-between gap-4" onSelect={onSelect}>
      <span>{label}</span>
      <span className="font-mono text-[0.625rem] text-muted-foreground">{count ?? "-"}</span>
    </DropdownMenuItem>
  );
}

function StateBadge({ state }: { state: string }) {
  const { t } = useTranslation();
  const kind = getDockerStateKind(state);
  const labelKey = getDockerStateLabelKey(state);
  return (
    <Badge
      variant="outline"
      className={cn("max-w-20 px-1.5 text-[0.625rem] leading-4", stateBadgeClass(kind))}
      title={state}
    >
      {t(`dockerManager.stateLabels.${labelKey}`)}
    </Badge>
  );
}

function ResourceTabState({
  children,
  failed,
  loaded,
  loading,
  onRetry,
}: {
  children: ReactNode;
  failed: boolean;
  loaded: boolean;
  loading: boolean;
  onRetry: () => void;
}) {
  const { t } = useTranslation();

  if (loading && !loaded) {
    return <LoadingSpinner label={t("common.loading")} />;
  }

  if (failed && !loaded) {
    return (
      <div className="flex h-full min-h-0 flex-col items-center justify-center gap-2 rounded-md border border-dashed p-4 text-center">
        <span className="text-sm text-muted-foreground">{t("dockerManager.error")}</span>
        <Button variant="ghost" size="xs" onClick={onRetry}>
          {t("common.retry")}
        </Button>
      </div>
    );
  }

  return <>{children}</>;
}

function VirtualListPane<T>({
  getRowHeight,
  items,
  rowHeight,
  renderRow,
}: {
  getRowHeight?: (item: T, index: number) => number;
  items: T[];
  rowHeight: number;
  renderRow: (item: T) => ReactNode;
}) {
  const {
    containerRef,
    visibleItems,
    paddingTop,
    paddingBottom,
    onScroll: handleScroll,
  } = useVirtualList(items, { getItemHeight: getRowHeight, itemHeight: rowHeight, overscan: 6 });

  return (
    <div
      ref={containerRef}
      className="h-full min-h-0 overflow-y-auto terminal-scroll"
      onScroll={handleScroll}
    >
      <div style={{ paddingTop, paddingBottom }}>
        {visibleItems.map(({ item, index }) => (
          <div
            key={index}
            className="pb-1.5"
            style={{ height: getRowHeight ? getRowHeight(item, index) : rowHeight }}
          >
            {renderRow(item)}
          </div>
        ))}
      </div>
      <EmptyList visible={items.length === 0} />
    </div>
  );
}

function ContainerRow({
  container,
  onDetails,
  onEnter,
  onLogs,
  onAction,
  pendingAction,
}: {
  container: DockerContainer;
  onDetails: () => void;
  onEnter: () => void;
  onLogs: () => void;
  onAction: (action: string) => void;
  pendingAction?: string | null;
}) {
  const stateKind = getDockerStateKind(container.state);
  const running = stateKind === "running";
  const startable = canStartDockerContainer(container.state);
  const stoppable = canStopDockerContainer(container.state);
  const pending = Boolean(pendingAction);
  return (
    <div
      className={cn(
        "relative h-full overflow-hidden rounded-md border border-l-2 bg-muted/[0.03] px-3 py-2 transition-colors hover:bg-sky-500/[0.06]",
        stateAccentClass(stateKind),
      )}
    >
      <button
        type="button"
        className="absolute inset-0 z-0 cursor-pointer rounded-md focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        onClick={onDetails}
        aria-label={container.name}
      />
      <div className="pointer-events-none relative z-10 flex h-full min-w-0 flex-col justify-center gap-1.5 pr-7">
        <div className="flex min-w-0 items-center gap-1.5">
          <span
            className="min-w-0 flex-1 truncate text-[0.825rem] font-semibold leading-4"
            title={container.name}
          >
            {container.name}
          </span>
          <StateBadge state={container.state} />
        </div>
        <div className="flex min-w-0 items-center gap-1.5 font-mono text-[0.625rem] leading-3 text-muted-foreground">
          <span className="min-w-0 flex-1 truncate" title={container.image}>
            {container.image}
          </span>
          <span className="shrink-0">{shortId(container.id)}</span>
        </div>
      </div>
      <div className="absolute top-2 right-2 z-20">
        <DockerActionMenu
          pending={pending}
          running={running}
          startable={startable}
          stoppable={stoppable}
          onEnter={onEnter}
          onLogs={onLogs}
          onAction={onAction}
        />
      </div>
    </div>
  );
}

function DockerActionMenu({
  pending,
  running,
  startable,
  stoppable,
  onEnter,
  onLogs,
  onAction,
}: {
  pending: boolean;
  running: boolean;
  startable: boolean;
  stoppable: boolean;
  onEnter: () => void;
  onLogs: () => void;
  onAction: (action: string) => void;
}) {
  const { t } = useTranslation();
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="ghost"
          size="icon"
          className="h-7 w-7"
          disabled={pending}
          onClick={(event) => event.stopPropagation()}
        >
          {pending ? (
            <Loader2 className="h-4 w-4 animate-spin" />
          ) : (
            <EllipsisVertical className="h-4 w-4" />
          )}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" onClick={(event) => event.stopPropagation()}>
        <DropdownMenuItem onSelect={onLogs}>{t("dockerManager.logs")}</DropdownMenuItem>
        <DropdownMenuItem disabled={!running} onSelect={onEnter}>
          {t("dockerManager.enter")}
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem disabled={pending || !startable} onSelect={() => onAction("start")}>
          {t("dockerManager.start")}
        </DropdownMenuItem>
        <DropdownMenuItem disabled={pending || !stoppable} onSelect={() => onAction("stop")}>
          {t("dockerManager.stop")}
        </DropdownMenuItem>
        <DropdownMenuItem disabled={pending} onSelect={() => onAction("restart")}>
          {t("dockerManager.restart")}
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem
          variant="destructive"
          disabled={pending || !running}
          onSelect={() => onAction("kill")}
        >
          {t("dockerManager.kill")}
        </DropdownMenuItem>
        <DropdownMenuItem
          variant="destructive"
          disabled={pending}
          onSelect={() => onAction("remove")}
        >
          {t("common.delete")}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function DockerObjectRow({
  title,
  meta,
  detail,
  identifier,
  onRemove,
  pending = false,
}: {
  title: string;
  meta: string;
  detail: string;
  identifier?: string;
  onRemove: () => void;
  pending?: boolean;
}) {
  return (
    <div className="flex h-full items-center gap-2 rounded-md border px-2.5 py-2 transition-colors hover:bg-sky-500/5">
      <div className="min-w-0 flex-1">
        <div className="truncate text-xs font-semibold leading-4" title={title}>
          {title}
        </div>
        <div className="mt-0.5 flex min-w-0 items-center gap-1.5 font-mono text-[0.625rem] text-muted-foreground">
          <span className="min-w-0 truncate" title={meta}>
            {meta}
          </span>
          <span className="shrink-0 text-muted-foreground/70">{detail}</span>
        </div>
      </div>
      {identifier ? (
        <span className="shrink-0 font-mono text-[0.625rem] text-muted-foreground">
          {identifier}
        </span>
      ) : null}
      <Button
        variant="ghost"
        size="icon"
        className="h-7 w-7 text-destructive"
        disabled={pending}
        onClick={onRemove}
        aria-label={title}
      >
        {pending ? <Loader2 className="h-4 w-4 animate-spin" /> : <Trash2 className="h-4 w-4" />}
      </Button>
    </div>
  );
}

function ComposeRow({
  expanded,
  getServicePendingAction,
  onEnterService,
  onLogsService,
  onRetryServices,
  onServiceAction,
  project,
  onAction,
  onToggle,
  pendingAction,
  servicesState,
}: {
  expanded: boolean;
  getServicePendingAction: (service: DockerComposeService) => string | null;
  onEnterService: (service: DockerComposeService) => void;
  onLogsService: (service: DockerComposeService) => void;
  onRetryServices: () => void;
  onServiceAction: (service: DockerComposeService, action: string) => void;
  project: DockerComposeProject;
  onAction: (action: string) => void;
  onToggle: () => void;
  pendingAction?: string | null;
  servicesState?: ComposeServicesState;
}) {
  const { t } = useTranslation();
  const pending = Boolean(pendingAction);
  const services = servicesState?.services ?? [];
  return (
    <div className="h-full overflow-hidden rounded-md border transition-colors hover:bg-sky-500/5">
      <div className="flex h-[74px] items-start gap-2 px-2.5 py-2">
        <button
          type="button"
          className="mt-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          onClick={onToggle}
          aria-label={project.name}
        >
          <ChevronRight className={cn("h-4 w-4 transition-transform", expanded && "rotate-90")} />
        </button>
        <button type="button" className="min-w-0 flex-1 text-left" onClick={onToggle}>
          <div className="flex min-w-0 items-center gap-1.5">
            <div
              className="min-w-0 flex-1 truncate text-xs font-semibold leading-4"
              title={project.name}
            >
              {project.name}
            </div>
            <Badge variant="outline" className="max-w-24 truncate px-1.5 text-[0.625rem]">
              {project.status || "-"}
            </Badge>
          </div>
          <div className="mt-1 truncate font-mono text-[0.625rem] text-muted-foreground">
            {project.config_files || "-"}
          </div>
        </button>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="icon"
              className="h-7 w-7"
              disabled={pending}
              aria-label={t("dockerManager.composeActions")}
            >
              {pending ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <EllipsisVertical className="h-4 w-4" />
              )}
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem disabled={pending} onSelect={() => onAction("up")}>
              {t("dockerManager.up")}
            </DropdownMenuItem>
            <DropdownMenuItem disabled={pending} onSelect={() => onAction("restart")}>
              {t("dockerManager.restart")}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem
              variant="destructive"
              disabled={pending}
              onSelect={() => onAction("down")}
            >
              {t("dockerManager.down")}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      {expanded ? (
        <div className="border-t bg-muted/[0.025] px-2.5 py-1.5">
          {servicesState?.loading ? (
            <div className="flex h-10 items-center justify-center gap-2 text-xs text-muted-foreground">
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
              {t("dockerManager.loadingServices")}
            </div>
          ) : servicesState?.error ? (
            <div className="flex h-10 items-center justify-between gap-2 text-xs text-muted-foreground">
              <span className="min-w-0 truncate">{t("dockerManager.serviceLoadFailed")}</span>
              <Button variant="ghost" size="xs" onClick={onRetryServices}>
                {t("common.retry")}
              </Button>
            </div>
          ) : services.length > 0 ? (
            <div className="space-y-1">
              {services.map((service) => (
                <ComposeServiceRow
                  key={service.name}
                  pendingAction={getServicePendingAction(service)}
                  service={service}
                  onEnter={() => onEnterService(service)}
                  onLogs={() => onLogsService(service)}
                  onAction={(action) => onServiceAction(service, action)}
                />
              ))}
            </div>
          ) : (
            <div className="flex h-10 items-center justify-center text-xs text-muted-foreground">
              {t("dockerManager.noServices")}
            </div>
          )}
        </div>
      ) : null}
    </div>
  );
}

function ComposeServiceRow({
  pendingAction,
  service,
  onAction,
  onEnter,
  onLogs,
}: {
  pendingAction?: string | null;
  service: DockerComposeService;
  onAction: (action: string) => void;
  onEnter: () => void;
  onLogs: () => void;
}) {
  const { t } = useTranslation();
  const pending = Boolean(pendingAction);
  const canEnter = service.containers.some(
    (container) => container.state.toLowerCase() === "running",
  );
  const containerSummary =
    service.containers.length > 0
      ? service.containers.map((container) => container.name || shortId(container.id)).join(", ")
      : t("dockerManager.noContainers");

  return (
    <div className="flex h-[58px] items-center gap-2 rounded-md bg-background/30 px-2">
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-1.5">
          <div className="min-w-0 flex-1 truncate text-xs font-medium" title={service.name}>
            {service.name}
          </div>
          <Badge variant="outline" className="max-w-24 truncate px-1.5 text-[0.625rem]">
            {service.status || t("dockerManager.notCreated")}
          </Badge>
        </div>
        <div className="mt-1 truncate font-mono text-[0.625rem] text-muted-foreground">
          {containerSummary}
        </div>
      </div>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            disabled={pending}
            aria-label={service.name}
          >
            {pending ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <EllipsisVertical className="h-4 w-4" />
            )}
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          <DropdownMenuItem onSelect={onLogs}>{t("dockerManager.logs")}</DropdownMenuItem>
          <DropdownMenuItem disabled={!canEnter} onSelect={onEnter}>
            {t("dockerManager.enter")}
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem disabled={pending} onSelect={() => onAction("up")}>
            {t("dockerManager.up")}
          </DropdownMenuItem>
          <DropdownMenuItem disabled={pending} onSelect={() => onAction("stop")}>
            {t("dockerManager.stop")}
          </DropdownMenuItem>
          <DropdownMenuItem disabled={pending} onSelect={() => onAction("restart")}>
            {t("dockerManager.restart")}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}

function EmptyList({ visible }: { visible: boolean }) {
  const { t } = useTranslation();
  if (!visible) return null;
  return (
    <div className="rounded-lg border border-dashed p-6 text-center text-sm text-muted-foreground">
      {t("dockerManager.noMatches")}
    </div>
  );
}

function EmptyState({ icon, text }: { icon: ReactNode; text: string }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-2 px-4 text-center">
      <span className="text-2xl" style={{ color: "var(--df-text-dimmed)" }}>
        {icon}
      </span>
      <span className="text-sm" style={{ color: "var(--df-text-muted)" }}>
        {text}
      </span>
    </div>
  );
}

function LoadingSpinner({ label }: { label: string }) {
  return (
    <div className="flex h-full items-center justify-center">
      <svg
        className="h-5 w-5 animate-spin"
        style={{ color: "var(--df-text-dimmed)" }}
        xmlns="http://www.w3.org/2000/svg"
        fill="none"
        viewBox="0 0 24 24"
      >
        <title>{label}</title>
        <circle
          className="opacity-25"
          cx="12"
          cy="12"
          r="10"
          stroke="currentColor"
          strokeWidth="4"
        />
        <path
          className="opacity-75"
          fill="currentColor"
          d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"
        />
      </svg>
    </div>
  );
}

function shortId(id: string) {
  return id.replace(/^sha256:/, "").slice(0, 12);
}
