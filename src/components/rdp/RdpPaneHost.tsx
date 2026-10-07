import { Channel } from "@tauri-apps/api/core";
import { listen } from "@/lib/backend/api";
import { ShieldAlert } from "lucide-react";
import {
  memo,
  type FocusEvent as ReactFocusEvent,
  type KeyboardEvent as ReactKeyboardEvent,
  type PointerEvent as ReactPointerEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  createRemoteDesktopRenderer,
  type RemoteDesktopRenderer,
} from "@/components/remote-desktop/renderer";
import { invoke } from "@/lib/invoke";
import { decodeRdpFramePatch } from "@/lib/rdpFrame";
import {
  buildRdpUnicodeInput,
  rdpBeforeInputText,
  rdpCompositionCommitText,
  rdpInputFallbackText,
  shouldFallbackToPrintableRdpKey,
  shouldUsePhysicalRdpKey,
} from "@/lib/rdpIme";
import { buildRdpKeyEvent, type RdpInputEvent } from "@/lib/rdpInput";
import {
  decideFitWindowResize,
  keepDesktopSizeIfUnchanged,
  shouldDisableDynamicResizeAfterState,
} from "@/lib/rdpResize";
import {
  getRemoteDesktopContentRect,
  mapClientEventToRemoteDesktopPixel,
} from "@/lib/remoteDesktopViewport";
import type { RdpSessionPane, RemoteDesktopScaleMode } from "@/types/global";

type RdpSessionState =
  | "connecting"
  | "certificate_verification"
  | "authenticating"
  | "negotiating"
  | "active"
  | "reconnecting"
  | "disconnected"
  | "failed";

interface RdpStatePayload {
  sessionId: string;
  state: RdpSessionState;
  message?: string | null;
  errorKind?: string | null;
}

type RdpPointerPayload =
  | { type: "default"; sessionId: string }
  | { type: "hidden"; sessionId: string }
  | { type: "position"; sessionId: string; x: number; y: number }
  | {
      type: "bitmap";
      sessionId: string;
      width: number;
      height: number;
      hotspotX: number;
      hotspotY: number;
      rgbaBase64: string;
    };

interface RemoteCursorBitmap {
  src: string;
  width: number;
  height: number;
  hotspotX: number;
  hotspotY: number;
}

interface RdpPaneHostProps {
  pane: RdpSessionPane;
  active: boolean;
  visible: boolean;
  onConnectionError?: (sessionId: string, error: string) => void;
}

function getCanvasPoint(
  canvas: HTMLCanvasElement,
  event: PointerEvent | WheelEvent,
  scaleMode: RemoteDesktopScaleMode,
) {
  return mapClientEventToRemoteDesktopPixel(canvas, event, scaleMode);
}

function buttonName(button: number): Extract<RdpInputEvent, { type: "mouse-button" }>["button"] {
  if (button === 1) return "middle";
  if (button === 2) return "right";
  if (button === 3) return "back";
  if (button === 4) return "forward";
  return "left";
}

function rgbaBase64ToDataUrl(base64: string, width: number, height: number) {
  const binary = atob(base64);
  const bytes = new Uint8ClampedArray(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const ctx = canvas.getContext("2d");
  if (!ctx) return "";
  ctx.putImageData(new ImageData(bytes, width, height), 0, 0);
  return canvas.toDataURL("image/png");
}

function statusLabel(state: RdpSessionState, message?: string | null) {
  if (message) return message;
  switch (state) {
    case "certificate_verification":
      return "Verifying certificate";
    case "authenticating":
      return "Authenticating";
    case "negotiating":
      return "Initializing remote desktop";
    case "active":
      return "Connected";
    case "reconnecting":
      return "Reconnecting";
    case "disconnected":
      return "Disconnected";
    case "failed":
      return "Connection failed";
    default:
      return "Connecting";
  }
}

function RdpPaneHost({ pane, active, visible, onConnectionError }: RdpPaneHostProps) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const imeRef = useRef<HTMLTextAreaElement | null>(null);
  const cursorRef = useRef<HTMLDivElement | null>(null);
  const rendererRef = useRef<RemoteDesktopRenderer | null>(null);
  const pressedKeysRef = useRef(new Set<string>());
  const composingRef = useRef(false);
  const printableFallbackTimersRef = useRef(new Set<number>());
  const suppressNextInputTextRef = useRef<string | null>(null);
  const pendingMouseMoveRef = useRef<{ x: number; y: number } | null>(null);
  const mouseRafRef = useRef<number | null>(null);
  const cursorRafRef = useRef<number | null>(null);
  const pendingCursorRef = useRef<{ x: number; y: number } | null>(null);
  const remoteCursorBitmapRef = useRef<RemoteCursorBitmap | null>(null);
  const lastResizeRef = useRef<{ width: number; height: number } | null>(null);
  const lastResizeSentAtRef = useRef<number | null>(null);
  const dynamicResizeDisabledRef = useRef(false);
  const [state, setState] = useState<RdpSessionState>(pane.connectError ? "failed" : "connecting");
  const [message, setMessage] = useState<string | null>(pane.connectError ?? null);
  const [desktopSize, setDesktopSize] = useState({
    width: pane.display?.remoteWidth ?? 1920,
    height: pane.display?.remoteHeight ?? 1080,
  });
  const desktopSizeRef = useRef(desktopSize);

  const sendInputBatch = useCallback(
    async (events: RdpInputEvent[]) => {
      if (events.length === 0 || pane.connecting || pane.connectError) return;
      await invoke("rdp_input_batch", { sessionId: pane.sessionId, events }).catch(() => {});
    },
    [pane.connectError, pane.connecting, pane.sessionId],
  );

  const releaseAllKeys = useCallback(() => {
    if (pressedKeysRef.current.size === 0) return;
    pressedKeysRef.current.clear();
    void sendInputBatch([{ type: "release-all-keys" }]);
  }, [sendInputBatch]);

  const cancelPrintableKeyFallbacks = useCallback(() => {
    for (const timer of printableFallbackTimersRef.current) {
      window.clearTimeout(timer);
    }
    printableFallbackTimersRef.current.clear();
  }, []);

  const sendUnicodeInput = useCallback(
    (text: string) => {
      const events = buildRdpUnicodeInput(text);
      if (events.length === 0) return;
      cancelPrintableKeyFallbacks();
      void sendInputBatch(events);
    },
    [cancelPrintableKeyFallbacks, sendInputBatch],
  );

  const schedulePrintableKeyFallback = useCallback(
    (event: KeyboardEvent) => {
      if (!shouldFallbackToPrintableRdpKey(event)) return false;
      const keyDown = buildRdpKeyEvent(event, "key-down");
      if (!keyDown || !("scanCode" in keyDown)) return false;
      const keyUp: RdpInputEvent = {
        type: "key-up",
        scanCode: keyDown.scanCode,
        extended: keyDown.extended,
        repeat: false,
      };
      const timer = window.setTimeout(() => {
        printableFallbackTimersRef.current.delete(timer);
        void sendInputBatch([keyDown, keyUp]);
      }, 80);
      printableFallbackTimersRef.current.add(timer);
      return true;
    },
    [sendInputBatch],
  );

  useEffect(() => {
    lastResizeRef.current = null;
    lastResizeSentAtRef.current = null;
    dynamicResizeDisabledRef.current = false;

    const channel = new Channel<ArrayBuffer>((frame) => {
      const patch = decodeRdpFramePatch(frame);
      setDesktopSize((current) =>
        keepDesktopSizeIfUnchanged(current, {
          width: patch.desktopWidth,
          height: patch.desktopHeight,
        }),
      );
      const canvas = canvasRef.current;
      if (!canvas) return;
      rendererRef.current ??= createRemoteDesktopRenderer(canvas);
      rendererRef.current?.draw(patch);
    });

    if (!pane.connecting && !pane.connectError) {
      void invoke("rdp_attach_frame_channel", {
        sessionId: pane.sessionId,
        frameChannel: channel,
      });
    }
  }, [pane.connectError, pane.connecting, pane.sessionId]);

  useEffect(() => {
    desktopSizeRef.current = desktopSize;
  }, [desktopSize]);

  useEffect(() => {
    return () => {
      rendererRef.current?.dispose();
      rendererRef.current = null;
    };
  }, []);

  useEffect(() => {
    const unlisten = listen<RdpStatePayload>(`rdp-state-${pane.sessionId}`, (event) => {
      setState(event.payload.state);
      setMessage(event.payload.message ?? null);
      if (
        shouldDisableDynamicResizeAfterState({
          state: event.payload.state,
          lastResizeAt: lastResizeSentAtRef.current,
          now: Date.now(),
        })
      ) {
        dynamicResizeDisabledRef.current = true;
      }
      if (event.payload.state === "failed") {
        onConnectionError?.(pane.sessionId, event.payload.message ?? "RDP connection failed");
      }
    });
    return () => {
      void unlisten.then((dispose) => dispose());
    };
  }, [onConnectionError, pane.sessionId]);

  const applyCursorPosition = useCallback(() => {
    cursorRafRef.current = null;
    const cursor = cursorRef.current;
    const canvas = canvasRef.current;
    const container = containerRef.current;
    const position = pendingCursorRef.current;
    const bitmap = remoteCursorBitmapRef.current;
    if (!cursor || !canvas || !container || !position || !bitmap) return;
    const rect = getRemoteDesktopContentRect(canvas, pane.display?.scaleMode ?? "fit");
    const containerRect = container.getBoundingClientRect();
    const scaleX = rect.width / Math.max(1, canvas.width);
    const scaleY = rect.height / Math.max(1, canvas.height);
    const x = rect.left - containerRect.left + position.x * scaleX - bitmap.hotspotX * scaleX;
    const y = rect.top - containerRect.top + position.y * scaleY - bitmap.hotspotY * scaleY;
    cursor.style.transform = `translate3d(${Math.round(x)}px, ${Math.round(y)}px, 0)`;
  }, [pane.display?.scaleMode]);

  useEffect(() => {
    const unlisten = listen<RdpPointerPayload>(`rdp-pointer-${pane.sessionId}`, (event) => {
      const canvas = canvasRef.current;
      const cursor = cursorRef.current;
      if (!canvas || !cursor) return;

      if (event.payload.type === "default") {
        remoteCursorBitmapRef.current = null;
        cursor.replaceChildren();
        cursor.style.display = "none";
        canvas.style.cursor = "";
        return;
      }

      if (event.payload.type === "hidden") {
        cursor.style.display = "none";
        canvas.style.cursor = "";
        return;
      }

      if (event.payload.type === "bitmap") {
        const bitmap = {
          src: rgbaBase64ToDataUrl(
            event.payload.rgbaBase64,
            event.payload.width,
            event.payload.height,
          ),
          width: event.payload.width,
          height: event.payload.height,
          hotspotX: event.payload.hotspotX,
          hotspotY: event.payload.hotspotY,
        };
        remoteCursorBitmapRef.current = bitmap;
        canvas.style.cursor = "none";
        const img = document.createElement("img");
        img.src = bitmap.src;
        img.width = bitmap.width;
        img.height = bitmap.height;
        img.draggable = false;
        cursor.replaceChildren(img);
        cursor.style.display = "block";
        return;
      }

      pendingCursorRef.current = { x: event.payload.x, y: event.payload.y };
      const hasRemoteCursor = remoteCursorBitmapRef.current !== null;
      canvas.style.cursor = hasRemoteCursor ? "none" : "";
      cursor.style.display = hasRemoteCursor ? "block" : "none";
      if (cursorRafRef.current === null) {
        cursorRafRef.current = requestAnimationFrame(applyCursorPosition);
      }
    });
    return () => {
      void unlisten.then((dispose) => dispose());
      if (cursorRafRef.current !== null) cancelAnimationFrame(cursorRafRef.current);
      cursorRafRef.current = null;
    };
  }, [applyCursorPosition, pane.sessionId]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    if (pane.display?.scaleMode !== "fit") return;

    let timer: number | null = null;
    const syncResize = () => {
      if (timer !== null) window.clearTimeout(timer);
      const delay = lastResizeRef.current ? 200 : 600;
      timer = window.setTimeout(() => {
        const rect = container.getBoundingClientRect();
        const visibleForResize =
          active && visible && state === "active" && rect.width > 0 && rect.height > 0;
        const remoteSize = desktopSizeRef.current;
        const decision = decideFitWindowResize({
          mode: "fit-window",
          visible: visibleForResize,
          containerWidth: rect.width,
          containerHeight: rect.height,
          remoteWidth: remoteSize.width,
          remoteHeight: remoteSize.height,
          lastWidth: lastResizeRef.current?.width,
          lastHeight: lastResizeRef.current?.height,
          allowInitialResize: true,
          disabled: dynamicResizeDisabledRef.current,
          minDelta: 32,
        });
        if (!decision.shouldResize) {
          if (visibleForResize && decision.width > 0 && decision.height > 0) {
            lastResizeRef.current ??= { width: decision.width, height: decision.height };
          }
          return;
        }
        lastResizeRef.current = { width: decision.width, height: decision.height };
        lastResizeSentAtRef.current = Date.now();
        void invoke("rdp_resize", {
          sessionId: pane.sessionId,
          width: decision.width,
          height: decision.height,
        }).catch(() => {});
      }, delay);
    };

    const observer = new ResizeObserver(syncResize);
    observer.observe(container);
    syncResize();
    return () => {
      observer.disconnect();
      if (timer !== null) window.clearTimeout(timer);
    };
  }, [active, pane.display?.scaleMode, pane.sessionId, state, visible]);

  useEffect(() => {
    if (!active || !visible) releaseAllKeys();
  }, [active, releaseAllKeys, visible]);

  useEffect(() => {
    if (active && visible) {
      imeRef.current?.focus({ preventScroll: true });
    }
  }, [active, visible]);

  useEffect(() => {
    window.addEventListener("blur", releaseAllKeys);
    return () => {
      window.removeEventListener("blur", releaseAllKeys);
      cancelPrintableKeyFallbacks();
      releaseAllKeys();
    };
  }, [cancelPrintableKeyFallbacks, releaseAllKeys]);

  useEffect(() => {
    if (!active || !visible || pane.connecting || pane.connectError || state !== "active") {
      void invoke("rdp_set_keyboard_capture", { sessionId: null }).catch(() => {});
      return;
    }

    const container = containerRef.current;
    if (container?.contains(document.activeElement)) {
      void invoke("rdp_set_keyboard_capture", { sessionId: pane.sessionId }).catch(() => {});
    }

    return () => {
      void invoke("rdp_set_keyboard_capture", { sessionId: null }).catch(() => {});
    };
  }, [active, pane.connectError, pane.connecting, pane.sessionId, state, visible]);

  const handleFocus = useCallback(() => {
    if (!active || !visible || pane.connecting || pane.connectError || state !== "active") return;
    void invoke("rdp_set_keyboard_capture", { sessionId: pane.sessionId }).catch(() => {});
  }, [active, pane.connectError, pane.connecting, pane.sessionId, state, visible]);

  const handleBlur = useCallback(
    (event: ReactFocusEvent<HTMLElement>) => {
      if (event.currentTarget.contains(event.relatedTarget as Node | null)) return;
      void invoke("rdp_set_keyboard_capture", { sessionId: null }).catch(() => {});
      releaseAllKeys();
    },
    [releaseAllKeys],
  );

  const handleKeyDown = useCallback(
    (event: ReactKeyboardEvent<HTMLElement>) => {
      if (!shouldUsePhysicalRdpKey(event.nativeEvent)) return;
      const inputEvent = buildRdpKeyEvent(event.nativeEvent, "key-down");
      if (!inputEvent) return;
      event.preventDefault();
      event.stopPropagation();
      pressedKeysRef.current.add(event.code);
      void sendInputBatch([inputEvent]);
    },
    [sendInputBatch],
  );

  const handleKeyUp = useCallback(
    (event: ReactKeyboardEvent<HTMLElement>) => {
      if (event.defaultPrevented) return;
      if (!shouldUsePhysicalRdpKey(event.nativeEvent)) return;
      const inputEvent = buildRdpKeyEvent(event.nativeEvent, "key-up");
      if (!inputEvent) return;
      event.preventDefault();
      event.stopPropagation();
      pressedKeysRef.current.delete(event.code);
      void sendInputBatch([inputEvent]);
    },
    [sendInputBatch],
  );

  const handleRdpKeyDown = useCallback(
    (event: ReactKeyboardEvent<HTMLElement>) => {
      if (event.defaultPrevented) return;
      if (shouldUsePhysicalRdpKey(event.nativeEvent)) {
        handleKeyDown(event);
        return;
      }
      if (schedulePrintableKeyFallback(event.nativeEvent)) {
        event.stopPropagation();
      }
    },
    [handleKeyDown, schedulePrintableKeyFallback],
  );

  const handlePhysicalKeyDownCapture = useCallback(
    (event: ReactKeyboardEvent<HTMLElement>) => {
      if (!shouldUsePhysicalRdpKey(event.nativeEvent)) return;
      handleKeyDown(event);
    },
    [handleKeyDown],
  );

  const handlePhysicalKeyUpCapture = useCallback(
    (event: ReactKeyboardEvent<HTMLElement>) => {
      if (!shouldUsePhysicalRdpKey(event.nativeEvent)) return;
      handleKeyUp(event);
    },
    [handleKeyUp],
  );

  const flushMouseMove = useCallback(() => {
    mouseRafRef.current = null;
    const move = pendingMouseMoveRef.current;
    pendingMouseMoveRef.current = null;
    if (move) void sendInputBatch([{ type: "mouse-move", ...move }]);
  }, [sendInputBatch]);

  const queueMouseMove = useCallback(
    (event: ReactPointerEvent<HTMLCanvasElement>) => {
      const canvas = canvasRef.current;
      if (!canvas) return;
      pendingMouseMoveRef.current = getCanvasPoint(
        canvas,
        event.nativeEvent,
        pane.display?.scaleMode ?? "fit",
      );
      if (mouseRafRef.current === null) {
        mouseRafRef.current = requestAnimationFrame(flushMouseMove);
      }
    },
    [flushMouseMove, pane.display?.scaleMode],
  );

  const scaleStyle = useMemo(
    () => ({
      width: "100%",
      height: "100%",
      objectFit: "contain" as const,
    }),
    [],
  );

  return (
    <div
      ref={containerRef}
      className="group relative flex h-full w-full min-h-0 min-w-0 items-center justify-center overflow-hidden bg-black outline-none"
      data-rdp-input-root="true"
      data-remote-desktop-input-root="true"
      tabIndex={active ? 0 : -1}
      onFocus={handleFocus}
      onKeyDownCapture={handlePhysicalKeyDownCapture}
      onKeyUpCapture={handlePhysicalKeyUpCapture}
      onKeyDown={handleRdpKeyDown}
      onKeyUp={handleKeyUp}
      onBlur={handleBlur}
      onPointerDown={() => imeRef.current?.focus({ preventScroll: true })}
    >
      <textarea
        ref={imeRef}
        aria-hidden="true"
        className="pointer-events-none absolute h-px w-px resize-none opacity-0"
        tabIndex={active ? 0 : -1}
        defaultValue=""
        onCompositionStart={() => {
          composingRef.current = true;
        }}
        onCompositionEnd={(event) => {
          composingRef.current = false;
          const text = rdpCompositionCommitText(event.data || event.currentTarget.value);
          event.currentTarget.value = "";
          if (text) {
            suppressNextInputTextRef.current = text;
            sendUnicodeInput(text);
          }
        }}
        onBeforeInput={(event) => {
          const text = rdpBeforeInputText(event.nativeEvent as InputEvent);
          if (!text || composingRef.current) return;
          event.preventDefault();
          event.currentTarget.value = "";
          sendUnicodeInput(text);
        }}
        onInput={(event) => {
          const text = rdpInputFallbackText(event.currentTarget.value, composingRef.current);
          if (!text) return;
          event.currentTarget.value = "";
          if (suppressNextInputTextRef.current === text) {
            suppressNextInputTextRef.current = null;
            return;
          }
          suppressNextInputTextRef.current = null;
          sendUnicodeInput(text);
        }}
        onKeyDown={(event) => {
          handleRdpKeyDown(event);
        }}
        onKeyUp={(event) => {
          if (!shouldUsePhysicalRdpKey(event.nativeEvent)) return;
          handleKeyUp(event);
        }}
      />
      <canvas
        ref={canvasRef}
        className="block object-contain"
        style={scaleStyle}
        onPointerMove={queueMouseMove}
        onPointerDown={(event) => {
          const canvas = canvasRef.current;
          if (!canvas) return;
          event.currentTarget.setPointerCapture(event.pointerId);
          const point = getCanvasPoint(canvas, event.nativeEvent, pane.display?.scaleMode ?? "fit");
          void sendInputBatch([
            { type: "mouse-button", button: buttonName(event.button), pressed: true, ...point },
          ]);
        }}
        onPointerUp={(event) => {
          const canvas = canvasRef.current;
          if (!canvas) return;
          const point = getCanvasPoint(canvas, event.nativeEvent, pane.display?.scaleMode ?? "fit");
          void sendInputBatch([
            { type: "mouse-button", button: buttonName(event.button), pressed: false, ...point },
          ]);
        }}
        onWheel={(event) => {
          const canvas = canvasRef.current;
          if (!canvas) return;
          event.preventDefault();
          const point = getCanvasPoint(canvas, event.nativeEvent, pane.display?.scaleMode ?? "fit");
          void sendInputBatch([
            { type: "mouse-wheel", deltaX: event.deltaX, deltaY: event.deltaY, ...point },
          ]);
        }}
      />

      <div
        ref={cursorRef}
        aria-hidden="true"
        className="pointer-events-none absolute left-0 top-0 z-10 hidden"
      />

      {state !== "active" && (
        <div className="absolute inset-0 flex items-center justify-center bg-black/45 text-white">
          <div className="flex items-center gap-3 rounded border border-white/15 bg-black/70 px-4 py-3 text-sm">
            <ShieldAlert className="h-5 w-5 text-sky-300" />
            <span>{statusLabel(state, message)}</span>
          </div>
        </div>
      )}
    </div>
  );
}

export default memo(RdpPaneHost);
