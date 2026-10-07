import { listen } from "@/lib/backend/api";
import { useEffect, useRef, useState } from "react";
import {
  getOpenModalChildWindowLabels,
  isOwnedModalChildLabel,
  prepareForModalChildClose,
  raiseModalChildWindowGroup,
  syncMainWindowModalState,
} from "@/lib/windowManager";

export function useModalChildWindows() {
  const [modalChildWindowLabels, setModalChildWindowLabels] = useState<Set<string>>(
    () => new Set(),
  );
  const closingLabelsRef = useRef<Set<string>>(new Set());

  useEffect(() => {
    const refreshOpenModalChildWindows = async () => {
      const labels = await getOpenModalChildWindowLabels().catch(() => []);
      setModalChildWindowLabels(
        new Set(labels.filter((label) => !closingLabelsRef.current.has(label))),
      );
      await syncMainWindowModalState().catch(() => {});
    };

    void refreshOpenModalChildWindows();

    const unsubs = [
      listen<{ label: string }>("child-window-opened", ({ payload }) => {
        if (!isOwnedModalChildLabel(payload.label)) return;
        closingLabelsRef.current.delete(payload.label);
        setModalChildWindowLabels((labels) => {
          const nextLabels = new Set(labels);
          nextLabels.add(payload.label);
          return nextLabels;
        });
        void refreshOpenModalChildWindows();
      }),
      listen<{ label: string }>("child-window-closed", ({ payload }) => {
        if (!isOwnedModalChildLabel(payload.label)) return;
        closingLabelsRef.current.add(payload.label);
        setModalChildWindowLabels((labels) => {
          const nextLabels = new Set(labels);
          nextLabels.delete(payload.label);
          return nextLabels;
        });
        void prepareForModalChildClose(payload.label);
        window.setTimeout(() => {
          closingLabelsRef.current.delete(payload.label);
          void refreshOpenModalChildWindows();
        }, 250);
      }),
      listen<{ label: string; ownerLabel?: string | null }>(
        "modal-child-window-focused",
        ({ payload }) => {
          if (!isOwnedModalChildLabel(payload.label)) return;
          void raiseModalChildWindowGroup({
            focusLabel: payload.label,
            reason: "child-focus",
          });
        },
      ),
    ];

    return () => {
      unsubs.forEach((promise) => {
        promise.then((unsub) => unsub());
      });
    };
  }, []);

  const modalChildWindowCount = modalChildWindowLabels.size;

  useEffect(() => {
    if (modalChildWindowCount === 0) return;

    let disposed = false;
    let checking = false;
    const reconcileClosedWindows = async () => {
      if (checking) return;
      checking = true;
      try {
        const openLabels = await getOpenModalChildWindowLabels();
        if (disposed) return;
        const labels = openLabels.filter((label) => !closingLabelsRef.current.has(label));
        if (
          labels.length === modalChildWindowLabels.size &&
          labels.every((label) => modalChildWindowLabels.has(label))
        ) {
          return;
        }
        setModalChildWindowLabels(new Set(labels));
        await syncMainWindowModalState();
      } catch {
        // The next check can recover from a transient window query failure.
      } finally {
        checking = false;
      }
    };

    const intervalId = window.setInterval(() => {
      void reconcileClosedWindows();
    }, 500);
    return () => {
      disposed = true;
      window.clearInterval(intervalId);
    };
  }, [modalChildWindowCount, modalChildWindowLabels]);

  useEffect(() => {
    let unlistenFocusChanged: (() => void) | undefined;

    import("@/lib/backend/platform/window").then(({ getCurrentWindow }) => {
      getCurrentWindow()
        .onFocusChanged(({ payload: focused }) => {
          if (!focused || modalChildWindowCount === 0) return;
          void syncMainWindowModalState();
          void raiseModalChildWindowGroup({ reason: "main-focus" });
        })
        .then((unlisten) => {
          unlistenFocusChanged = unlisten;
        })
        .catch(() => {});
    });

    return () => {
      unlistenFocusChanged?.();
    };
  }, [modalChildWindowCount]);

  return modalChildWindowCount;
}
