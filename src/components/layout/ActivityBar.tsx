import {
  type DragEvent,
  type MutableRefObject,
  type PointerEvent,
  type ReactNode,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import {
  HiMiniArrowTurnLeftDown,
  HiMiniArrowTurnLeftUp,
  HiMiniArrowTurnRightDown,
  HiMiniArrowTurnRightUp,
} from "react-icons/hi2";
import {
  ContextMenu,
  ContextMenuCheckboxItem,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import type { PanelOpenMode } from "@/lib/appWorkspace";
import type { ActivityBarZone } from "@/types/global";

export interface ActivityBarItem {
  id: string;
  icon: ReactNode;
  tooltip: string;
}

const DRAG_MIME = "application/x-nyaterm-activity";
const POINTER_DRAG_THRESHOLD_PX = 4;

interface PointerActivityDragState {
  itemId: string;
  sourceZone: ActivityBarZone;
  pointerId: number;
  startX: number;
  startY: number;
  dragging: boolean;
}

interface ActivityDropZoneRegistryEntry {
  zoneKey: "top" | "bottom";
  items: ActivityBarItem[];
  onReorder: (zone: "top" | "bottom", orderedIds: string[]) => void;
  onMoveItem: (itemId: string, targetZone: ActivityBarZone) => void;
}

const activityDropZones = new Map<ActivityBarZone, ActivityDropZoneRegistryEntry>();

function shouldUsePointerActivityDrag() {
  if (typeof navigator === "undefined") return false;
  return /Mac/.test(navigator.platform) && /AppleWebKit/.test(navigator.userAgent);
}

function resolvePointerDropTarget(clientX: number, clientY: number) {
  const elements = document.elementsFromPoint(clientX, clientY);
  for (const element of elements) {
    const target = element.closest<HTMLElement>(
      "[data-activity-drop-zone][data-activity-drop-index]",
    );
    if (!target) continue;

    const targetZone = target.dataset.activityDropZone as ActivityBarZone | undefined;
    const baseIndex = Number(target.dataset.activityDropIndex);
    if (!targetZone || !Number.isFinite(baseIndex)) continue;

    const rect = target.getBoundingClientRect();
    const isEndTarget = target.dataset.activityDropEnd === "true";
    const targetIndex =
      isEndTarget || clientY < rect.top + rect.height / 2 ? baseIndex : baseIndex + 1;
    return { targetZone, targetIndex };
  }

  return null;
}

const ZONE_LABELS: { zone: ActivityBarZone; key: string; icon: ReactNode }[] = [
  { zone: "left_top", key: "activityBar.leftTop", icon: <HiMiniArrowTurnLeftUp /> },
  { zone: "left_bottom", key: "activityBar.leftBottom", icon: <HiMiniArrowTurnLeftDown /> },
  { zone: "right_top", key: "activityBar.rightTop", icon: <HiMiniArrowTurnRightUp /> },
  { zone: "right_bottom", key: "activityBar.rightBottom", icon: <HiMiniArrowTurnRightDown /> },
];

interface ActivityBarProps {
  items: ActivityBarItem[];
  bottomItems?: ActivityBarItem[];
  hiddenItems?: ActivityBarItem[];
  activeId: string | null;
  /** Additional active panel ids (multi-open panel mode). */
  activeIds?: Set<string>;
  activeBottomIds?: Set<string>;
  onSelect: (id: string) => void;
  onReorder: (zone: "top" | "bottom", orderedIds: string[]) => void;
  onMoveItem: (itemId: string, targetZone: ActivityBarZone) => void;
  onHideItem: (itemId: string) => void;
  onShowItem: (itemId: string) => void;
  onToggleLabel: () => void;
  onRequestResetLayout: () => void;
  panelOpenMode: PanelOpenMode;
  onPanelOpenModeChange: (mode: PanelOpenMode) => void;
  showLabels: boolean;
  side: "left" | "right";
  zone: { top: ActivityBarZone; bottom: ActivityBarZone };
}

export default function ActivityBar({
  items,
  bottomItems,
  hiddenItems = [],
  activeId,
  activeIds,
  activeBottomIds,
  onSelect,
  onReorder,
  onMoveItem,
  onHideItem,
  onShowItem,
  onToggleLabel,
  onRequestResetLayout,
  panelOpenMode,
  onPanelOpenModeChange,
  showLabels,
  side,
  zone,
}: ActivityBarProps) {
  const { t } = useTranslation();
  const indicatorSide = side === "left" ? "left-0" : "right-0";
  const tooltipSide = side === "left" ? "right" : "left";

  return (
    <TooltipProvider delayDuration={400}>
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <div
            className="flex flex-col shrink-0 w-10 select-none"
            style={{
              backgroundColor: "var(--df-bg-panel)",
              borderColor: "var(--df-border)",
              borderRightWidth: side === "left" ? 1 : 0,
              borderLeftWidth: side === "right" ? 1 : 0,
            }}
          >
            <DropZone
              items={items}
              zoneKey="top"
              zoneName={zone.top}
              activeId={activeId}
              activeIds={activeIds}
              onSelect={onSelect}
              onReorder={onReorder}
              onMoveItem={onMoveItem}
              onHideItem={onHideItem}
              onToggleLabel={onToggleLabel}
              showLabels={showLabels}
              indicatorSide={indicatorSide}
              tooltipSide={tooltipSide}
              className="flex flex-col items-center gap-0.5 pt-1"
            />
            <DropZone
              items={bottomItems ?? []}
              zoneKey="bottom"
              zoneName={zone.bottom}
              activeId={activeId}
              activeIds={activeIds}
              activeBottomIds={activeBottomIds}
              onSelect={onSelect}
              onReorder={onReorder}
              onMoveItem={onMoveItem}
              onHideItem={onHideItem}
              onToggleLabel={onToggleLabel}
              showLabels={showLabels}
              indicatorSide={indicatorSide}
              tooltipSide={tooltipSide}
              className="mt-auto flex flex-col items-center gap-0.5 pb-1"
            />
          </div>
        </ContextMenuTrigger>
        <ContextMenuContent>
          <ContextMenuCheckboxItem
            checked={panelOpenMode === "floating"}
            onCheckedChange={(checked) =>
              onPanelOpenModeChange(checked ? "floating" : "docked")
            }
          >
            {t("panel.floatingMode")}
          </ContextMenuCheckboxItem>
          <ContextMenuSeparator />
          <ContextMenuSub>
            <ContextMenuSubTrigger disabled={hiddenItems.length === 0}>
              {t("activityBar.hiddenItems")}
            </ContextMenuSubTrigger>
            <ContextMenuSubContent>
              {hiddenItems.length === 0 ? (
                <ContextMenuItem disabled>{t("activityBar.noHiddenItems")}</ContextMenuItem>
              ) : (
                hiddenItems.map((item) => (
                  <ContextMenuItem key={item.id} onClick={() => onShowItem(item.id)}>
                    {item.icon}
                    {item.tooltip}
                  </ContextMenuItem>
                ))
              )}
            </ContextMenuSubContent>
          </ContextMenuSub>
          <ContextMenuSeparator />
          <ContextMenuCheckboxItem checked={showLabels} onCheckedChange={onToggleLabel}>
            {t("activityBar.showLabel")}
          </ContextMenuCheckboxItem>
          <ContextMenuSeparator />
          <ContextMenuItem onClick={onRequestResetLayout}>
            {t("activityBar.resetLayout")}
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>
    </TooltipProvider>
  );
}

interface DropZoneProps {
  items: ActivityBarItem[];
  zoneKey: "top" | "bottom";
  zoneName: ActivityBarZone;
  activeId: string | null;
  activeIds?: Set<string>;
  activeBottomIds?: Set<string>;
  onSelect: (id: string) => void;
  onReorder: (zone: "top" | "bottom", orderedIds: string[]) => void;
  onMoveItem: (itemId: string, targetZone: ActivityBarZone) => void;
  onHideItem: (itemId: string) => void;
  onToggleLabel: () => void;
  showLabels: boolean;
  indicatorSide: string;
  tooltipSide: "left" | "right";
  className: string;
}

function DropZone({
  items,
  zoneKey,
  zoneName,
  activeId,
  activeIds,
  activeBottomIds,
  onSelect,
  onReorder,
  onMoveItem,
  onHideItem,
  onToggleLabel,
  showLabels,
  indicatorSide,
  tooltipSide,
  className,
}: DropZoneProps) {
  const [dropIndex, setDropIndex] = useState<number | null>(null);
  const dragItemIdRef = useRef<string | null>(null);
  const pointerDragRef = useRef<PointerActivityDragState | null>(null);
  const suppressClickRef = useRef(false);
  const usePointerDrag = shouldUsePointerActivityDrag();

  useEffect(() => {
    activityDropZones.set(zoneName, { zoneKey, items, onReorder, onMoveItem });
    return () => {
      activityDropZones.delete(zoneName);
    };
  }, [items, onMoveItem, onReorder, zoneKey, zoneName]);

  const resetDragState = useCallback(() => {
    setDropIndex(null);
    dragItemIdRef.current = null;
    pointerDragRef.current = null;
  }, []);

  const requestPointerDrop = useCallback(
    (state: PointerActivityDragState, targetZone: ActivityBarZone, targetIndex: number) => {
      const targetEntry = activityDropZones.get(targetZone);
      if (!targetEntry) {
        resetDragState();
        return;
      }

      if (targetZone !== state.sourceZone) {
        targetEntry.onMoveItem(state.itemId, targetZone);
        resetDragState();
        return;
      }

      const currentIds = targetEntry.items.map((item) => item.id);
      const fromIdx = currentIds.indexOf(state.itemId);
      if (fromIdx === -1) {
        resetDragState();
        return;
      }

      const reordered = [...currentIds];
      reordered.splice(fromIdx, 1);
      const insertAt = Math.max(
        0,
        Math.min(reordered.length, targetIndex > fromIdx ? targetIndex - 1 : targetIndex),
      );
      reordered.splice(insertAt, 0, state.itemId);
      targetEntry.onReorder(targetEntry.zoneKey, reordered);
      resetDragState();
    },
    [resetDragState],
  );

  const handlePointerDown = useCallback(
    (event: PointerEvent, itemId: string) => {
      if (!usePointerDrag) return;
      if (event.button !== 0 || event.ctrlKey || event.metaKey || event.altKey || event.shiftKey) {
        return;
      }

      pointerDragRef.current = {
        itemId,
        sourceZone: zoneName,
        pointerId: event.pointerId,
        startX: event.clientX,
        startY: event.clientY,
        dragging: false,
      };
      event.currentTarget.setPointerCapture(event.pointerId);
    },
    [usePointerDrag, zoneName],
  );

  const handlePointerMove = useCallback(
    (event: PointerEvent) => {
      const state = pointerDragRef.current;
      if (!state || state.pointerId !== event.pointerId) return;

      const moved =
        Math.abs(event.clientX - state.startX) >= POINTER_DRAG_THRESHOLD_PX ||
        Math.abs(event.clientY - state.startY) >= POINTER_DRAG_THRESHOLD_PX;
      if (!state.dragging) {
        if (!moved) return;
        state.dragging = true;
        suppressClickRef.current = true;
        dragItemIdRef.current = state.itemId;
      }

      const target = resolvePointerDropTarget(event.clientX, event.clientY);
      setDropIndex(target?.targetZone === zoneName ? target.targetIndex : null);
      event.preventDefault();
    },
    [zoneName],
  );

  const handlePointerEnd = useCallback(
    (event: PointerEvent) => {
      const state = pointerDragRef.current;
      if (!state || state.pointerId !== event.pointerId) return;
      pointerDragRef.current = null;
      if (event.currentTarget.hasPointerCapture(event.pointerId)) {
        event.currentTarget.releasePointerCapture(event.pointerId);
      }

      dragItemIdRef.current = null;
      if (!state.dragging) return;

      const target = resolvePointerDropTarget(event.clientX, event.clientY);
      if (target) {
        requestPointerDrop(state, target.targetZone, target.targetIndex);
      } else {
        resetDragState();
      }
      event.preventDefault();
      window.setTimeout(() => {
        suppressClickRef.current = false;
      }, 0);
    },
    [requestPointerDrop, resetDragState],
  );

  const handlePointerCancel = useCallback(
    (event: PointerEvent) => {
      const state = pointerDragRef.current;
      if (!state || state.pointerId !== event.pointerId) return;

      if (event.currentTarget.hasPointerCapture(event.pointerId)) {
        event.currentTarget.releasePointerCapture(event.pointerId);
      }
      resetDragState();
      window.setTimeout(() => {
        suppressClickRef.current = false;
      }, 0);
    },
    [resetDragState],
  );

  const handleDragStart = useCallback(
    (e: DragEvent, itemId: string) => {
      dragItemIdRef.current = itemId;
      e.dataTransfer.setData(DRAG_MIME, JSON.stringify({ id: itemId, zone: zoneName }));
      e.dataTransfer.effectAllowed = "move";
    },
    [zoneName],
  );

  const handleDragOver = useCallback((e: DragEvent, index: number) => {
    if (!e.dataTransfer.types.includes(DRAG_MIME)) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
    setDropIndex(index);
  }, []);

  const handleDragLeave = useCallback(() => {
    setDropIndex(null);
  }, []);

  const handleDrop = useCallback(
    (e: DragEvent, targetIndex: number) => {
      e.preventDefault();
      setDropIndex(null);
      const raw = e.dataTransfer.getData(DRAG_MIME);
      if (!raw) return;
      try {
        const { id, zone: srcZone } = JSON.parse(raw) as { id: string; zone: string };
        if (srcZone !== zoneName) {
          onMoveItem(id, zoneName);
          return;
        }
        const currentIds = items.map((i) => i.id);
        const fromIdx = currentIds.indexOf(id);
        if (fromIdx === -1) return;
        const reordered = [...currentIds];
        reordered.splice(fromIdx, 1);
        const insertAt = targetIndex > fromIdx ? targetIndex - 1 : targetIndex;
        reordered.splice(insertAt, 0, id);
        onReorder(zoneKey, reordered);
      } catch {
        /* ignore malformed data */
      }
    },
    [items, zoneName, zoneKey, onReorder, onMoveItem],
  );

  const handleDragEnd = useCallback(() => {
    resetDragState();
  }, [resetDragState]);

  return (
    <div className={className} onDragLeave={handleDragLeave}>
      {items.map((item, idx) => (
        <ActivityBarButton
          key={item.id}
          item={item}
          active={
            activeId === item.id || !!activeIds?.has(item.id) || !!activeBottomIds?.has(item.id)
          }
          showLabel={showLabels}
          onSelect={onSelect}
          indicatorSide={indicatorSide}
          tooltipSide={tooltipSide}
          currentZone={zoneName}
          onMoveItem={onMoveItem}
          onHideItem={onHideItem}
          onToggleLabel={onToggleLabel}
          dropZoneName={zoneName}
          dropIndex={idx}
          onDragStart={(e) => handleDragStart(e, item.id)}
          onDragOver={(e) => handleDragOver(e, idx)}
          onDrop={(e) => handleDrop(e, idx)}
          onDragEnd={handleDragEnd}
          onPointerDown={(e) => handlePointerDown(e, item.id)}
          onPointerMove={handlePointerMove}
          onPointerEnd={handlePointerEnd}
          onPointerCancel={handlePointerCancel}
          draggable={!usePointerDrag}
          suppressClickRef={suppressClickRef}
          showDropIndicator={dropIndex === idx}
        />
      ))}
      {/* Drop target after last item */}
      <div
        className="w-full h-1"
        data-activity-drop-zone={zoneName}
        data-activity-drop-index={items.length}
        data-activity-drop-end="true"
        onDragOver={(e) => handleDragOver(e, items.length)}
        onDrop={(e) => handleDrop(e, items.length)}
      />
    </div>
  );
}

function ActivityBarButton({
  item,
  active,
  showLabel,
  onSelect,
  indicatorSide,
  tooltipSide,
  currentZone,
  onMoveItem,
  onHideItem,
  onToggleLabel,
  dropZoneName,
  dropIndex,
  onDragStart,
  onDragOver,
  onDrop,
  onDragEnd,
  onPointerDown,
  onPointerMove,
  onPointerEnd,
  onPointerCancel,
  draggable,
  suppressClickRef,
  showDropIndicator,
}: {
  item: ActivityBarItem;
  active: boolean;
  showLabel: boolean;
  onSelect: (id: string) => void;
  indicatorSide: string;
  tooltipSide: "left" | "right";
  currentZone: ActivityBarZone;
  onMoveItem: (itemId: string, targetZone: ActivityBarZone) => void;
  onHideItem: (itemId: string) => void;
  onToggleLabel: () => void;
  dropZoneName: ActivityBarZone;
  dropIndex: number;
  onDragStart: (e: DragEvent) => void;
  onDragOver: (e: DragEvent) => void;
  onDrop: (e: DragEvent) => void;
  onDragEnd: () => void;
  onPointerDown: (e: PointerEvent) => void;
  onPointerMove: (e: PointerEvent) => void;
  onPointerEnd: (e: PointerEvent) => void;
  onPointerCancel: (e: PointerEvent) => void;
  draggable: boolean;
  suppressClickRef: MutableRefObject<boolean>;
  showDropIndicator: boolean;
}) {
  const { t } = useTranslation();

  return (
    <ContextMenu>
      <Tooltip>
        <ContextMenuTrigger asChild>
          <TooltipTrigger asChild>
            <button
              aria-label={item.tooltip}
              draggable={draggable}
              data-activity-drop-zone={dropZoneName}
              data-activity-drop-index={dropIndex}
              onDragStart={onDragStart}
              onDragOver={onDragOver}
              onDrop={onDrop}
              onDragEnd={onDragEnd}
              onPointerDown={onPointerDown}
              onPointerMove={onPointerMove}
              onPointerUp={onPointerEnd}
              onPointerCancel={onPointerCancel}
              onContextMenu={(event) => event.stopPropagation()}
              className={`relative flex flex-col items-center justify-center w-full transition-colors ${showLabel ? "min-h-12 gap-0.5 py-1" : "h-9"}`}
              style={{
                color: active ? "var(--df-primary)" : "var(--df-text-muted)",
                cursor: "default",
              }}
              onClick={() => {
                if (suppressClickRef.current) {
                  suppressClickRef.current = false;
                  return;
                }
                onSelect(item.id);
              }}
            >
              {showDropIndicator && (
                <span
                  className="absolute left-1 right-1 -top-[1px] h-[2px] rounded-full"
                  style={{ backgroundColor: "var(--df-primary)" }}
                />
              )}
              {active && (
                <span
                  className={`absolute ${indicatorSide} top-1 bottom-1 w-[2px] rounded-full`}
                  style={{ backgroundColor: "var(--df-primary)" }}
                />
              )}
              <span className="text-[1.125rem] shrink-0">{item.icon}</span>
              {showLabel && (
                <span
                  className="text-[0.5rem] leading-tight w-full text-center break-words hyphens-auto"
                  lang="zh"
                  style={{ wordBreak: "break-word", overflowWrap: "break-word" }}
                >
                  {item.tooltip}
                </span>
              )}
            </button>
          </TooltipTrigger>
        </ContextMenuTrigger>
        {!showLabel && (
          <TooltipContent side={tooltipSide} sideOffset={4}>
            <span className="text-xs">{item.tooltip}</span>
          </TooltipContent>
        )}
      </Tooltip>

      <ContextMenuContent>
        <ContextMenuSub>
          <ContextMenuSubTrigger>{t("activityBar.moveTo")}</ContextMenuSubTrigger>
          <ContextMenuSubContent>
            {ZONE_LABELS.map(({ zone, key, icon }) => (
              <ContextMenuItem
                key={zone}
                disabled={zone === currentZone}
                onClick={() => onMoveItem(item.id, zone)}
              >
                {icon}
                {t(key)}
              </ContextMenuItem>
            ))}
          </ContextMenuSubContent>
        </ContextMenuSub>
        <ContextMenuSeparator />
        <ContextMenuItem onClick={() => onHideItem(item.id)}>
          {t("activityBar.hideItem", { name: item.tooltip })}
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuCheckboxItem checked={showLabel} onCheckedChange={onToggleLabel}>
          {t("activityBar.showLabel")}
        </ContextMenuCheckboxItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}
