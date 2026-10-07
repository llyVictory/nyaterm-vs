import { listen } from "@/lib/backend/api";
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@/lib/invoke";
import { logger } from "@/lib/logger";

interface AppLockStateChangedPayload {
  locked: boolean;
}

export function useAppLockState() {
  const [isLocked, setIsLockedState] = useState(false);
  const [lockStateLoaded, setLockStateLoaded] = useState(false);

  useEffect(() => {
    let disposed = false;

    invoke<boolean>("get_app_lock_state")
      .then((locked) => {
        if (disposed) return;
        setIsLockedState(locked);
      })
      .catch((error) => {
        logger.error({
          domain: "security.flow",
          event: "state.load_failed",
          message: "Failed to load app lock state",
          error,
        });
      })
      .finally(() => {
        if (!disposed) {
          setLockStateLoaded(true);
        }
      });

    return () => {
      disposed = true;
    };
  }, []);

  useEffect(() => {
    const unlisten = listen<AppLockStateChangedPayload>("app-lock-state-changed", ({ payload }) => {
      setIsLockedState(payload.locked);
      setLockStateLoaded(true);
    });

    return () => {
      unlisten.then((dispose) => dispose());
    };
  }, []);

  const setIsLocked = useCallback((locked: boolean) => {
    setIsLockedState(locked);
    setLockStateLoaded(true);
    invoke<boolean>("set_app_lock_state", { locked })
      .then((nextLocked) => {
        setIsLockedState(nextLocked);
      })
      .catch((error) => {
        logger.error({
          domain: "security.flow",
          event: "state.save_failed",
          message: "Failed to update app lock state",
          error,
        });
      });
  }, []);

  return { isLocked, setIsLocked, lockStateLoaded };
}
