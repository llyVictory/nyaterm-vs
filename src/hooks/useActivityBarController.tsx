import type { TFunction } from "i18next";
import { type ReactNode, useCallback, useEffect, useMemo } from "react";
import { BiServer } from "react-icons/bi";
import { FaRegFolder } from "react-icons/fa";
import { LuKeyRound } from "react-icons/lu";
import {
  MdAutoAwesome,
  MdBackup,
  MdBolt,
  MdHistory,
  MdLan,
  MdLink,
  MdListAlt,
  MdLock,
  MdOutlineMonitorHeart,
  MdOutlineStickyNote2,
  MdSend,
  MdSettings,
  MdExtension,
} from "react-icons/md";
import { PiRecordFill } from "react-icons/pi";
import { SiDocker, SiNvidia } from "react-icons/si";
import { usePlugins } from "@/context/PluginContext";
import { activeManifest, getPluginPanels, pluginPanelId } from "@/lib/plugins";
import type { ActivityBarItem } from "@/components/layout/ActivityBar";
import {
  ACTIVITY_BAR_ITEM_IDS,
  ACTIVITY_LAYOUT_ZONES,
  buildMultiPanelToggleUpdate,
  canUseFloatingPanel,
  getHiddenActivityItemsForSide,
  getItemSide,
  hideActivityBarItem,
  isActivityBarItemVisible,
  isActivityItemAvailable,
  mergeVisibleReorder,
  resetActivityBarLayout,
  showActivityBarItem,
  toggleActivityBarItemVisibility,
  type PanelOpenMode,
} from "@/lib/appWorkspace";
import { focusTerminalSession } from "@/lib/appSessionFactory";
import { openSettings } from "@/lib/windowManager";
import type { ActivityBarLayout, ActivityBarZone, UiConfig } from "@/types/global";

type UpdateUi = (updates: Partial<UiConfig> | ((prev: UiConfig) => Partial<UiConfig>)) => void;

function insertAfter(ids: string[], anchorId: string, itemId: string) {
  if (ids.includes(itemId)) return ids;
  const next = [...ids];
  const anchorIndex = next.indexOf(anchorId);
  if (anchorIndex === -1) {
    next.unshift(itemId);
  } else {
    next.splice(anchorIndex + 1, 0, itemId);
  }
  return next;
}

function insertBeforeOrPush(ids: string[], anchorId: string, itemId: string) {
  if (ids.includes(itemId)) return ids;
  const next = [...ids];
  const anchorIndex = next.indexOf(anchorId);
  if (anchorIndex === -1) {
    next.push(itemId);
  } else {
    next.splice(anchorIndex, 0, itemId);
  }
  return next;
}

function AscendIcon() {
  return (
    <span
      aria-hidden="true"
      className="inline-block h-[1em] w-[1em] bg-current"
      style={{
        WebkitMask: `url('${import.meta.env.BASE_URL}icons/brands/ascend.svg') center / contain no-repeat`,
        mask: `url('${import.meta.env.BASE_URL}icons/brands/ascend.svg') center / contain no-repeat`,
      }}
    />
  );
}

function normalizeActivityBarState(uiConfig: UiConfig): Partial<UiConfig> | null {
  const originalLeftOpenPanels = uiConfig.left_open_panels ?? [];
  const originalRightOpenPanels = uiConfig.right_open_panels ?? [];
  const seen = new Set<string>();
  const layout: ActivityBarLayout = {
    ...uiConfig.activity_bar_layout,
    left_top: [],
    left_bottom: [],
    right_top: [],
    right_bottom: [],
    hidden_items: [],
  };

  for (const zone of ACTIVITY_LAYOUT_ZONES) {
    for (const id of uiConfig.activity_bar_layout[zone]) {
      if (id === "fileTransfer") continue;
      if (seen.has(id)) continue;
      seen.add(id);
      layout[zone].push(id);
    }
  }

  if (!seen.has("syncBackupHistory")) {
    layout.left_bottom = insertBeforeOrPush(layout.left_bottom, "settings", "syncBackupHistory");
    seen.add("syncBackupHistory");
  }
  if (!seen.has("plugins")) {
    layout.left_bottom = insertBeforeOrPush(
      layout.left_bottom,
      "settings",
      "plugins",
    );
    seen.add("plugins");
  }
  if (!seen.has("notes")) {
    layout.left_top = insertAfter(layout.left_top, "fileExplorer", "notes");
    seen.add("notes");
  }
  if (!seen.has("aiAssistant")) {
    layout.right_top = insertAfter(layout.right_top, "savedConnections", "aiAssistant");
    seen.add("aiAssistant");
  }

  if (!seen.has("serialSend")) {
    const quickCmdIndex = layout.right_bottom.indexOf("quickCmdBar");
    const recordingIndex = layout.right_bottom.indexOf("recording");
    const lockIndex = layout.right_bottom.indexOf("lock");
    if (quickCmdIndex !== -1) {
      layout.right_bottom.splice(quickCmdIndex + 1, 0, "serialSend");
    } else if (recordingIndex !== -1) {
      layout.right_bottom.splice(recordingIndex, 0, "serialSend");
    } else if (lockIndex !== -1) {
      layout.right_bottom.splice(lockIndex, 0, "serialSend");
    } else {
      layout.right_bottom.push("serialSend");
    }
    seen.add("serialSend");
  }

  if (!seen.has("recording")) {
    layout.right_bottom = insertBeforeOrPush(layout.right_bottom, "lock", "recording");
    seen.add("recording");
  }
  if (!seen.has("gpuMonitor")) {
    layout.right_top = insertAfter(layout.right_top, "resourceMonitor", "gpuMonitor");
    seen.add("gpuMonitor");
  }
  if (!seen.has("ascendNpuMonitor")) {
    layout.right_top = insertAfter(layout.right_top, "gpuMonitor", "ascendNpuMonitor");
    seen.add("ascendNpuMonitor");
  }
  if (!seen.has("processManager")) {
    layout.right_top = insertAfter(layout.right_top, "ascendNpuMonitor", "processManager");
    seen.add("processManager");
  }
  if (!seen.has("dockerManager")) {
    layout.right_top = insertAfter(layout.right_top, "processManager", "dockerManager");
    seen.add("dockerManager");
  }

  for (const id of uiConfig.activity_bar_layout.hidden_items ?? []) {
    if (layout.hidden_items.includes(id)) continue;
    if (!seen.has(id)) continue;
    layout.hidden_items.push(id);
  }

  const leftPanelIds = new Set(
    [...layout.left_top, ...layout.left_bottom].filter((id) =>
      isActivityItemAvailable(id, uiConfig),
    ),
  );
  const rightPanelIds = new Set(
    [...layout.right_top, ...layout.right_bottom].filter((id) =>
      isActivityItemAvailable(id, uiConfig),
    ),
  );
  const leftOpenPanels = [...new Set(originalLeftOpenPanels)].filter((id) => leftPanelIds.has(id));
  const rightOpenPanels = [...new Set(originalRightOpenPanels)].filter((id) =>
    rightPanelIds.has(id),
  );
  const activeLeftPanel =
    uiConfig.active_left_panel && leftPanelIds.has(uiConfig.active_left_panel)
      ? uiConfig.active_left_panel
      : uiConfig.active_left_panel === "fileTransfer" && leftPanelIds.has("fileExplorer")
        ? "fileExplorer"
        : null;
  const activeRightPanel =
    uiConfig.active_right_panel && rightPanelIds.has(uiConfig.active_right_panel)
      ? uiConfig.active_right_panel
      : null;

  const layoutChanged = ACTIVITY_LAYOUT_ZONES.some(
    (zone) =>
      layout[zone].length !== uiConfig.activity_bar_layout[zone].length ||
      layout[zone].some((id, index) => id !== uiConfig.activity_bar_layout[zone][index]),
  );
  const originalHiddenItems = uiConfig.activity_bar_layout.hidden_items ?? [];
  const hiddenItemsChanged =
    layout.hidden_items.length !== originalHiddenItems.length ||
    layout.hidden_items.some((id, index) => id !== originalHiddenItems[index]);
  const leftOpenChanged =
    leftOpenPanels.length !== originalLeftOpenPanels.length ||
    leftOpenPanels.some((id, index) => id !== originalLeftOpenPanels[index]);
  const rightOpenChanged =
    rightOpenPanels.length !== originalRightOpenPanels.length ||
    rightOpenPanels.some((id, index) => id !== originalRightOpenPanels[index]);
  const activeLeftChanged = activeLeftPanel !== uiConfig.active_left_panel;
  const activeRightChanged = activeRightPanel !== uiConfig.active_right_panel;

  if (
    !layoutChanged &&
    !hiddenItemsChanged &&
    !leftOpenChanged &&
    !rightOpenChanged &&
    !activeLeftChanged &&
    !activeRightChanged
  ) {
    return null;
  }

  return {
    ...(layoutChanged || hiddenItemsChanged ? { activity_bar_layout: layout } : {}),
    ...(leftOpenChanged ? { left_open_panels: leftOpenPanels } : {}),
    ...(rightOpenChanged ? { right_open_panels: rightOpenPanels } : {}),
    ...(activeLeftChanged ? { active_left_panel: activeLeftPanel } : {}),
    ...(activeRightChanged ? { active_right_panel: activeRightPanel } : {}),
  };
}

interface UseActivityBarControllerOptions {
  uiConfig: UiConfig;
  activeSessionId: string | null;
  recordingSessions: Set<string>;
  multiPanelOpen: boolean;
  panelOpenMode: PanelOpenMode;
  onFloatingPanelSelect: (panelId: string, side: "left" | "right") => void;
  onFloatingPanelMove: (panelId: string, targetSide: "left" | "right") => void;
  updateUi: UpdateUi;
  setIsLocked: (locked: boolean) => void;
  t: TFunction;
}

export function useActivityBarController({
  uiConfig,
  activeSessionId,
  recordingSessions,
  multiPanelOpen,
  panelOpenMode,
  onFloatingPanelSelect,
  onFloatingPanelMove,
  updateUi,
  setIsLocked,
  t,
}: UseActivityBarControllerOptions) {
  const { plugins, openIntent } = usePlugins();
  const pluginPanels = useMemo(() => getPluginPanels(plugins), [plugins]);
  const itemRegistry = useMemo<Record<string, { icon: ReactNode; tooltip: string }>>(
    () => ({
      fileExplorer: { icon: <FaRegFolder />, tooltip: t("panel.fileExplorer") },
      notes: { icon: <MdOutlineStickyNote2 />, tooltip: t("panel.notes") },
      network: { icon: <MdLan />, tooltip: t("panel.network") },
      securityAuth: { icon: <LuKeyRound />, tooltip: t("securityAuth.title") },
      syncBackupHistory: { icon: <MdBackup />, tooltip: t("panel.syncBackupHistory") },
      settings: { icon: <MdSettings />, tooltip: t("settings.title") },
      plugins: { icon: <MdExtension />, tooltip: t("plugins.title") },
      ...Object.fromEntries(
        pluginPanels.map((panel) => [
          panel.activityId,
          { icon: <MdExtension />, tooltip: panel.title },
        ]),
      ),
      savedConnections: { icon: <BiServer />, tooltip: t("panel.savedConnections") },
      aiAssistant: { icon: <MdAutoAwesome />, tooltip: t("ai.title") },
      activeSessions: { icon: <MdLink />, tooltip: t("panel.activeSessions") },
      commandHistory: { icon: <MdHistory />, tooltip: t("panel.commandHistory") },
      resourceMonitor: { icon: <MdOutlineMonitorHeart />, tooltip: t("panel.resourceMonitor") },
      gpuMonitor: { icon: <SiNvidia />, tooltip: t("panel.gpuMonitor") },
      ascendNpuMonitor: { icon: <AscendIcon />, tooltip: t("panel.ascendNpuMonitor") },
      processManager: { icon: <MdListAlt />, tooltip: t("panel.processManager") },
      dockerManager: { icon: <SiDocker />, tooltip: t("panel.dockerManager") },
      quickCmdBar: { icon: <MdBolt />, tooltip: t("panel.quickCommands") },
      serialSend: { icon: <MdSend />, tooltip: t("panel.serialSend", "Command Send") },
      recording: {
        icon: (
          <PiRecordFill
            className={recordingSessions.size > 0 ? "animate-pulse" : undefined}
          />
        ),
        tooltip: t("recording.panelTitle"),
      },
      lock: { icon: <MdLock />, tooltip: t("statusBar.lock") },
    }),
    [recordingSessions, t, pluginPanels],
  );

  const layout = uiConfig.activity_bar_layout;
  useEffect(() => {
    const allIds = new Set(
      ACTIVITY_LAYOUT_ZONES.flatMap((zone) => layout[zone]),
    );
    if (!pluginPanels.some((panel) => !allIds.has(panel.activityId))) return;
    updateUi((prev) => {
      const existing = new Set(
        ACTIVITY_LAYOUT_ZONES.flatMap((zone) => prev.activity_bar_layout[zone]),
      );
      const added = pluginPanels
        .map((panel) => panel.activityId)
        .filter((id) => !existing.has(id));
      return added.length
        ? {
            activity_bar_layout: {
              ...prev.activity_bar_layout,
              right_top: [...prev.activity_bar_layout.right_top, ...added],
            },
          }
        : {};
    });
  }, [layout, pluginPanels, updateUi]);

  useEffect(() => {
    if (!openIntent) return;
    const plugin = plugins.find(
      (plugin) => plugin.id === openIntent.pluginId && plugin.enabled,
    );
    const manifest = plugin && activeManifest(plugin);
    const panelId =
      openIntent.panelId ??
      manifest?.contributions.commands.find(
        (command) => command.id === openIntent.commandId,
      )?.panel;
    if (
      !panelId ||
      !manifest?.contributions.panels.some((panel) => panel.id === panelId)
    )
      return;
    const id = pluginPanelId(openIntent.pluginId, panelId);
    updateUi((prev) => {
      const side = getItemSide(id, prev.activity_bar_layout) ?? "right";
      const list = side === "left" ? "left_open_panels" : "right_open_panels";
      return {
        ...(side === "left"
          ? { active_left_panel: id }
          : { active_right_panel: id }),
        [list]: [...new Set([...(prev[list] ?? []), id])],
        activity_bar_layout: showActivityBarItem(prev.activity_bar_layout, id),
      };
    });
  }, [openIntent, plugins, updateUi]);

  useEffect(() => {
    if (!normalizeActivityBarState(uiConfig)) return;
    updateUi((prev) => {
      return normalizeActivityBarState(prev) ?? {};
    });
  }, [uiConfig, updateUi]);

  const buildItems = useCallback(
    (ids: string[]): ActivityBarItem[] =>
      ids
        .filter((id) => id in itemRegistry && isActivityBarItemVisible(id, uiConfig))
        .map((id) => ({ id, ...itemRegistry[id] })),
    [itemRegistry, uiConfig],
  );

  const leftTopItems = useMemo(() => buildItems(layout.left_top), [buildItems, layout.left_top]);
  const leftBottomItems = useMemo(
    () => buildItems(layout.left_bottom),
    [buildItems, layout.left_bottom],
  );
  const rightTopItems = useMemo(() => buildItems(layout.right_top), [buildItems, layout.right_top]);
  const rightBottomItems = useMemo(
    () => buildItems(layout.right_bottom),
    [buildItems, layout.right_bottom],
  );
  const leftHiddenItems = useMemo(
    () =>
      getHiddenActivityItemsForSide(uiConfig, "left", ACTIVITY_BAR_ITEM_IDS)
        .filter((id) => id in itemRegistry)
        .map((id) => ({ id, ...itemRegistry[id] })),
    [itemRegistry, uiConfig],
  );
  const rightHiddenItems = useMemo(
    () =>
      getHiddenActivityItemsForSide(uiConfig, "right", ACTIVITY_BAR_ITEM_IDS)
        .filter((id) => id in itemRegistry)
        .map((id) => ({ id, ...itemRegistry[id] })),
    [itemRegistry, uiConfig],
  );

  const toggleActiveIds = useMemo(() => {
    const activeIds = new Set<string>();
    if (uiConfig.show_quick_cmd_bar) activeIds.add("quickCmdBar");
    if (uiConfig.show_serial_send_panel) activeIds.add("serialSend");
    if (recordingSessions.size > 0) activeIds.add("recording");
    return activeIds;
  }, [recordingSessions, uiConfig.show_quick_cmd_bar, uiConfig.show_serial_send_panel]);

  useEffect(() => {
    if (!uiConfig.show_quick_cmd_bar || !uiConfig.show_serial_send_panel) return;
    updateUi({ show_quick_cmd_bar: false });
  }, [uiConfig.show_quick_cmd_bar, uiConfig.show_serial_send_panel, updateUi]);

  const handleItemSelect = useCallback(
    (id: string) => {
      if (id === "settings") {
        const terminalSessionId = activeSessionId;
        const restoreTerminalFocus = () => focusTerminalSession(terminalSessionId);
        void openSettings().then(restoreTerminalFocus, restoreTerminalFocus);
        return;
      }
      if (id === "lock") {
        setIsLocked(true);
        return;
      }
      if (id === "quickCmdBar") {
        updateUi((prev) => ({
          show_quick_cmd_bar: !prev.show_quick_cmd_bar,
          ...(prev.show_serial_send_panel ? { show_serial_send_panel: false } : {}),
        }));
        return;
      }
      if (id === "serialSend") {
        updateUi((prev) => ({
          show_serial_send_panel: !prev.show_serial_send_panel,
          ...(prev.show_quick_cmd_bar ? { show_quick_cmd_bar: false } : {}),
        }));
        return;
      }
      const side = getItemSide(id, layout);
      if (!side) return;
      if (panelOpenMode === "floating" && canUseFloatingPanel(id)) {
        onFloatingPanelSelect(id, side);
        return;
      }
      if (multiPanelOpen) {
        updateUi((prev) => buildMultiPanelToggleUpdate(prev, id, side));
        return;
      }
      if (side === "left") {
        updateUi((prev) => ({ active_left_panel: prev.active_left_panel === id ? null : id }));
      } else if (side === "right") {
        updateUi((prev) => ({ active_right_panel: prev.active_right_panel === id ? null : id }));
      }
    },
    [
      activeSessionId,
      layout,
      multiPanelOpen,
      onFloatingPanelSelect,
      panelOpenMode,
      setIsLocked,
      updateUi,
    ],
  );

  const handleReorder = useCallback(
    (side: "left" | "right", zoneKey: "top" | "bottom", orderedIds: string[]) => {
      const layoutKey = `${side}_${zoneKey}` as ActivityBarZone;
      updateUi((prev) => ({
        activity_bar_layout: {
          ...prev.activity_bar_layout,
          [layoutKey]: mergeVisibleReorder(prev.activity_bar_layout[layoutKey], orderedIds, prev),
        },
      }));
    },
    [updateUi],
  );

  const handleMoveItem = useCallback(
    (itemId: string, targetZone: ActivityBarZone) => {
      const isMovingToRight = targetZone === "right_top" || targetZone === "right_bottom";
      const isMovingToLeft = targetZone === "left_top" || targetZone === "left_bottom";
      updateUi((prev) => {
        const zones = ["left_top", "left_bottom", "right_top", "right_bottom"] as const;
        const newLayout = { ...prev.activity_bar_layout };
        for (const zone of zones) {
          newLayout[zone] = newLayout[zone].filter((id) => id !== itemId);
        }
        newLayout[targetZone] = [...newLayout[targetZone], itemId];
        return {
          activity_bar_layout: newLayout,
          ...(prev.active_left_panel === itemId && isMovingToRight
            ? { active_left_panel: null }
            : {}),
          ...(prev.active_right_panel === itemId && isMovingToLeft
            ? { active_right_panel: null }
            : {}),
          ...(isMovingToRight && prev.left_open_panels?.includes(itemId)
            ? { left_open_panels: prev.left_open_panels.filter((id) => id !== itemId) }
            : {}),
          ...(isMovingToLeft && prev.right_open_panels?.includes(itemId)
            ? { right_open_panels: prev.right_open_panels.filter((id) => id !== itemId) }
            : {}),
        };
      });
      if (canUseFloatingPanel(itemId) && (isMovingToRight || isMovingToLeft)) {
        onFloatingPanelMove(itemId, isMovingToRight ? "right" : "left");
      }
    },
    [onFloatingPanelMove, updateUi],
  );

  const handleToggleLabel = useCallback(() => {
    updateUi((prev) => ({
      activity_bar_layout: {
        ...prev.activity_bar_layout,
        show_labels: !prev.activity_bar_layout.show_labels,
      },
    }));
  }, [updateUi]);

  const handleHideItem = useCallback(
    (itemId: string) => {
      updateUi((prev) => ({
        activity_bar_layout: hideActivityBarItem(prev.activity_bar_layout, itemId),
      }));
    },
    [updateUi],
  );

  const handleShowItem = useCallback(
    (itemId: string) => {
      updateUi((prev) => ({
        activity_bar_layout: showActivityBarItem(prev.activity_bar_layout, itemId),
      }));
    },
    [updateUi],
  );

  const handleToggleItemVisibility = useCallback(
    (itemId: string) => {
      updateUi((prev) => ({
        activity_bar_layout: toggleActivityBarItemVisibility(prev.activity_bar_layout, itemId),
      }));
    },
    [updateUi],
  );

  const handleResetActivityBarLayout = useCallback(() => {
    updateUi((prev) => {
      const activityBarLayout = resetActivityBarLayout();
      const next = {
        ...prev,
        activity_bar_layout: activityBarLayout,
      };
      return {
        activity_bar_layout: activityBarLayout,
        ...(normalizeActivityBarState(next) ?? {}),
      };
    });
  }, [updateUi]);

  return {
    leftTopItems,
    leftBottomItems,
    rightTopItems,
    rightBottomItems,
    leftHiddenItems,
    rightHiddenItems,
    showLabels: layout.show_labels,
    toggleActiveIds,
    handleItemSelect,
    handleReorder,
    handleMoveItem,
    handleToggleLabel,
    handleHideItem,
    handleShowItem,
    handleToggleItemVisibility,
    handleResetActivityBarLayout,
  };
}
