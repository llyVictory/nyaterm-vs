import { listen } from "@/lib/backend/api";
import { useCallback, useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { invoke } from "@/lib/invoke";
import { logger } from "@/lib/logger";

export interface ExternalOpenRequest {
  id: string;
  rawUrl: string;
  source: "startupArguments" | "secondInstance" | "deepLink";
  targetWindowLabel: string;
  receivedAtMs: number;
}

interface UseExternalOpenRequestsOptions {
  ready: boolean;
  onRequest: (request: ExternalOpenRequest) => Promise<void>;
}

export function useExternalOpenRequests({ ready, onRequest }: UseExternalOpenRequestsOptions) {
  const { t } = useTranslation();
  const queueRef = useRef<ExternalOpenRequest[]>([]);
  const processingRef = useRef(false);
  const claimingRef = useRef(false);
  const claimAgainRef = useRef(false);
  const readyRef = useRef(ready);
  const onRequestRef = useRef(onRequest);

  useEffect(() => {
    readyRef.current = ready;
  }, [ready]);

  useEffect(() => {
    onRequestRef.current = onRequest;
  }, [onRequest]);

  const processQueue = useCallback(() => {
    if (processingRef.current) return;
    processingRef.current = true;

    const run = async () => {
      while (queueRef.current.length > 0) {
        const request = queueRef.current.shift();
        if (!request) continue;

        try {
          await onRequestRef.current(request);
        } catch (error) {
          logger.error({
            domain: "app.lifecycle",
            event: "external_open.processing_failed",
            message: "Failed to process external open request",
            ids: { request_id: request.id },
            data: {
              source: request.source,
              target_window_label: request.targetWindowLabel,
            },
            error,
          });
          toast.error(t("externalOpen.processingFailed"));
        }
      }
      processingRef.current = false;
    };

    void run();
  }, [t]);

  const claim = useCallback(async () => {
    if (!readyRef.current) return;
    if (claimingRef.current) {
      claimAgainRef.current = true;
      return;
    }

    claimingRef.current = true;
    try {
      const requests = await invoke<ExternalOpenRequest[]>("claim_external_open_requests");
      if (requests.length > 0) {
        queueRef.current.push(...requests);
        processQueue();
      }
    } catch (error) {
      logger.error({
        domain: "app.lifecycle",
        event: "external_open.processing_failed",
        message: "Failed to claim external open requests",
        error,
      });
      toast.error(t("externalOpen.processingFailed"));
    } finally {
      claimingRef.current = false;
      if (claimAgainRef.current) {
        claimAgainRef.current = false;
        void claim();
      }
    }
  }, [processQueue, t]);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;

    listen("external-open-available", () => {
      void claim();
    })
      .then((nextUnlisten) => {
        if (cancelled) {
          nextUnlisten();
          return;
        }
        unlisten = nextUnlisten;
        void claim();
      })
      .catch((error) => {
        logger.error({
          domain: "app.lifecycle",
          event: "external_open.processing_failed",
          message: "Failed to listen for external open requests",
          error,
        });
      });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [claim]);

  useEffect(() => {
    if (ready) void claim();
  }, [claim, ready]);
}
