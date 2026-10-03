import { openUrl } from "@/lib/backend/platform/opener";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { MdAdd, MdDelete, MdEdit, MdMoreHoriz, MdOpenInNew } from "react-icons/md";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent } from "@/components/ui/collapsible";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { useApp } from "@/context/AppContext";
import { cn } from "@/lib/utils";
import type { SearchEngine } from "@/types/global";
import { type QuickIconDef, SEARCH_ICONS } from "../icons";
import { SettingSection } from "./SettingFormItems";

let searchEngineListKeySeed = 0;

function createSearchEngineListKey() {
  searchEngineListKeySeed += 1;
  return `search-engine-${searchEngineListKeySeed}`;
}

type SearchEngineListItemProps = {
  engine: SearchEngine;
  index: number;
  isOpen: boolean;
  onDelete: (index: number) => void;
  onOpenChange: (index: number, open: boolean) => void;
  onPatch: (index: number, patch: Partial<SearchEngine>) => void;
  onTest: (engine: SearchEngine) => void;
};

function SearchEngineListItem({
  engine,
  index,
  isOpen,
  onDelete,
  onOpenChange,
  onPatch,
  onTest,
}: SearchEngineListItemProps) {
  const { t } = useTranslation();
  const hasQueryPlaceholder = engine.url_template.includes("%s");

  return (
    <Collapsible open={isOpen} onOpenChange={(open) => onOpenChange(index, open)}>
      <div className="px-3 py-3 sm:px-4">
        <div className="flex items-start gap-3">
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button
                type="button"
                className="mt-0.5 flex h-9 w-9 shrink-0 items-center justify-center rounded-lg border border-border/70 bg-background transition-colors hover:bg-secondary"
                title={t("settings.selectIcon")}
              >
                {engine.icon && SEARCH_ICONS[engine.icon] ? (
                  (() => {
                    const Icon = SEARCH_ICONS[engine.icon].icon;
                    return (
                      <Icon
                        className="text-base"
                        style={{ color: SEARCH_ICONS[engine.icon].color }}
                      />
                    );
                  })()
                ) : (
                  <MdAdd className="text-sm text-muted-foreground" />
                )}
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start" className="z-50 w-48 max-w-[calc(100vw-2rem)] p-2">
              <div className="grid max-h-48 grid-cols-6 gap-1 overflow-y-auto terminal-scroll">
                <DropdownMenuItem
                  className="flex cursor-pointer items-center justify-center rounded p-1 text-xs text-muted-foreground hover:bg-secondary"
                  onSelect={() => onPatch(index, { icon: undefined })}
                  title="Clear icon"
                >
                  ✕
                </DropdownMenuItem>
                {Object.entries(SEARCH_ICONS).map(([name, iconDef]) => {
                  const Icon = (iconDef as QuickIconDef).icon;
                  const color = (iconDef as QuickIconDef).color;
                  return (
                    <DropdownMenuItem
                      key={name}
                      className="flex cursor-pointer items-center justify-center rounded p-1 hover:bg-secondary"
                      onSelect={() => onPatch(index, { icon: name })}
                      title={name}
                    >
                      <Icon className="text-base" style={{ color }} />
                    </DropdownMenuItem>
                  );
                })}
              </div>
            </DropdownMenuContent>
          </DropdownMenu>

          <div className="min-w-0 flex-1 px-1 py-0.5 text-left">
            <div className="flex min-w-0 items-start justify-between gap-3">
              <div className="min-w-0 flex-1">
                <div className="truncate text-sm font-medium">
                  {engine.name || t("settings.engineName")}
                </div>
                <div
                  className={cn(
                    "mt-1 truncate text-xs",
                    hasQueryPlaceholder ? "text-muted-foreground" : "text-destructive",
                  )}
                >
                  {engine.url_template || t("settings.engineUrl")}
                </div>
              </div>
            </div>
          </div>

          <div className="flex shrink-0 items-center gap-1 sm:gap-2">
            <label
              className="inline-flex cursor-pointer items-center gap-2 text-xs text-muted-foreground"
              title={t("settings.showInSearchMenu")}
            >
              <Switch
                size="sm"
                checked={engine.show_in_menu !== false}
                onCheckedChange={(checked) => onPatch(index, { show_in_menu: checked })}
              />
            </label>

            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  className="text-muted-foreground hover:text-foreground"
                  title={t("settings.searchEngineActions")}
                >
                  <MdMoreHoriz className="text-[1rem]" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuItem onSelect={() => onOpenChange(index, !isOpen)}>
                  <MdEdit className="text-[0.95rem]" />
                  {t("common.edit")}
                </DropdownMenuItem>
                <DropdownMenuItem disabled={!hasQueryPlaceholder} onSelect={() => onTest(engine)}>
                  <MdOpenInNew className="text-[0.95rem]" />
                  {t("settings.testSearchEngine")}
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem variant="destructive" onSelect={() => onDelete(index)}>
                  <MdDelete className="text-[0.95rem]" />
                  {t("common.delete")}
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        </div>

        <CollapsibleContent className="overflow-hidden data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:animate-in data-[state=open]:fade-in-0">
          <div className="mt-3 space-y-3 border-t border-border/60 pt-3">
            <div className="space-y-2">
              <Label
                htmlFor={`search-engine-name-${index}`}
                className="text-xs font-medium text-muted-foreground"
              >
                {t("settings.engineName")}
              </Label>
              <Input
                id={`search-engine-name-${index}`}
                placeholder={t("settings.engineName")}
                className="text-sm"
                value={engine.name}
                onChange={(event) => onPatch(index, { name: event.target.value })}
              />
            </div>

            <div className="space-y-2">
              <Label
                htmlFor={`search-engine-url-${index}`}
                className="text-xs font-medium text-muted-foreground"
              >
                {t("settings.engineUrl")}
              </Label>
              <Input
                id={`search-engine-url-${index}`}
                placeholder="https://google.com/search?q=%s"
                className="text-sm"
                aria-invalid={!hasQueryPlaceholder}
                value={engine.url_template}
                onChange={(event) => onPatch(index, { url_template: event.target.value })}
              />
              <p
                className={cn(
                  "text-xs",
                  hasQueryPlaceholder ? "text-muted-foreground" : "text-destructive",
                )}
              >
                {hasQueryPlaceholder ? t("settings.engineUrlHint") : t("settings.engineUrlInvalid")}
              </p>
            </div>
          </div>
        </CollapsibleContent>
      </div>
    </Collapsible>
  );
}

export function SearchTab() {
  const { t } = useTranslation();
  const { appSettings, updateAppSettings } = useApp();
  const [expandedIndex, setExpandedIndex] = useState<number | null>(null);
  const [pendingFocusIndex, setPendingFocusIndex] = useState<number | null>(null);
  const engineCount = appSettings.search.custom_engines.length;
  const engineListKeysRef = useRef<string[]>([]);

  if (engineListKeysRef.current.length !== engineCount) {
    engineListKeysRef.current = appSettings.search.custom_engines.map(
      (_, index) => engineListKeysRef.current[index] ?? createSearchEngineListKey(),
    );
  }

  useEffect(() => {
    if (pendingFocusIndex === null || engineCount === 0 || expandedIndex !== pendingFocusIndex) {
      return;
    }

    const input = document.getElementById(
      `search-engine-name-${pendingFocusIndex}`,
    ) as HTMLInputElement | null;

    if (!input) return;

    requestAnimationFrame(() => {
      input.focus();
      input.select();
      input.scrollIntoView({ behavior: "smooth", block: "center" });
      setPendingFocusIndex(null);
    });
  }, [engineCount, expandedIndex, pendingFocusIndex]);

  function updateEngines(nextEngines: SearchEngine[]) {
    updateAppSettings({ search: { ...appSettings.search, custom_engines: nextEngines } });
  }

  function patchEngine(index: number, patch: Partial<SearchEngine>) {
    const nextEngines = [...appSettings.search.custom_engines];
    nextEngines[index] = { ...nextEngines[index], ...patch };
    updateEngines(nextEngines);
  }

  function addEngine() {
    setExpandedIndex(0);
    setPendingFocusIndex(0);
    engineListKeysRef.current = [createSearchEngineListKey(), ...engineListKeysRef.current];
    updateEngines([
      {
        name: "New Engine",
        url_template: "https://example.com/search?q=%s",
        show_in_menu: true,
      },
      ...appSettings.search.custom_engines,
    ]);
  }

  function removeEngine(index: number) {
    engineListKeysRef.current = engineListKeysRef.current.filter(
      (_, currentIndex) => currentIndex !== index,
    );
    updateEngines(
      appSettings.search.custom_engines.filter((_, currentIndex) => currentIndex !== index),
    );

    setExpandedIndex((current) => {
      if (current === null) return current;
      if (current === index) return null;
      return current > index ? current - 1 : current;
    });
  }

  function handleOpenChange(index: number, open: boolean) {
    setExpandedIndex(open ? index : expandedIndex === index ? null : expandedIndex);
  }

  function testEngine(engine: SearchEngine) {
    if (!engine.url_template.includes("%s")) return;
    openUrl(engine.url_template.replace("%s", encodeURIComponent("nyaterm")));
  }

  return (
    <div className="space-y-5">
      <SettingSection
        title={t("settings.customEngines")}
        desc={t("settings.engineUrlHint")}
        action={
          <Button variant="ghost" size="xs" className="text-primary" onClick={addEngine}>
            <MdAdd className="text-[0.875rem]" />
            {t("common.add")}
          </Button>
        }
        contentClassName="space-y-3"
      >
        {appSettings.search.custom_engines.length > 0 ? (
          <div className="overflow-hidden rounded-xl border border-border/70 bg-background/75 divide-y divide-border/60">
            {appSettings.search.custom_engines.map((engine, index) => (
              <SearchEngineListItem
                key={engineListKeysRef.current[index]}
                engine={engine}
                index={index}
                isOpen={expandedIndex === index}
                onDelete={removeEngine}
                onOpenChange={handleOpenChange}
                onPatch={patchEngine}
                onTest={testEngine}
              />
            ))}
          </div>
        ) : (
          <div className="rounded-xl border border-dashed border-border/70 bg-background/40 px-4 py-8 text-center text-sm text-muted-foreground">
            {t("settings.noCustomEngines")}
          </div>
        )}
      </SettingSection>
    </div>
  );
}
