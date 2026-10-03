import { Channel } from "@tauri-apps/api/core";
import { listen } from "@/lib/backend/api";
import { Eye, Monitor, Power, RotateCcw, ShieldAlert } from "lucide-react";
import {
  memo,
  type FocusEvent as ReactFocusEvent,
  type KeyboardEvent as ReactKeyboardEvent,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import {
  RemoteDesktopSurface,
  type RemoteDesktopSurfaceHandle,
} from "@/components/remote-desktop/RemoteDesktopSurface";
import { Button } from "@/components/ui/button";
import { readClipboardText, writeClipboardText } from "@/lib/clipboard";
import { invoke } from "@/lib/invoke";
import { decodeRemoteDesktopFramePatch } from "@/lib/remoteDesktopFrame";
import {
  buildVncCompositionKeyEvents,
  mapKeyboardEventToVncKeysym,
  pointerButtonMask,
  VNC_POINTER_BUTTON,
  type VncInputEvent,
} from "@/lib/vncInput";
import type { VncSessionPane } from "@/types/global";

type VncSessionState =
  | "connecting"
  | "authenticating"
  | "negotiating"
  | "active"
  | "reconnecting"
  | "disconnected"
  | "failed";

interface VncStatePayload {
  sessionId: string;
  state: VncSessionState;
  message?: string | null;
  errorKind?: string | null;
}

interface VncClipboardPayload {
  sessionId: string;
  text: string;
}

interface VncPaneHostProps {
  pane: VncSessionPane;
  active: boolean;
  visible: boolean;
  onDisconnectedCloseRequested?: () => void;
  onConnectionError?: (sessionId: string, error: string) => void;
}

function statusLabel(state: VncSessionState, message?: string | null) {
  if (message) return message;
  switch (state) {
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

function clampCoordinate(value: number) {
  if (!Number.isFinite(value)) return 0;
  return Math.max(0, Math.min(65535, Math.round(value)));
}

function wheelButtonMask(event: WheelEvent) {
  if (Math.abs(event.deltaX) > Math.abs(event.deltaY)) {
    return event.deltaX > 0 ? VNC_POINTER_BUTTON.wheelRight : VNC_POINTER_BUTTON.wheelLeft;
  }
  return event.deltaY > 0 ? VNC_POINTER_BUTTON.wheelDown : VNC_POINTER_BUTTON.wheelUp;
}

function isVncClipboardTextAllowed(text: string) {
  return (
    text.length > 0 &&
    text.length <= 1024 * 1024 &&
    [...text].every((ch) => ch.charCodeAt(0) <= 0xff)
  );
}

function VncPaneHost({
  pane,
  active,
  visible,
  onDisconnectedCloseRequested,
  onConnectionError,
}: VncPaneHostProps) {
  const surfaceRef = useRef<RemoteDesktopSurfaceHandle | null>(null);
  const imeRef = useRef<HTMLTextAreaElement | null>(null);
  const pressedKeysRef = useRef(new Set<number>());
  const pointerButtonsRef = useRef(new Set<number>());
  const pendingPointerRef = useRef<{
    x: number;
    y: number;
    buttonMask: number;
  } | null>(null);
  const pointerRafRef = useRef<number | null>(null);
  const composingRef = useRef(false);
  const lastLocalSentRef = useRef<string | null>(null);
  const lastRemoteReceivedRef = useRef<string | null>(null);
  const [state, setState] = useState<VncSessionState>(pane.connectError ? "failed" : "connecting");
  const [message, setMessage] = useState<string | null>(pane.connectError ?? null);
  const [desktopSize, setDesktopSize] = useState({
    width: pane.display?.remoteWidth ?? 0,
    height: pane.display?.remoteHeight ?? 0,
  });

  const viewOnly = pane.display?.viewOnly ?? false;
  const clipboardEnabled = pane.display?.clipboardEnabled ?? true;
  const inputEnabled = !viewOnly && !pane.connecting && !pane.connectError && state === "active";
  const clipboardBridgeEnabled =
    clipboardEnabled &&
    active &&
    visible &&
    !pane.connecting &&
    !pane.connectError &&
    state === "active";

  const sendInputBatch = useCallback(
    async (events: VncInputEvent[]) => {
      if (events.length === 0 || !inputEnabled) return;
      await invoke("vnc_input_batch", { sessionId: pane.sessionId, events }).catch(() => {});
    },
    [inputEnabled, pane.sessionId],
  );

  const releaseAllKeys = useCallback(() => {
    if (pressedKeysRef.current.size === 0) return;
    pressedKeysRef.current.clear();
    void invoke("vnc_input_batch", {
      sessionId: pane.sessionId,
      events: [{ type: "release-all-keys" }],
    }).catch(() => {});
  }, [pane.sessionId]);

  useEffect(() => {
    const channel = new Channel<ArrayBuffer>((frame) => {
      const patch = decodeRemoteDesktopFramePatch(frame);
      setDesktopSize({ width: patch.desktopWidth, height: patch.desktopHeight });
      surfaceRef.current?.drawFrame(patch);
    });

    if (!pane.connecting && !pane.connectError) {
      void invoke("vnc_attach_frame_channel", {
        sessionId: pane.sessionId,
        frameChannel: channel,
      }).catch(() => {});
    }
  }, [pane.connectError, pane.connecting, pane.sessionId]);

  useEffect(() => {
    const unlisten = listen<VncStatePayload>(`vnc-state-${pane.sessionId}`, (event) => {
      setState(event.payload.state);
      setMessage(event.payload.message ?? null);
      if (event.payload.state === "failed") {
        onConnectionError?.(pane.sessionId, event.payload.message ?? "VNC connection failed");
      }
    });
    return () => {
      void unlisten.then((dispose) => dispose());
    };
  }, [onConnectionError, pane.sessionId]);

  useEffect(() => {
    if (!clipboardBridgeEnabled) return;
    const unlisten = listen<VncClipboardPayload>(`vnc-clipboard-${pane.sessionId}`, (event) => {
      const text = event.payload.text;
      lastRemoteReceivedRef.current = text;
      void writeClipboardText(text).catch(() => {});
    });
    return () => {
      void unlisten.then((dispose) => dispose());
    };
  }, [clipboardBridgeEnabled, pane.sessionId]);

  useEffect(() => {
    if (!clipboardBridgeEnabled || viewOnly) return;
    let disposed = false;
    let polling = false;
    const poll = async () => {
      if (disposed || polling) return;
      polling = true;
      try {
        const text = await readClipboardText();
        if (
          !disposed &&
          text !== lastLocalSentRef.current &&
          text !== lastRemoteReceivedRef.current &&
          isVncClipboardTextAllowed(text)
        ) {
          lastLocalSentRef.current = text;
          await invoke("vnc_set_clipboard_text", { sessionId: pane.sessionId, text }).catch(
            () => {},
          );
        }
      } catch {
        /* clipboard access can be denied while the pane remains usable */
      } finally {
        polling = false;
      }
    };
    void poll();
    const interval = window.setInterval(() => void poll(), 1000);
    return () => {
      disposed = true;
      window.clearInterval(interval);
    };
  }, [clipboardBridgeEnabled, pane.sessionId, viewOnly]);

  useEffect(() => {
    if (!active || !visible) releaseAllKeys();
  }, [active, releaseAllKeys, visible]);

  useEffect(() => {
    if (active && visible) {
      surfaceRef.current?.focus();
      imeRef.current?.focus({ preventScroll: true });
    }
  }, [active, visible]);

  useEffect(() => {
    window.addEventListener("blur", releaseAllKeys);
    return () => {
      window.removeEventListener("blur", releaseAllKeys);
      releaseAllKeys();
      if (pointerRafRef.current !== null) cancelAnimationFrame(pointerRafRef.current);
      pointerRafRef.current = null;
    };
  }, [releaseAllKeys]);

  const flushPointer = useCallback(() => {
    pointerRafRef.current = null;
    const event = pendingPointerRef.current;
    pendingPointerRef.current = null;
    if (event) void sendInputBatch([{ type: "pointer", ...event }]);
  }, [sendInputBatch]);

  const queuePointer = useCallback(
    (
      point: { x: number; y: number },
      buttonMask = pointerButtonMask(pointerButtonsRef.current),
    ) => {
      pendingPointerRef.current = {
        x: clampCoordinate(point.x),
        y: clampCoordinate(point.y),
        buttonMask,
      };
      if (pointerRafRef.current === null) {
        pointerRafRef.current = requestAnimationFrame(flushPointer);
      }
    },
    [flushPointer],
  );

  const handleKeyDown = useCallback(
    (event: ReactKeyboardEvent<HTMLElement>) => {
      if (!inputEnabled || event.nativeEvent.isComposing) return;
      const keysym = mapKeyboardEventToVncKeysym(event.nativeEvent);
      if (keysym === null) return;
      event.preventDefault();
      event.stopPropagation();
      pressedKeysRef.current.add(keysym);
      void sendInputBatch([{ type: "key", keysym, pressed: true }]);
    },
    [inputEnabled, sendInputBatch],
  );

  const handleKeyUp = useCallback(
    (event: ReactKeyboardEvent<HTMLElement>) => {
      if (!inputEnabled) return;
      const keysym = mapKeyboardEventToVncKeysym(event.nativeEvent);
      if (keysym === null) return;
      event.preventDefault();
      event.stopPropagation();
      pressedKeysRef.current.delete(keysym);
      void sendInputBatch([{ type: "key", keysym, pressed: false }]);
    },
    [inputEnabled, sendInputBatch],
  );

  const handleBlur = useCallback(
    (_event?: ReactFocusEvent<HTMLElement>) => {
      releaseAllKeys();
    },
    [releaseAllKeys],
  );

  const commitText = useCallback(
    (text: string) => {
      const events = buildVncCompositionKeyEvents(text);
      if (events.length > 0) void sendInputBatch(events);
    },
    [sendInputBatch],
  );

  return (
    <div
      className="group relative h-full w-full min-h-0 min-w-0 bg-black"
      onKeyDownCapture={handleKeyDown}
      onKeyUpCapture={handleKeyUp}
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
          const text = event.data || event.currentTarget.value;
          event.currentTarget.value = "";
          if (text) commitText(text);
        }}
        onBeforeInput={(event) => {
          const text = (event.nativeEvent as InputEvent).data;
          if (!text || composingRef.current) return;
          event.preventDefault();
          event.currentTarget.value = "";
          commitText(text);
        }}
        onInput={(event) => {
          if (composingRef.current) return;
          const text = event.currentTarget.value;
          event.currentTarget.value = "";
          if (text) commitText(text);
        }}
      />
      <RemoteDesktopSurface
        ref={surfaceRef}
        scaleMode={pane.display?.scaleMode ?? "fit"}
        active={active}
        visible={visible}
        inputEnabled={inputEnabled}
        onFocus={() => imeRef.current?.focus({ preventScroll: true })}
        onBlur={handleBlur}
        onPointerMove={(point) => queuePointer(point)}
        onPointerButton={(point, button, pressed) => {
          if (pressed) {
            pointerButtonsRef.current.add(button);
          } else {
            pointerButtonsRef.current.delete(button);
          }
          void sendInputBatch([
            {
              type: "pointer",
              x: clampCoordinate(point.x),
              y: clampCoordinate(point.y),
              buttonMask: pointerButtonMask(pointerButtonsRef.current),
            },
          ]);
        }}
        onWheel={(point, event) => {
          const x = clampCoordinate(point.x);
          const y = clampCoordinate(point.y);
          const mask = wheelButtonMask(event);
          void sendInputBatch([
            { type: "pointer", x, y, buttonMask: mask },
            { type: "pointer", x, y, buttonMask: pointerButtonMask(pointerButtonsRef.current) },
          ]);
        }}
      >
        <div className="absolute left-2 top-2 flex items-center gap-1 rounded border border-white/15 bg-black/65 px-2 py-1 text-xs text-white opacity-0 transition-opacity group-hover:opacity-100">
          <Monitor className="h-3.5 w-3.5" />
          <span className="max-w-40 truncate">{pane.name}</span>
          {desktopSize.width > 0 && desktopSize.height > 0 ? (
            <span className="text-white/55">
              {desktopSize.width}x{desktopSize.height}
            </span>
          ) : null}
          {viewOnly ? <Eye className="h-3.5 w-3.5 text-white/70" /> : null}
          <Button
            variant="ghost"
            size="icon-xs"
            aria-label="Reconnect VNC session"
            className="h-6 w-6 text-white"
            onClick={() => void invoke("vnc_reconnect", { sessionId: pane.sessionId })}
          >
            <RotateCcw className="h-3.5 w-3.5" />
          </Button>
          <Button
            variant="ghost"
            size="icon-xs"
            aria-label="Close VNC session"
            className="h-6 w-6 text-white"
            onClick={onDisconnectedCloseRequested}
          >
            <Power className="h-3.5 w-3.5" />
          </Button>
        </div>

        {state !== "active" ? (
          <div className="absolute inset-0 flex items-center justify-center bg-black/45 text-white">
            <div className="flex items-center gap-3 rounded border border-white/15 bg-black/70 px-4 py-3 text-sm">
              <ShieldAlert className="h-5 w-5 text-sky-300" />
              <span>{statusLabel(state, message)}</span>
            </div>
          </div>
        ) : null}
      </RemoteDesktopSurface>
    </div>
  );
}

export default memo(VncPaneHost);
