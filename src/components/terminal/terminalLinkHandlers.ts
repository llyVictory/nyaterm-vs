import { openUrl } from "@/lib/backend/platform/opener";
import { WebLinksAddon } from "@xterm/addon-web-links";
import type { ILinkHandler, Terminal } from "@xterm/xterm";
import { logger } from "@/lib/logger";

type TranslationFn = (key: string, opts?: Record<string, unknown>) => string;

function isMacPlatform() {
  return (
    typeof navigator !== "undefined" &&
    /(Mac|iPhone|iPad|iPod)/i.test(`${navigator.platform} ${navigator.userAgent}`)
  );
}

export function createTerminalLinkHandlers(
  terminal: Terminal,
  tRef: { current: TranslationFn },
): {
  oscLinkHandler: ILinkHandler;
  webLinksAddon: WebLinksAddon;
  removePopup: () => void;
} {
  const modifierLabel = isMacPlatform() ? "Cmd" : "Ctrl";
  const allowedLinkProtocols = new Set(["http:", "https:", "mailto:"]);
  const linkPopupDelayMs = 350;
  let linkPopup: HTMLDivElement | null = null;
  let linkPopupTimer: number | null = null;

  const clearLinkPopupTimer = () => {
    if (linkPopupTimer !== null) {
      window.clearTimeout(linkPopupTimer);
      linkPopupTimer = null;
    }
  };

  const destroyLinkPopup = () => {
    linkPopup?.remove();
    linkPopup = null;
  };

  const removePopup = () => {
    clearLinkPopupTimer();
    destroyLinkPopup();
  };

  const positionLinkPopup = (popup: HTMLDivElement, clientX: number, clientY: number) => {
    const terminalEl = terminal.element;
    if (!terminalEl) return;

    const hostRect = terminalEl.getBoundingClientRect();
    const margin = 8;
    const offset = 16;

    let left = clientX - hostRect.left + offset;
    let top = clientY - hostRect.top + offset;

    if (left + popup.offsetWidth + margin > terminalEl.clientWidth) {
      left = terminalEl.clientWidth - popup.offsetWidth - margin;
    }

    if (top + popup.offsetHeight + margin > terminalEl.clientHeight) {
      top = clientY - hostRect.top - popup.offsetHeight - offset;
    }

    popup.style.left = `${Math.max(margin, left)}px`;
    popup.style.top = `${Math.max(margin, top)}px`;
  };

  const showLinkPopup = (text: string, clientX: number, clientY: number) => {
    const terminalEl = terminal.element;
    if (!terminalEl) return;

    destroyLinkPopup();

    const popup = document.createElement("div");
    popup.className = "xterm-link-popup xterm-hover";

    const urlLine = document.createElement("div");
    urlLine.className = "xterm-link-popup__url";
    urlLine.textContent = text;
    popup.appendChild(urlLine);

    const hintLine = document.createElement("div");
    hintLine.className = "xterm-link-popup__hint";
    hintLine.textContent = tRef.current("terminal.linkOpenHint", { modifier: modifierLabel });
    popup.appendChild(hintLine);

    terminalEl.appendChild(popup);
    positionLinkPopup(popup, clientX, clientY);
    linkPopup = popup;
  };

  const scheduleLinkPopup = (event: MouseEvent, text: string) => {
    clearLinkPopupTimer();
    destroyLinkPopup();

    const { clientX, clientY } = event;
    linkPopupTimer = window.setTimeout(() => {
      showLinkPopup(text, clientX, clientY);
      linkPopupTimer = null;
    }, linkPopupDelayMs);
  };

  const hasRequiredModifier = (event: MouseEvent) =>
    isMacPlatform() ? event.metaKey : event.ctrlKey;

  const isAllowedLinkUri = (uri: string) => {
    try {
      return allowedLinkProtocols.has(new URL(uri).protocol);
    } catch {
      return false;
    }
  };

  const handleLinkActivation = (event: MouseEvent, uri: string) => {
    if (!hasRequiredModifier(event)) return;
    if (!isAllowedLinkUri(uri)) return;

    removePopup();
    openUrl(uri).catch((err: unknown) =>
      logger.error({
        domain: "ui.error",
        event: "terminal.link_open_failed",
        message: "Failed to open link",
        error: err,
      }),
    );
  };

  const oscLinkHandler: ILinkHandler = {
    activate: handleLinkActivation,
    hover: (event, text) => scheduleLinkPopup(event, text),
    leave: () => removePopup(),
    allowNonHttpProtocols: true,
  };

  const webLinksAddon = new WebLinksAddon(handleLinkActivation, {
    hover: (event, text) => scheduleLinkPopup(event, text),
    leave: () => removePopup(),
  });

  return { oscLinkHandler, webLinksAddon, removePopup };
}
