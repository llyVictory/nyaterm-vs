import { emit, listen } from "@/lib/backend/api";
import { getCurrentWindow } from "@/lib/backend/platform/window";
import { useCallback, useEffect, useRef } from "react";

/**
 * Monitors user inactivity and fires `onLock` after `minutes` of idle time.
 * When `minutes` is 0 (or falsy), the hook is completely inert.
 *
 * Tracked events: mousemove, mousedown, keydown, touchstart, scroll.
 */
export function useIdleLock(minutes: number, locked: boolean, onLock: () => void) {
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const onLockRef = useRef(onLock);
  const lastActivityEmitAtRef = useRef(0);
  onLockRef.current = onLock;

  const resetTimer = useCallback(() => {
    if (timerRef.current) clearTimeout(timerRef.current);
    if (minutes > 0 && !locked) {
      timerRef.current = setTimeout(
        () => {
          onLockRef.current();
        },
        minutes * 60 * 1000,
      );
    }
  }, [locked, minutes]);

  useEffect(() => {
    if (minutes <= 0 || locked) {
      // Disabled – clear any existing timer and bail
      if (timerRef.current) {
        clearTimeout(timerRef.current);
        timerRef.current = null;
      }
      return;
    }

    const EVENTS: (keyof WindowEventMap)[] = [
      "mousemove",
      "mousedown",
      "keydown",
      "touchstart",
      "scroll",
    ];

    // Start the initial timer
    resetTimer();

    // Reset on any user activity
    const handleLocalActivity = () => {
      resetTimer();

      const now = Date.now();
      if (now - lastActivityEmitAtRef.current < 1000) return;
      lastActivityEmitAtRef.current = now;

      emit("app-user-activity", {
        at: now,
        sourceWindowLabel: getCurrentWindow().label,
      }).catch(() => {});
    };

    for (const evt of EVENTS) {
      window.addEventListener(evt, handleLocalActivity, { passive: true });
    }

    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
      for (const evt of EVENTS) {
        window.removeEventListener(evt, handleLocalActivity);
      }
    };
  }, [locked, minutes, resetTimer]);

  useEffect(() => {
    if (minutes <= 0 || locked) return;

    const unlisten = listen("app-user-activity", () => {
      resetTimer();
    });

    return () => {
      unlisten.then((dispose) => dispose());
    };
  }, [locked, minutes, resetTimer]);
}
