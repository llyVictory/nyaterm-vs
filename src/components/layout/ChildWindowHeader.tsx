import { supports } from "@/lib/backend/runtime";
import { getCurrentWindow } from "@/lib/backend/platform/window";
import type { ReactNode } from "react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  VscChromeClose,
  VscChromeMaximize,
  VscChromeMinimize,
  VscChromeRestore,
  VscPin,
  VscPinned,
} from "react-icons/vsc";
import { isMacOS as osMacOS } from "@/lib/platform";
const isMacOS = osMacOS && supports("nativeWindows");

interface ChildWindowHeaderProps {
  title: string;
  onClose: () => void;
  icon?: ReactNode;
  windowControls?: boolean;
  alwaysOnTopControl?: boolean;
  macOSDragOnly?: boolean;
}

export default function ChildWindowHeader({
  title,
  onClose,
  icon,
  windowControls = false,
  alwaysOnTopControl = false,
  macOSDragOnly = false,
}: ChildWindowHeaderProps) {
  const { t } = useTranslation();
  const [appWindow] = useState(() => getCurrentWindow());
  const [isMaximized, setIsMaximized] = useState(false);
  const [isAlwaysOnTop, setIsAlwaysOnTop] = useState(false);

  useEffect(() => {
    if (isMacOS || (!windowControls && !alwaysOnTopControl)) return;
    let mounted = true;
    let unlistenResized: (() => void) | undefined;

    const syncWindowState = async () => {
      const [maximized, alwaysOnTop] = await Promise.all([
        windowControls ? appWindow.isMaximized().catch(() => false) : Promise.resolve(false),
        alwaysOnTopControl ? appWindow.isAlwaysOnTop().catch(() => false) : Promise.resolve(false),
      ]);
      if (mounted) {
        setIsMaximized(maximized);
        setIsAlwaysOnTop(alwaysOnTop);
      }
    };

    void syncWindowState();
    if (windowControls) {
      appWindow
        .onResized(() => {
          void syncWindowState();
        })
        .then((unlisten) => {
          unlistenResized = unlisten;
        })
        .catch(() => {});
    }

    return () => {
      mounted = false;
      unlistenResized?.();
    };
  }, [alwaysOnTopControl, appWindow, windowControls]);

  const handleMinimize = () => {
    appWindow.minimize().catch(() => {});
  };

  const handleToggleMaximize = async () => {
    await appWindow.toggleMaximize().catch(() => {});
    const maximized = await appWindow.isMaximized().catch(() => false);
    setIsMaximized(maximized);
  };

  const handleToggleAlwaysOnTop = async () => {
    const next = !isAlwaysOnTop;
    await appWindow.setAlwaysOnTop(next).catch(() => {});
    const alwaysOnTop = await appWindow.isAlwaysOnTop().catch(() => next);
    setIsAlwaysOnTop(alwaysOnTop);
  };

  const hideHeaderContent = isMacOS && macOSDragOnly;

  return (
    <header
      className="h-10 border-b flex items-center shrink-0 select-none"
      style={{ backgroundColor: "var(--df-bg-panel)", borderColor: "var(--df-border)" }}
    >
      {hideHeaderContent ? (
        <div className="h-full min-w-0 flex-1" data-tauri-drag-region />
      ) : (
        <div
          className={`flex-1 min-w-0 h-full flex items-center gap-2 px-3${isMacOS ? " pl-[84px]" : ""}`}
          data-tauri-drag-region
        >
          {icon ? (
            <span className="text-primary pointer-events-none shrink-0">
              {icon}
            </span>
          ) : null}
          <span className="text-sm font-medium truncate pointer-events-none">
            {title}
          </span>
        </div>
      )}

      {!isMacOS && (
        <div className="flex h-full shrink-0 items-center">
          {alwaysOnTopControl && supports("nativeWindows") && (
            <button
              type="button"
              className={`flex h-10 w-[46px] items-center justify-center transition-colors hover:bg-[color-mix(in_srgb,var(--df-text)_10%,transparent)] hover:text-[var(--df-text)] ${
                isAlwaysOnTop ? "text-[var(--df-primary)]" : "text-[var(--df-text-muted)]"
              }`}
              aria-label={isAlwaysOnTop ? t("menu.disableAlwaysOnTop") : t("menu.alwaysOnTop")}
              title={isAlwaysOnTop ? t("menu.disableAlwaysOnTop") : t("menu.alwaysOnTop")}
              aria-pressed={isAlwaysOnTop}
              onClick={() => void handleToggleAlwaysOnTop()}
            >
              {isAlwaysOnTop ? (
                <VscPinned className="text-base" />
              ) : (
                <VscPin className="text-base" />
              )}
            </button>
          )}
          {windowControls && supports("nativeWindows") && (
            <>
              <button
                type="button"
                className="flex h-10 w-[46px] items-center justify-center text-[var(--df-text-muted)] transition-colors hover:bg-[color-mix(in_srgb,var(--df-text)_10%,transparent)] hover:text-[var(--df-text)]"
                aria-label={t("menu.minimize")}
                title={t("menu.minimize")}
                onClick={handleMinimize}
              >
                <VscChromeMinimize className="text-base" />
              </button>
              <button
                type="button"
                className="flex h-10 w-[46px] items-center justify-center text-[var(--df-text-muted)] transition-colors hover:bg-[color-mix(in_srgb,var(--df-text)_10%,transparent)] hover:text-[var(--df-text)]"
                aria-label={isMaximized ? t("menu.restore") : t("menu.maximize")}
                title={isMaximized ? t("menu.restore") : t("menu.maximize")}
                onClick={() => void handleToggleMaximize()}
              >
                {isMaximized ? (
                  <VscChromeRestore className="text-base" />
                ) : (
                  <VscChromeMaximize className="text-base" />
                )}
              </button>
            </>
          )}
          <button
            type="button"
            className="flex h-10 w-[46px] items-center justify-center text-[var(--df-text-muted)] transition-colors hover:bg-[#e81123] hover:text-white"
            aria-label={t("common.close")}
            title={t("common.close")}
            onClick={onClose}
          >
            <VscChromeClose className="text-base" />
          </button>
        </div>
      )}
    </header>
  );
}
