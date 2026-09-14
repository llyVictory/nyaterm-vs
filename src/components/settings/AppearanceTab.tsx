import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { GripVertical } from "lucide-react";
import { motion, Reorder, useDragControls } from "motion/react";
import {
  memo,
  type PointerEvent,
  type ReactNode,
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { MdAdd, MdClose, MdDeleteOutline, MdFolderOpen, MdImage, MdPalette } from "react-icons/md";
import { ThemeDesignerDialog } from "@/components/dialog/theme/ThemeDesignerDialog";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useApp } from "@/context/AppContext";
import { useSettingsDraft } from "@/context/SettingsDraftContext";
import { useTheme } from "@/context/ThemeContext";
import {
  BACKGROUND_IMAGE_FITS,
  clampOpacity,
  DEFAULT_BACKGROUND_CONTENT_OPACITY,
  DEFAULT_BACKGROUND_IMAGE_FIT,
  DEFAULT_BACKGROUND_IMAGE_OPACITY,
  DEFAULT_WINDOW_TRANSPARENCY_OPACITY,
  getWindowTransparencyOpacity,
  isBackgroundImageEnabled,
  normalizeBackgroundImageFit,
  windowTransparencyModeForOpacity,
} from "@/lib/backgroundImage";
import { invoke } from "@/lib/invoke";
import { logger } from "@/lib/logger";
import { isMacOS, isWindows } from "@/lib/platform";
import {
  DEFAULT_TERMINAL_FONT_SIZE,
  MAX_TERMINAL_FONT_SIZE,
  MIN_TERMINAL_FONT_SIZE,
} from "@/lib/terminalFontSize";
import type { AppearanceSettings } from "@/types/global";
import {
  SettingFieldGrid,
  SettingNumberInput,
  SettingRow,
  SettingSection,
  SettingSelect,
  SettingSwitch,
} from "./SettingFormItems";

interface FontInfo {
  family: string;
  monospace: boolean;
}

const PACKAGE_FONT_INFOS: FontInfo[] = [
  { family: "JetBrainsMono Nerd Font Mono", monospace: true },
  { family: "JetBrains Mono", monospace: true },
  { family: "Noto Sans SC Variable", monospace: false },
  { family: "Inter", monospace: false },
];
const TERMINAL_FALLBACK_FONT_OPTIONS = [
  "Cascadia Mono",
  "SF Mono",
  "Menlo",
  "Monaco",
  "Consolas",
  "Liberation Mono",
  "monospace",
];
const SYSTEM_UI_FONT_OPTIONS = [
  "system-ui",
  "-apple-system",
  "BlinkMacSystemFont",
  "Segoe UI",
  "PingFang SC",
  "Microsoft YaHei",
  "Noto Sans SC",
  "Noto Sans CJK SC",
  "Helvetica Neue",
  "Arial",
  "sans-serif",
  "serif",
  "monospace",
];
const UI_FALLBACK_FONT = "Inter";
const TERMINAL_FALLBACK_FONT = "JetBrainsMono Nerd Font Mono";
const GENERIC_FONT_FAMILIES = new Set([
  "serif",
  "sans-serif",
  "monospace",
  "cursive",
  "fantasy",
  "system-ui",
  "-apple-system",
  "blinkmacsystemfont",
]);
const PACKAGE_FONTS = PACKAGE_FONT_INFOS.map((font) => font.family);
const PACKAGE_BUILT_IN_FONTS = new Set(PACKAGE_FONTS.map((font) => font.toLowerCase()));
const TERMINAL_BUILT_IN_FONTS = new Set(
  PACKAGE_FONT_INFOS.filter((font) => font.monospace).map((font) => font.family.toLowerCase()),
);
const BACKGROUND_IMAGE_EXTENSIONS = ["png", "jpg", "jpeg", "webp", "bmp"];
const MINIMUM_CONTRAST_OPTIONS = [1, 3, 4.5, 7, 21] as const;
const TERMINAL_FONT_WEIGHT_OPTIONS = [300, 400, 500, 600, 700, 800] as const;
let cachedSystemFontInfos: FontInfo[] | null = null;
let systemFontInfosRequest: Promise<FontInfo[]> | null = null;

function splitFontStack(fontFamily: string) {
  return fontFamily
    .split(",")
    .map((font) => font.trim().replace(/^['"]|['"]$/g, ""))
    .filter(Boolean);
}

function mergeFontFamilies(...fontLists: string[][]) {
  const seen = new Set<string>();
  const merged: string[] = [];
  for (const font of fontLists.flat()) {
    const normalized = font.trim();
    if (!normalized) continue;
    const key = normalized.toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    merged.push(normalized);
  }
  return merged;
}

function requestSystemFontInfos() {
  if (cachedSystemFontInfos !== null) {
    return Promise.resolve(cachedSystemFontInfos);
  }

  if (!systemFontInfosRequest) {
    systemFontInfosRequest = invoke<FontInfo[]>("get_system_font_infos")
      .then((fonts) => {
        cachedSystemFontInfos = fonts;
        return fonts;
      })
      .catch((error) => {
        systemFontInfosRequest = null;
        logger.warn({
          domain: "ui.action",
          event: "system_font_infos_load_failed",
          message: "Failed to load system font list",
          error,
        });
        cachedSystemFontInfos = [];
        return cachedSystemFontInfos;
      });
  }

  return systemFontInfosRequest;
}

function InlineSpinner() {
  return (
    <span
      aria-hidden="true"
      className="size-3 shrink-0 animate-spin rounded-full border border-muted-foreground/30 border-t-muted-foreground"
    />
  );
}

function previewFontFamily(font: string, fallback: "sans-serif" | "monospace") {
  if (GENERIC_FONT_FAMILIES.has(font.toLowerCase())) {
    return font;
  }
  return `"${font}", ${fallback}`;
}

function percentLabel(value: number | null | undefined) {
  return `${Math.round(clampOpacity(value) * 100)}%`;
}

function PercentSlider({
  label,
  desc,
  value,
  disabled,
  onChange,
}: {
  label: string;
  desc?: string;
  value: number;
  disabled?: boolean;
  onChange: (value: number) => void;
}) {
  const percent = Math.round(clampOpacity(value) * 100);

  return (
    <div className="space-y-3">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <Label className="text-sm font-medium leading-5">{label}</Label>
          {desc && <p className="mt-1 text-xs leading-5 text-muted-foreground">{desc}</p>}
        </div>
        <span className="shrink-0 rounded-md border border-border/70 bg-background/60 px-2 py-1 font-mono text-xs text-muted-foreground">
          {percent}%
        </span>
      </div>
      <input
        type="range"
        min={0}
        max={100}
        step={1}
        value={percent}
        disabled={disabled}
        className="h-2 w-full cursor-pointer accent-primary disabled:cursor-not-allowed disabled:opacity-50"
        onChange={(event) => onChange(Number(event.target.value) / 100)}
      />
    </div>
  );
}

function BackgroundImageSection({
  appearance,
  onChange,
}: {
  appearance: AppearanceSettings;
  onChange: (patch: Partial<AppearanceSettings>) => void;
}) {
  const { t } = useTranslation();
  const hasImage = isBackgroundImageEnabled(appearance);

  const handleBrowse = async () => {
    const selected = await openDialog({
      directory: false,
      multiple: false,
      filters: [
        {
          name: t("settings.backgroundImageFiles"),
          extensions: BACKGROUND_IMAGE_EXTENSIONS,
        },
      ],
      title: t("settings.selectBackgroundImage"),
    });
    const selectedPath = Array.isArray(selected) ? selected[0] : selected;
    if (typeof selectedPath !== "string" || !selectedPath) return;

    onChange({
      background_image_path: selectedPath,
      background_image_fit: normalizeBackgroundImageFit(
        appearance.background_image_fit || DEFAULT_BACKGROUND_IMAGE_FIT,
      ),
      background_image_opacity:
        appearance.background_image_opacity ?? DEFAULT_BACKGROUND_IMAGE_OPACITY,
      ...(appearance.background_opacity >= 1
        ? { background_opacity: DEFAULT_BACKGROUND_CONTENT_OPACITY }
        : {}),
    });
  };

  return (
    <SettingSection
      title={t("settings.backgroundImage")}
      desc={t("settings.backgroundImageDesc")}
      contentClassName="space-y-5"
    >
      <div className="flex flex-col gap-2 sm:flex-row">
        <div className="flex min-h-9 min-w-0 flex-1 items-center rounded-md border border-border/70 bg-background/60 px-3 py-2 text-xs">
          {hasImage ? (
            <span className="truncate font-mono text-foreground/85">
              {appearance.background_image_path}
            </span>
          ) : (
            <span className="flex items-center gap-2 text-muted-foreground">
              <MdImage className="text-sm" />
              {t("settings.backgroundImageEmpty")}
            </span>
          )}
        </div>
        <Button
          variant="outline"
          size="sm"
          className="w-full shrink-0 gap-1.5 sm:w-auto"
          onClick={() => void handleBrowse()}
        >
          <MdFolderOpen className="text-sm" />
          {t("settings.selectBackgroundImage")}
        </Button>
        {hasImage && (
          <Button
            variant="ghost"
            size="sm"
            className="w-full shrink-0 gap-1.5 text-destructive hover:bg-destructive/10 sm:w-auto"
            onClick={() => onChange({ background_image_path: null })}
          >
            <MdDeleteOutline className="text-sm" />
            {t("settings.removeBackgroundImage")}
          </Button>
        )}
      </div>

      <SettingFieldGrid>
        <SettingSelect
          label={t("settings.backgroundImageFit")}
          desc={t("settings.backgroundImageFitDesc")}
          value={normalizeBackgroundImageFit(appearance.background_image_fit)}
          disabled={!hasImage}
          controlClassName="max-w-sm"
          onValueChange={(value) =>
            onChange({ background_image_fit: normalizeBackgroundImageFit(value) })
          }
        >
          {BACKGROUND_IMAGE_FITS.map((fit) => (
            <SelectItem key={fit} value={fit}>
              {t(`settings.backgroundImageFit_${fit}`)}
            </SelectItem>
          ))}
        </SettingSelect>

        <PercentSlider
          label={t("settings.backgroundImageOpacity")}
          desc={t("settings.backgroundImageOpacityDesc")}
          value={appearance.background_image_opacity ?? DEFAULT_BACKGROUND_IMAGE_OPACITY}
          disabled={!hasImage}
          onChange={(value) => onChange({ background_image_opacity: value })}
        />

        <PercentSlider
          label={t("settings.backgroundContentOpacity")}
          desc={t("settings.backgroundContentOpacityDesc", {
            value: percentLabel(DEFAULT_BACKGROUND_CONTENT_OPACITY),
          })}
          value={appearance.background_opacity}
          disabled={!hasImage}
          onChange={(value) => onChange({ background_opacity: value })}
        />
      </SettingFieldGrid>
    </SettingSection>
  );
}

interface FontStackSectionProps {
  title: string;
  desc: string;
  value: string;
  options: string[];
  builtInFonts: Set<string>;
  fallbackFont: string;
  previewFallback: "sans-serif" | "monospace";
  isLoadingOptions: boolean;
  hasLoadedOptions: boolean;
  onRequestOptions: () => void;
  onChange: (value: string) => void;
}

type SortableFontItem = {
  id: string;
  font: string;
};

const FONT_REORDER_TRANSITION = {
  layout: { type: "spring", stiffness: 620, damping: 42, mass: 0.72 },
} as const;
const FONT_DRAG_ACTIVE_ANIMATION = {
  scale: 1.015,
  boxShadow: "0 14px 30px rgb(0 0 0 / 0.18)",
} as const;
const FONT_DRAG_IDLE_ANIMATION = {
  scale: 1,
  boxShadow: "0 0 0 rgb(0 0 0 / 0)",
} as const;
const FONT_DRAG_TRANSITION = { duration: 0.12, ease: "easeOut" } as const;
const FONT_WHILE_DRAG = { zIndex: 20 } as const;

function haveSameFontOrder(items: SortableFontItem[], fonts: string[]) {
  return items.length === fonts.length && items.every((item, index) => item.font === fonts[index]);
}

function reconcileFontItems(
  currentItems: SortableFontItem[],
  fonts: string[],
  createItem: (font: string) => SortableFontItem,
) {
  const remainingItems = [...currentItems];
  return fonts.map((font) => {
    const existingIndex = remainingItems.findIndex((item) => item.font === font);
    if (existingIndex === -1) return createItem(font);
    return remainingItems.splice(existingIndex, 1)[0];
  });
}

interface SortableFontRowProps {
  item: SortableFontItem;
  index: number;
  itemCount: number;
  fontOptionLookup: ReadonlyMap<string, string>;
  fontOptionItems: ReactNode;
  previewFallback: "sans-serif" | "monospace";
  isLoadingOptions: boolean;
  hasLoadedOptions: boolean;
  isSelectOpen: boolean;
  isDragging: boolean;
  onRequestOptions: () => void;
  onSelectOpenChange: (id: string, open: boolean) => void;
  onFontChange: (id: string, font: string) => void;
  onRemove: (id: string) => void;
  onDragStart: (id: string) => void;
  onDragEnd: (id: string) => void;
}

interface FontSelectControlProps {
  item: SortableFontItem;
  fontOptionLookup: ReadonlyMap<string, string>;
  fontOptionItems: ReactNode;
  previewFallback: "sans-serif" | "monospace";
  isLoadingOptions: boolean;
  hasLoadedOptions: boolean;
  isSelectOpen: boolean;
  onRequestOptions: () => void;
  onSelectOpenChange: (id: string, open: boolean) => void;
  onFontChange: (id: string, font: string) => void;
}

const FontSelectControl = memo(function FontSelectControl({
  item,
  fontOptionLookup,
  fontOptionItems,
  previewFallback,
  isLoadingOptions,
  hasLoadedOptions,
  isSelectOpen,
  onRequestOptions,
  onSelectOpenChange,
  onFontChange,
}: FontSelectControlProps) {
  const { t } = useTranslation();
  const selectedFont = fontOptionLookup.get(item.font.toLowerCase());
  const selectValue = selectedFont ?? item.font;
  const isKnownFont = selectedFont !== undefined;
  const showLoading = isSelectOpen && isLoadingOptions;
  const showUnknownMarker = isSelectOpen && hasLoadedOptions && !isKnownFont;
  const handleOpenChange = useCallback(
    (open: boolean) => {
      onSelectOpenChange(item.id, open);
      if (open) onRequestOptions();
    },
    [item.id, onRequestOptions, onSelectOpenChange],
  );
  const handleValueChange = useCallback(
    (font: string) => onFontChange(item.id, font),
    [item.id, onFontChange],
  );

  return (
    <Select value={selectValue} onOpenChange={handleOpenChange} onValueChange={handleValueChange}>
      <SelectTrigger
        className="h-9 min-w-0 w-full flex-1 px-3 text-sm shadow-xs focus:ring-1 focus:ring-ring focus:outline-none"
        style={{ fontFamily: previewFontFamily(item.font, previewFallback) }}
      >
        <SelectValue />
      </SelectTrigger>
      <SelectContent position="popper">
        {showLoading && (
          <output
            aria-live="polite"
            className="flex items-center gap-2 px-2 py-2 text-xs text-muted-foreground"
          >
            <InlineSpinner />
            {t("settings.loadingSystemFonts")}
          </output>
        )}
        {!isKnownFont && (
          <SelectItem
            value={item.font}
            disabled
            style={{ fontFamily: previewFontFamily(item.font, previewFallback) }}
          >
            {item.font}
            {showUnknownMarker && " (Custom/Missing)"}
          </SelectItem>
        )}
        {fontOptionItems}
      </SelectContent>
    </Select>
  );
});

const SortableFontRow = memo(function SortableFontRow({
  item,
  index,
  itemCount,
  fontOptionLookup,
  fontOptionItems,
  previewFallback,
  isLoadingOptions,
  hasLoadedOptions,
  isSelectOpen,
  isDragging,
  onRequestOptions,
  onSelectOpenChange,
  onFontChange,
  onRemove,
  onDragStart,
  onDragEnd,
}: SortableFontRowProps) {
  const { t } = useTranslation();
  const dragControls = useDragControls();
  const handleDragStart = useCallback(() => onDragStart(item.id), [item.id, onDragStart]);
  const handleDragEnd = useCallback(() => onDragEnd(item.id), [item.id, onDragEnd]);
  const handlePointerDown = useCallback(
    (event: PointerEvent<HTMLButtonElement>) => {
      if (itemCount < 2) return;
      event.preventDefault();
      dragControls.start(event);
    },
    [dragControls, itemCount],
  );
  const handleRemove = useCallback(() => onRemove(item.id), [item.id, onRemove]);

  return (
    <Reorder.Item
      as="div"
      value={item}
      layout="position"
      dragListener={false}
      dragControls={dragControls}
      dragMomentum={false}
      onDragStart={handleDragStart}
      onDragEnd={handleDragEnd}
      whileDrag={FONT_WHILE_DRAG}
      transition={FONT_REORDER_TRANSITION}
      className="relative will-change-transform"
    >
      <motion.div
        animate={isDragging ? FONT_DRAG_ACTIVE_ANIMATION : FONT_DRAG_IDLE_ANIMATION}
        transition={FONT_DRAG_TRANSITION}
        className={`rounded-lg border bg-background/70 p-3 ${
          isDragging
            ? "border-primary/60 bg-background cursor-grabbing will-change-transform"
            : "border-border/70"
        }`}
      >
        <div className="flex items-center gap-2">
          <Tooltip>
            <TooltipTrigger asChild>
              <span className="inline-flex shrink-0">
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-sm"
                  className="touch-none cursor-grab text-muted-foreground hover:text-foreground active:cursor-grabbing"
                  disabled={itemCount < 2}
                  aria-label={t("settings.fontDragToSort")}
                  onPointerDown={handlePointerDown}
                >
                  <GripVertical className="size-4" />
                </Button>
              </span>
            </TooltipTrigger>
            <TooltipContent side="top">{t("settings.fontDragToSort")}</TooltipContent>
          </Tooltip>
          <div className="flex min-w-0 flex-1 flex-col gap-3 sm:flex-row sm:items-center">
            <div className="min-w-0 sm:w-32 sm:shrink-0">
              <p className="text-xs font-medium text-muted-foreground">
                {index === 0 ? t("settings.fontPrimary") : `${t("settings.fontFallback")} ${index}`}
              </p>
            </div>
            <FontSelectControl
              item={item}
              fontOptionLookup={fontOptionLookup}
              fontOptionItems={fontOptionItems}
              previewFallback={previewFallback}
              isLoadingOptions={isLoadingOptions}
              hasLoadedOptions={hasLoadedOptions}
              isSelectOpen={isSelectOpen}
              onRequestOptions={onRequestOptions}
              onSelectOpenChange={onSelectOpenChange}
              onFontChange={onFontChange}
            />
            <Button
              variant="ghost"
              size="icon-xs"
              className="self-end text-destructive hover:bg-destructive/10 sm:self-auto"
              title={t("common.remove")}
              onClick={handleRemove}
            >
              <MdClose className="text-[1rem]" />
            </Button>
          </div>
        </div>
      </motion.div>
    </Reorder.Item>
  );
});

const FontStackSection = memo(function FontStackSection({
  title,
  desc,
  value,
  options,
  builtInFonts,
  fallbackFont,
  previewFallback,
  isLoadingOptions,
  hasLoadedOptions,
  onRequestOptions,
  onChange,
}: FontStackSectionProps) {
  const { t } = useTranslation();
  const fontOptionLookup = useMemo(
    () => new Map(options.map((option) => [option.toLowerCase(), option])),
    [options],
  );
  const fontOptionItems = useMemo(
    () =>
      options.map((option) => (
        <SelectItem
          key={option}
          value={option}
          style={{ fontFamily: previewFontFamily(option, previewFallback) }}
        >
          {option} {builtInFonts.has(option.toLowerCase()) && `(${t("settings.fontBuiltIn")})`}
        </SelectItem>
      )),
    [builtInFonts, options, previewFallback, t],
  );
  const itemIdPrefix = useId();
  const nextItemIdRef = useRef(0);
  const createFontItem = useCallback(
    (font: string): SortableFontItem => ({
      id: `${itemIdPrefix}-${nextItemIdRef.current++}`,
      font,
    }),
    [itemIdPrefix],
  );
  const [fontItems, setFontItems] = useState<SortableFontItem[]>(() => {
    const fonts = splitFontStack(value);
    return (fonts.length > 0 ? fonts : [fallbackFont]).map(createFontItem);
  });
  const fontItemsRef = useRef(fontItems);
  const [openFontId, setOpenFontId] = useState<string | null>(null);
  const [draggingFontId, setDraggingFontId] = useState<string | null>(null);
  const draggingFontIdRef = useRef<string | null>(null);
  const pendingFontChangeFrameRef = useRef<number | null>(null);
  const pendingFontChangeSecondFrameRef = useRef<number | null>(null);
  const pendingFontChangeValueRef = useRef<string | null>(null);

  useEffect(() => {
    if (draggingFontIdRef.current !== null || pendingFontChangeValueRef.current !== null) {
      return;
    }

    const fonts = splitFontStack(value);
    const normalizedFonts = fonts.length > 0 ? fonts : [fallbackFont];
    // fontItemsRef is kept in sync with the fontItems state by every mutation
    // path (applyFontItems, handleReorder, and this effect), so reading it in
    // the effect body is safe and keeps the setState updater pure.
    const currentItems = fontItemsRef.current;
    if (haveSameFontOrder(currentItems, normalizedFonts)) return;

    const nextItems = reconcileFontItems(currentItems, normalizedFonts, createFontItem);
    fontItemsRef.current = nextItems;
    setFontItems(nextItems);
  }, [createFontItem, fallbackFont, value]);

  useEffect(() => {
    if (draggingFontId === null) return;

    const previousCursor = document.documentElement.style.cursor;
    const previousUserSelect = document.documentElement.style.userSelect;
    document.documentElement.style.cursor = "grabbing";
    document.documentElement.style.userSelect = "none";
    return () => {
      document.documentElement.style.cursor = previousCursor;
      document.documentElement.style.userSelect = previousUserSelect;
    };
  }, [draggingFontId]);

  const cancelPendingFontChange = useCallback(() => {
    if (pendingFontChangeFrameRef.current !== null) {
      window.cancelAnimationFrame(pendingFontChangeFrameRef.current);
      pendingFontChangeFrameRef.current = null;
    }
    if (pendingFontChangeSecondFrameRef.current !== null) {
      window.cancelAnimationFrame(pendingFontChangeSecondFrameRef.current);
      pendingFontChangeSecondFrameRef.current = null;
    }
    pendingFontChangeValueRef.current = null;
  }, []);

  const scheduleFontChange = useCallback(
    (nextItems: SortableFontItem[]) => {
      cancelPendingFontChange();
      pendingFontChangeValueRef.current = nextItems.map((item) => item.font).join(", ");

      // Let Motion finish the release and the last layout reorder before updating
      // the settings draft. The second frame prevents both updates from landing
      // in the same rendering cycle when the pointer is released during a reorder.
      pendingFontChangeFrameRef.current = window.requestAnimationFrame(() => {
        pendingFontChangeFrameRef.current = null;
        pendingFontChangeSecondFrameRef.current = window.requestAnimationFrame(() => {
          pendingFontChangeSecondFrameRef.current = null;
          const nextValue = pendingFontChangeValueRef.current;
          pendingFontChangeValueRef.current = null;
          if (nextValue !== null) onChange(nextValue);
        });
      });
    },
    [cancelPendingFontChange, onChange],
  );

  useEffect(() => cancelPendingFontChange, [cancelPendingFontChange]);

  const applyFontItems = useCallback(
    (nextItems: SortableFontItem[]) => {
      cancelPendingFontChange();
      fontItemsRef.current = nextItems;
      setFontItems(nextItems);
      onChange(nextItems.map((item) => item.font).join(", "));
    },
    [cancelPendingFontChange, onChange],
  );

  const handleReorder = useCallback((nextItems: SortableFontItem[]) => {
    fontItemsRef.current = nextItems;
    setFontItems(nextItems);
  }, []);

  const handleFontDragStart = useCallback((id: string) => {
    draggingFontIdRef.current = id;
    setDraggingFontId(id);
    setOpenFontId(null);
  }, []);

  const handleFontDragEnd = useCallback(
    (id: string) => {
      if (draggingFontIdRef.current !== id) return;

      draggingFontIdRef.current = null;
      setDraggingFontId(null);

      const nextItems = fontItemsRef.current;
      const fonts = splitFontStack(value);
      const normalizedFonts = fonts.length > 0 ? fonts : [fallbackFont];
      if (!haveSameFontOrder(nextItems, normalizedFonts)) {
        scheduleFontChange(nextItems);
      }
    },
    [fallbackFont, scheduleFontChange, value],
  );

  const updateFont = useCallback(
    (id: string, font: string) => {
      applyFontItems(
        fontItemsRef.current.map((item) => (item.id === id ? { ...item, font } : item)),
      );
    },
    [applyFontItems],
  );

  const removeFont = useCallback(
    (id: string) => {
      let nextItems = fontItemsRef.current.filter((item) => item.id !== id);
      if (nextItems.length === 0) nextItems = [createFontItem(fallbackFont)];
      setOpenFontId((current) => (current === id ? null : current));
      applyFontItems(nextItems);
    },
    [applyFontItems, createFontItem, fallbackFont],
  );

  const handleSelectOpenChange = useCallback((id: string, open: boolean) => {
    setOpenFontId((current) => {
      if (open) return id;
      return current === id ? null : current;
    });
  }, []);

  return (
    <SettingSection
      title={title}
      desc={desc}
      action={
        <Button
          variant="ghost"
          size="xs"
          className="text-primary"
          onClick={() => {
            applyFontItems([...fontItemsRef.current, createFontItem(options[0] || fallbackFont)]);
          }}
        >
          <MdAdd className="text-[0.875rem]" />
          {t("settings.addFallbackFont")}
        </Button>
      }
      contentClassName="space-y-0"
    >
      <Reorder.Group
        as="div"
        axis="y"
        values={fontItems}
        onReorder={handleReorder}
        className="flex flex-col gap-3"
      >
        {fontItems.map((item, index) => (
          <SortableFontRow
            key={item.id}
            item={item}
            index={index}
            itemCount={fontItems.length}
            fontOptionLookup={fontOptionLookup}
            fontOptionItems={fontOptionItems}
            previewFallback={previewFallback}
            isLoadingOptions={isLoadingOptions}
            hasLoadedOptions={hasLoadedOptions}
            isSelectOpen={openFontId === item.id}
            isDragging={draggingFontId === item.id}
            onRequestOptions={onRequestOptions}
            onSelectOpenChange={handleSelectOpenChange}
            onFontChange={updateFont}
            onRemove={removeFont}
            onDragStart={handleFontDragStart}
            onDragEnd={handleFontDragEnd}
          />
        ))}
      </Reorder.Group>
    </SettingSection>
  );
});

export function AppearanceTab() {
  const { t } = useTranslation();
  const { appSettings, updateAppSettings } = useApp();
  const { themeNames } = useTheme();
  const appearance = appSettings.appearance;
  const mountedRef = useRef(true);
  const [themeDesignerOpen, setThemeDesignerOpen] = useState(false);
  const [systemFontInfos, setSystemFontInfos] = useState<FontInfo[]>(
    () => cachedSystemFontInfos ?? [],
  );
  const [systemFontInfosLoading, setSystemFontInfosLoading] = useState(
    () => Boolean(systemFontInfosRequest) && cachedSystemFontInfos === null,
  );
  const applicationFonts = useMemo(
    () =>
      mergeFontFamilies(
        PACKAGE_FONTS,
        systemFontInfos.map((font) => font.family),
        SYSTEM_UI_FONT_OPTIONS,
      ),
    [systemFontInfos],
  );
  const hasLoadedSystemFontInfos = cachedSystemFontInfos !== null;
  const terminalFonts = useMemo(
    () =>
      mergeFontFamilies(
        PACKAGE_FONT_INFOS.filter((font) => font.monospace).map((font) => font.family),
        systemFontInfos.filter((font) => font.monospace).map((font) => font.family),
        TERMINAL_FALLBACK_FONT_OPTIONS,
      ),
    [systemFontInfos],
  );

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  const loadSystemFontInfos = useCallback(() => {
    if (cachedSystemFontInfos !== null) {
      setSystemFontInfos(cachedSystemFontInfos);
      setSystemFontInfosLoading(false);
      return;
    }

    setSystemFontInfosLoading(true);
    void requestSystemFontInfos().then((fonts) => {
      if (!mountedRef.current) return;
      setSystemFontInfos(fonts);
      setSystemFontInfosLoading(false);
    });
  }, []);

  const updateAppearance = useCallback(
    (
      patch:
        | Partial<AppearanceSettings>
        | ((prev: AppearanceSettings) => Partial<AppearanceSettings>),
    ) => {
      updateAppSettings((prev) => ({
        appearance: {
          ...prev.appearance,
          ...(typeof patch === "function" ? patch(prev.appearance) : patch),
        },
      }));
    },
    [updateAppSettings],
  );

  const { updateCommittedAppSettings } = useSettingsDraft();

  // Theme designer operations (import/save/delete) have immediate-effect
  // semantics: apply them to the settings draft AND the committed app settings
  // so they persist and appear in every theme list without restarting.
  const applyAppearance = useCallback(
    (
      patch:
        | Partial<AppearanceSettings>
        | ((prev: AppearanceSettings) => Partial<AppearanceSettings>),
    ) => {
      updateAppearance(patch);
      updateCommittedAppSettings((prev) => ({
        appearance: {
          ...prev.appearance,
          ...(typeof patch === "function" ? patch(prev.appearance) : patch),
        },
      }));
    },
    [updateAppearance, updateCommittedAppSettings],
  );
  const updateUiFontFamily = useCallback(
    (uiFontFamily: string) => updateAppearance({ ui_font_family: uiFontFamily }),
    [updateAppearance],
  );
  const updateTerminalFontFamily = useCallback(
    (fontFamily: string) => updateAppearance({ font_family: fontFamily }),
    [updateAppearance],
  );

  return (
    <div className="space-y-5">
      <SettingSection contentClassName="space-y-5">
        <SettingSelect
          label={t("settings.theme")}
          desc={t("settings.themeDesc")}
          value={appearance.theme || "github-dark"}
          onValueChange={(v) => updateAppearance({ theme: v })}
        >
          {themeNames.map((tm) => (
            <SelectItem key={tm.id} value={tm.id}>
              {tm.name}
            </SelectItem>
          ))}
        </SettingSelect>

        <SettingSelect
          label={t("settings.terminalTheme")}
          desc={t("settings.terminalThemeDesc")}
          value={appearance.terminal_theme || "__follow__"}
          onValueChange={(v) =>
            updateAppearance({
              terminal_theme: v === "__follow__" ? null : v,
            })
          }
        >
          <SelectItem value="__follow__">{t("settings.followUiTheme")}</SelectItem>
          {themeNames.map((tm) => (
            <SelectItem key={tm.id} value={tm.id}>
              {tm.name}
            </SelectItem>
          ))}
        </SettingSelect>

        <SettingRow
          label={t("settings.themeDesigner")}
          desc={t("settings.themeDesignerSettingDesc")}
        >
          <Button size="sm" variant="outline" onClick={() => setThemeDesignerOpen(true)}>
            <MdPalette className="text-sm" />
            {t("settings.themeDesignerOpen")}
          </Button>
        </SettingRow>

        <SettingSelect
          label={t("settings.minimumContrastRatio")}
          desc={t("settings.minimumContrastRatioDesc")}
          value={String(appearance.minimum_contrast_ratio ?? 1)}
          onValueChange={(v) =>
            updateAppearance({
              minimum_contrast_ratio: Number(v),
            })
          }
        >
          {MINIMUM_CONTRAST_OPTIONS.map((ratio) => (
            <SelectItem key={ratio} value={String(ratio)}>
              {t(`settings.minimumContrastRatio_${String(ratio).replace(".", "_")}`)}
            </SelectItem>
          ))}
        </SettingSelect>

        <SettingRow label={t("settings.panelMultiOpen")} desc={t("settings.panelMultiOpenDesc")}>
          <SettingSwitch
            checked={appearance.panel_multi_open}
            onChange={(v) => updateAppearance({ panel_multi_open: v })}
          />
        </SettingRow>
      </SettingSection>

      {(isWindows || isMacOS) && (
        <SettingSection
          title={t("settings.windowTransparency")}
          desc={t("settings.windowTransparencyDesc")}
          contentClassName="space-y-5"
        >
          <PercentSlider
            label={t("settings.windowTransparencyOpacity")}
            desc={t("settings.windowTransparencyOpacityDesc")}
            value={getWindowTransparencyOpacity(appearance) ?? DEFAULT_WINDOW_TRANSPARENCY_OPACITY}
            onChange={(value) =>
              updateAppearance({
                window_transparency_tint: value,
                window_transparency: windowTransparencyModeForOpacity(value),
              })
            }
          />
          {isWindows && (
            <SettingRow
              label={t("settings.windowTransparencyBlur")}
              desc={t("settings.windowTransparencyBlurDesc")}
            >
              <SettingSwitch
                checked={appearance.window_transparency_blur ?? false}
                onChange={(v) => updateAppearance({ window_transparency_blur: v })}
              />
            </SettingRow>
          )}
        </SettingSection>
      )}

      <BackgroundImageSection appearance={appearance} onChange={updateAppearance} />

      <FontStackSection
        title={t("settings.uiFontFamily")}
        desc={t("settings.uiFontFamilyDesc")}
        value={appearance.ui_font_family}
        options={applicationFonts}
        builtInFonts={PACKAGE_BUILT_IN_FONTS}
        fallbackFont={UI_FALLBACK_FONT}
        previewFallback="sans-serif"
        isLoadingOptions={systemFontInfosLoading}
        hasLoadedOptions={hasLoadedSystemFontInfos}
        onRequestOptions={loadSystemFontInfos}
        onChange={updateUiFontFamily}
      />

      <FontStackSection
        title={t("settings.terminalFontFamily")}
        desc={t("settings.terminalFontFamilyDesc")}
        value={appearance.font_family}
        options={terminalFonts}
        builtInFonts={TERMINAL_BUILT_IN_FONTS}
        fallbackFont={TERMINAL_FALLBACK_FONT}
        previewFallback="monospace"
        isLoadingOptions={systemFontInfosLoading}
        hasLoadedOptions={hasLoadedSystemFontInfos}
        onRequestOptions={loadSystemFontInfos}
        onChange={updateTerminalFontFamily}
      />

      <SettingSection contentClassName="space-y-5">
        <SettingFieldGrid>
          <SettingNumberInput
            label={t("settings.fontSize")}
            min={MIN_TERMINAL_FONT_SIZE}
            max={MAX_TERMINAL_FONT_SIZE}
            value={appearance.font_size}
            controlClassName="max-w-sm"
            onChange={(v) =>
              updateAppearance({
                font_size: v || DEFAULT_TERMINAL_FONT_SIZE,
              })
            }
          />
          <SettingSelect
            label={t("settings.terminalFontWeight")}
            desc={t("settings.terminalFontWeightDesc")}
            value={String(appearance.font_weight ?? 400)}
            controlClassName="max-w-sm"
            onValueChange={(v) => updateAppearance({ font_weight: Number(v) })}
          >
            {TERMINAL_FONT_WEIGHT_OPTIONS.map((weight) => (
              <SelectItem key={weight} value={String(weight)}>
                {t(`settings.fontWeight_${weight}`)}
              </SelectItem>
            ))}
          </SettingSelect>
          <SettingSelect
            label={t("settings.terminalFontWeightBold")}
            desc={t("settings.terminalFontWeightBoldDesc")}
            value={String(appearance.font_weight_bold ?? 700)}
            controlClassName="max-w-sm"
            onValueChange={(v) => updateAppearance({ font_weight_bold: Number(v) })}
          >
            {TERMINAL_FONT_WEIGHT_OPTIONS.map((weight) => (
              <SelectItem key={weight} value={String(weight)}>
                {t(`settings.fontWeight_${weight}`)}
              </SelectItem>
            ))}
          </SettingSelect>
          <SettingNumberInput
            label={t("settings.uiFontSize")}
            min={12}
            max={24}
            value={appearance.ui_font_size}
            controlClassName="max-w-sm"
            onChange={(v) =>
              updateAppearance({
                ui_font_size: v || 16,
              })
            }
          />
          <SettingSelect
            label={t("settings.cursorStyle")}
            value={appearance.cursor_style}
            controlClassName="max-w-sm"
            onValueChange={(v) => updateAppearance({ cursor_style: v })}
          >
            <SelectItem value="block">{t("settings.cursorBlock")}</SelectItem>
            <SelectItem value="underline">{t("settings.cursorUnderline")}</SelectItem>
            <SelectItem value="bar">{t("settings.cursorBar")}</SelectItem>
          </SettingSelect>
        </SettingFieldGrid>

        <SettingRow
          label={t("settings.boldDefaultForegroundHighlight")}
          desc={t("settings.boldDefaultForegroundHighlightDesc")}
        >
          <SettingSwitch
            checked={appearance.bold_default_foreground_highlight ?? false}
            onChange={(v) => updateAppearance({ bold_default_foreground_highlight: v })}
          />
        </SettingRow>

        <SettingRow label={t("settings.cursorBlink")}>
          <SettingSwitch
            checked={appearance.cursor_blink}
            onChange={(v) => updateAppearance({ cursor_blink: v })}
          />
        </SettingRow>
      </SettingSection>

      <ThemeDesignerDialog
        open={themeDesignerOpen}
        onClose={() => setThemeDesignerOpen(false)}
        appearance={appearance}
        availableThemes={themeNames}
        applyAppearance={applyAppearance}
      />
    </div>
  );
}
