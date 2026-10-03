import { listen } from "@/lib/backend/api";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { pluginApi } from "@/lib/plugins";
import type { PluginApprovalRequest } from "@/types/plugins";

export function PluginApprovalHost() {
  const { t } = useTranslation();
  const [requests, setRequests] = useState<PluginApprovalRequest[]>([]);
  const current = requests[0] ?? null;
  useEffect(() => {
    let disposed = false;
    const disposers: (() => void)[] = [];
    const register = (promise: Promise<() => void>) => {
      void promise
        .then((dispose) => (disposed ? dispose() : disposers.push(dispose)))
        .catch(() => {});
    };
    register(
      listen<PluginApprovalRequest>("plugin-approval-request", (event) => {
        if (!disposed)
          setRequests((items) =>
            items.some((item) => item.requestId === event.payload.requestId)
              ? items
              : [...items, event.payload],
          );
      }),
    );
    register(
      listen<string>("plugin-approval-closed", (event) => {
        if (!disposed)
          setRequests((items) =>
            items.filter((item) => item.requestId !== event.payload),
          );
      }),
    );
    register(
      listen<{ locked: boolean }>("app-lock-state-changed", (event) => {
        if (!disposed && event.payload.locked) setRequests([]);
      }),
    );
    return () => {
      disposed = true;
      for (const dispose of disposers) dispose();
    };
  }, []);
  const respond = (approved: boolean) => {
    if (!current) return;
    setRequests((items) =>
      items.filter((item) => item.requestId !== current.requestId),
    );
    void pluginApi.approve(current.requestId, approved).catch(() => {});
  };
  return (
    <AlertDialog
      open={current !== null}
      onOpenChange={(open) => !open && respond(false)}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t("plugins.approvalTitle")}</AlertDialogTitle>
          <AlertDialogDescription>
            {t("plugins.approvalDescription", { name: current?.pluginName })}
          </AlertDialogDescription>
        </AlertDialogHeader>
        {current && (
          <div className="space-y-2 rounded-md border p-3 text-sm">
            <p>
              {current.sessionName} · {current.capability} · {current.risk}
            </p>
            <pre className="max-h-64 overflow-auto whitespace-pre-wrap break-all rounded bg-muted p-2 text-xs">
              {current.summary}
            </pre>
          </div>
        )}
        <AlertDialogFooter>
          <Button variant="outline" onClick={() => respond(false)}>
            {t("plugins.deny")}
          </Button>
          <Button onClick={() => respond(true)}>
            {t("plugins.allowOnce")}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
