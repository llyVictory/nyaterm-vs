import {
  ChevronRight,
  Fingerprint,
  Gauge,
  RefreshCw,
  Search,
  Thermometer,
  Zap,
} from "lucide-react";
import type { ReactNode } from "react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import PanelHeader from "@/components/layout/PanelHeader";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Input } from "@/components/ui/input";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import type { RemoteNpuOverviewState } from "@/hooks/useRemoteNpuOverview";
import { useVirtualList } from "@/hooks/useVirtualList";
import { cn } from "@/lib/utils";
import type { RemoteNpu, RemoteNpuOverview, RemoteNpuProcess } from "@/types/global";

const NPU_PROCESS_ROW_HEIGHT = 56;
const NPU_PROCESS_LIST_MAX_HEIGHT = 320;

interface AscendNpuMonitorProps {
  activeSessionId: string | null;
  enabled: boolean;
  npuOverviewState: RemoteNpuOverviewState;
}

function AscendBrandIcon({ className }: { className?: string }) {
  return (
    <span
      aria-hidden="true"
      className={cn("inline-block bg-current", className)}
      style={{
        WebkitMask: `url('${import.meta.env.BASE_URL}icons/brands/ascend.svg') center / contain no-repeat`,
        mask: `url('${import.meta.env.BASE_URL}icons/brands/ascend.svg') center / contain no-repeat`,
      }}
    />
  );
}

export default function AscendNpuMonitor({
  activeSessionId,
  enabled,
  npuOverviewState,
}: AscendNpuMonitorProps) {
  const { t } = useTranslation();
  const [query, setQuery] = useState("");
  const { error, isManualRefreshing, overview, refresh } = npuOverviewState;

  const normalizedQuery = query.trim().toLowerCase();
  const processes = useMemo(() => {
    const items = overview?.processes ?? [];
    return items
      .filter((process) => processMatches(process, normalizedQuery))
      .sort(
        (left, right) =>
          right.used_memory_mb - left.used_memory_mb ||
          left.npu_index - right.npu_index ||
          left.chip_id - right.chip_id ||
          left.pid - right.pid,
      );
  }, [normalizedQuery, overview?.processes]);
  const summary = useMemo(() => buildNpuSummary(overview), [overview]);

  return (
    <div
      className="nyaterm-wallpaper-transparent-surface h-full flex flex-col"
      style={{ backgroundColor: "var(--df-bg-panel)" }}
    >
      <PanelHeader
        title={t("panel.ascendNpuMonitor")}
        meta={
          overview?.available ? (
            <span className="font-mono">
              {t("ascendNpuMonitor.driver")} {overview.driver_version || "-"} ·{" "}
              {t("ascendNpuMonitor.cann")} {overview.cann_version || "-"}
            </span>
          ) : null
        }
        actions={
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
        }
      />

      <div className="flex-1 min-h-0 overflow-y-auto terminal-scroll p-2.5">
        {!activeSessionId ? (
          <EmptyState
            icon={<AscendBrandIcon className="h-7 w-7" />}
            text={t("ascendNpuMonitor.noSession")}
          />
        ) : !enabled ? (
          <EmptyState
            icon={<AscendBrandIcon className="h-7 w-7" />}
            text={t("ascendNpuMonitor.disabled")}
          />
        ) : error && !overview ? (
          <EmptyState
            icon={<AscendBrandIcon className="h-7 w-7" />}
            text={t("ascendNpuMonitor.error")}
          />
        ) : !overview ? (
          <LoadingState label={t("common.loading")} />
        ) : !overview.available ? (
          <EmptyState
            icon={<AscendBrandIcon className="h-7 w-7" />}
            text={t("ascendNpuMonitor.unavailable")}
          />
        ) : overview.npus.length === 0 ? (
          <EmptyState
            icon={<AscendBrandIcon className="h-7 w-7" />}
            text={t("ascendNpuMonitor.noNpus")}
          />
        ) : (
          <div className="space-y-2.5">
            <SummaryGrid summary={summary} />

            <div className="space-y-2">
              {overview.npus.map((npu) => (
                <NpuCard key={npu.device_key} npu={npu} />
              ))}
            </div>

            <div className="space-y-2">
              <div className="relative">
                <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
                <Input
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                  placeholder={t("ascendNpuMonitor.search")}
                  className="h-8 pl-8 text-xs"
                />
              </div>
              {processes.length > 0 ? (
                <VirtualNpuProcessList processes={processes} />
              ) : (
                <div>
                  <div className="rounded-md border border-dashed p-4 text-center text-xs text-muted-foreground">
                    {normalizedQuery
                      ? t("ascendNpuMonitor.noMatches")
                      : t("ascendNpuMonitor.noProcesses")}
                  </div>
                </div>
              )}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function SummaryGrid({ summary }: { summary: NpuSummary }) {
  const { t } = useTranslation();
  return (
    <div className="grid grid-cols-2 gap-1.5">
      <SummaryItem label={t("ascendNpuMonitor.npus")} value={summary.npuCount.toString()} />
      <SummaryItem
        label={t("ascendNpuMonitor.maxAicore")}
        value={formatOptionalPercent(summary.maxAicore)}
      />
      <SummaryItem
        label={t("ascendNpuMonitor.memory")}
        value={`${formatMemoryMb(summary.memoryUsedMb)} / ${formatMemoryMb(summary.memoryTotalMb)}`}
      />
      <SummaryItem
        label={t("ascendNpuMonitor.maxTemperature")}
        value={summary.maxTemperature == null ? "-" : `${Math.round(summary.maxTemperature)} C`}
      />
    </div>
  );
}

function SummaryItem({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-md border bg-background/35 px-2.5 py-2">
      <div className="text-[0.625rem] uppercase text-muted-foreground">{label}</div>
      <div className="mt-1 truncate font-mono text-sm font-semibold">{value}</div>
    </div>
  );
}

function NpuCard({ npu }: { npu: RemoteNpu }) {
  const { t } = useTranslation();
  const [detailsOpen, setDetailsOpen] = useState(false);
  const memoryPercent =
    npu.memory_total_mb > 0 ? (npu.memory_used_mb / npu.memory_total_mb) * 100 : null;
  const aicoreUtilization = npu.utilization_aicore_percent ?? null;
  const severityValue = Math.max(aicoreUtilization ?? 0, memoryPercent ?? 0);

  return (
    <Collapsible open={detailsOpen} onOpenChange={setDetailsOpen}>
      <div
        className={cn(
          "rounded-md border border-l-2 bg-background/35 px-3 py-2.5",
          severityValue >= 90
            ? "border-l-red-500"
            : severityValue >= 70
              ? "border-l-amber-400"
              : "border-l-emerald-500",
        )}
      >
        <div className="flex min-w-0 items-start justify-between gap-2">
          <div className="min-w-0">
            <div className="flex min-w-0 items-center gap-2">
              <Badge
                variant="secondary"
                className="shrink-0 border border-primary/30 bg-primary/10 px-1.5 font-mono text-[0.6875rem] font-semibold text-primary"
              >
                NPU #{npu.index}:{npu.chip_id}
              </Badge>
              <div className="truncate text-sm font-semibold leading-5" title={npu.name}>
                {npu.name || t("ascendNpuMonitor.unknownNpu")}
              </div>
            </div>
          </div>
          <div className="flex shrink-0 items-center gap-1">
            <Badge variant="outline" className="px-1.5 text-[0.625rem]">
              {npu.health || "-"}
            </Badge>
            <Tooltip>
              <CollapsibleTrigger asChild>
                <TooltipTrigger asChild>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-6 w-6 rounded-md text-muted-foreground hover:text-foreground"
                    aria-label={t("ascendNpuMonitor.details")}
                  >
                    <ChevronRight
                      className={cn("h-3.5 w-3.5 transition-transform", detailsOpen && "rotate-90")}
                    />
                  </Button>
                </TooltipTrigger>
              </CollapsibleTrigger>
              <TooltipContent side="top">{t("ascendNpuMonitor.details")}</TooltipContent>
            </Tooltip>
          </div>
        </div>

        <div className="mt-2.5 space-y-2">
          <MetricBar label={t("ascendNpuMonitor.aicoreUtilization")} value={aicoreUtilization} />
          <MetricBar
            label={t("ascendNpuMonitor.memoryUtilization", {
              kind: npu.memory_kind || t("ascendNpuMonitor.memoryKind"),
            })}
            detail={
              npu.memory_total_mb > 0
                ? `${formatMemoryMb(npu.memory_used_mb)} / ${formatMemoryMb(npu.memory_total_mb)}`
                : "-"
            }
            value={memoryPercent}
          />
        </div>

        <CollapsibleContent className="mt-1 grid grid-cols-2 gap-1.5 overflow-hidden text-[0.6875rem] data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:animate-in data-[state=open]:fade-in-0">
          <NpuMetric
            icon={<Fingerprint className="h-3.5 w-3.5" />}
            label={t("ascendNpuMonitor.device")}
            value={npu.device_key || "-"}
            className="col-span-2"
            valueTitle={npu.device_key}
          />
          <NpuMetric
            icon={<Gauge className="h-3.5 w-3.5" />}
            label={t("ascendNpuMonitor.busId")}
            value={npu.bus_id || "-"}
          />
          <NpuMetric
            icon={<Gauge className="h-3.5 w-3.5" />}
            label={t("ascendNpuMonitor.physicalId")}
            value={npu.physical_id == null ? "-" : npu.physical_id.toString()}
          />
          <NpuMetric
            icon={<Thermometer className="h-3.5 w-3.5" />}
            label={t("ascendNpuMonitor.temperature")}
            value={npu.temperature_c == null ? "-" : `${Math.round(npu.temperature_c)} C`}
          />
          <NpuMetric
            icon={<Zap className="h-3.5 w-3.5" />}
            label={t("ascendNpuMonitor.power")}
            value={formatWatts(npu.power_draw_w)}
          />
          <NpuMetric
            icon={<Gauge className="h-3.5 w-3.5" />}
            label={t("ascendNpuMonitor.memoryFree")}
            value={npu.memory_total_mb > 0 ? formatMemoryMb(npu.memory_free_mb) : "-"}
          />
        </CollapsibleContent>
      </div>
    </Collapsible>
  );
}

function MetricBar({
  detail,
  label,
  value,
}: {
  detail?: string;
  label: string;
  value: number | null;
}) {
  const safeValue = value == null ? 0 : clampPercent(value);
  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between gap-2 text-[0.6875rem]">
        <span className="text-muted-foreground">{label}</span>
        <span className="font-mono text-muted-foreground">
          {detail ?? formatOptionalPercent(value)}
        </span>
      </div>
      <div className="h-1.5 overflow-hidden rounded-full bg-muted">
        <div
          className={cn(
            "h-full rounded-full transition-all duration-700",
            safeValue >= 90 ? "bg-red-500" : safeValue >= 70 ? "bg-amber-400" : "bg-emerald-500",
          )}
          style={{ width: `${safeValue}%` }}
        />
      </div>
    </div>
  );
}

function NpuMetric({
  className,
  icon,
  label,
  value,
  valueTitle,
}: {
  className?: string;
  icon: ReactNode;
  label: string;
  value: string;
  valueTitle?: string;
}) {
  return (
    <div
      className={cn(
        "flex min-w-0 items-center gap-1.5 rounded-md bg-muted/25 px-2 py-1.5",
        className,
      )}
    >
      <span className="shrink-0 text-muted-foreground">{icon}</span>
      <span className="min-w-0 flex-1 truncate text-muted-foreground">{label}</span>
      <span className="min-w-0 truncate text-right font-mono" title={valueTitle ?? value}>
        {value}
      </span>
    </div>
  );
}

function VirtualNpuProcessList({
  processes,
}: {
  processes: RemoteNpuProcess[];
}) {
  const listHeight = Math.min(
    processes.length * NPU_PROCESS_ROW_HEIGHT,
    NPU_PROCESS_LIST_MAX_HEIGHT,
  );
  const { containerRef, visibleItems, paddingTop, paddingBottom, onScroll } = useVirtualList(
    processes,
    {
      itemHeight: NPU_PROCESS_ROW_HEIGHT,
      overscan: 6,
    },
  );

  return (
    <div
      ref={containerRef}
      className="overflow-y-auto terminal-scroll"
      style={{ height: listHeight }}
      onScroll={onScroll}
    >
      <div style={{ paddingTop, paddingBottom }}>
        {visibleItems.map(({ item: process, index }) => (
          <div
            key={`${process.device_key}-${process.pid}-${index}`}
            className="pb-1.5"
            style={{ height: NPU_PROCESS_ROW_HEIGHT }}
          >
            <NpuProcessRow process={process} />
          </div>
        ))}
      </div>
    </div>
  );
}

function NpuProcessRow({ process }: { process: RemoteNpuProcess }) {
  const { t } = useTranslation();
  return (
    <div className="flex h-full items-center gap-2 rounded-md border bg-background/30 px-2.5 py-2 transition-colors hover:bg-sky-500/[0.06]">
      <div className="min-w-0 flex-1">
        <div className="truncate text-xs font-semibold" title={process.process_name}>
          {process.process_name || "-"}
        </div>
        <div className="mt-0.5 flex min-w-0 items-center gap-1.5 font-mono text-[0.625rem] text-muted-foreground">
          <span>
            {t("ascendNpuMonitor.pid")} {process.pid}
          </span>
          <span>
            {t("ascendNpuMonitor.npu")} {process.npu_index}:{process.chip_id}
          </span>
        </div>
      </div>
      <span className="shrink-0 font-mono text-xs">{formatMemoryMb(process.used_memory_mb)}</span>
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

function LoadingState({ label }: { label: string }) {
  return (
    <div className="flex h-full items-center justify-center">
      <RefreshCw className="h-5 w-5 animate-spin text-muted-foreground" aria-label={label} />
    </div>
  );
}

interface NpuSummary {
  npuCount: number;
  maxAicore: number | null;
  maxTemperature: number | null;
  memoryTotalMb: number;
  memoryUsedMb: number;
}

function buildNpuSummary(overview: RemoteNpuOverview | null): NpuSummary {
  const npus = overview?.npus ?? [];
  const temperatures = npus
    .map((npu) => npu.temperature_c)
    .filter((value): value is number => value != null);
  const aicoreValues = npus
    .map((npu) => npu.utilization_aicore_percent)
    .filter((value): value is number => value != null);

  return {
    npuCount: npus.length,
    maxAicore: aicoreValues.length > 0 ? Math.max(...aicoreValues) : null,
    maxTemperature: temperatures.length > 0 ? Math.max(...temperatures) : null,
    memoryTotalMb: npus.reduce((total, npu) => total + npu.memory_total_mb, 0),
    memoryUsedMb: npus.reduce((total, npu) => total + npu.memory_used_mb, 0),
  };
}

function processMatches(process: RemoteNpuProcess, query: string) {
  if (!query) return true;
  const haystack =
    `${process.pid} ${process.npu_index} ${process.chip_id} ${process.device_key} ${process.process_name}`.toLowerCase();
  return haystack.includes(query);
}

function clampPercent(value: number) {
  return Math.min(100, Math.max(0, value));
}

function formatOptionalPercent(value: number | null) {
  return value == null ? "-" : `${Math.round(clampPercent(value))}%`;
}

function formatMemoryMb(value: number) {
  if (value >= 1024) {
    const gib = value / 1024;
    return `${gib < 10 ? gib.toFixed(1) : Math.round(gib)} GiB`;
  }
  return `${Math.round(value)} MiB`;
}

function formatWatts(value?: number | null) {
  if (value == null) return "-";
  return `${value < 100 ? value.toFixed(1) : Math.round(value)} W`;
}
