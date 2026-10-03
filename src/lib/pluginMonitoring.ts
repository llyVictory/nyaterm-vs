import { listen } from "@/lib/backend/api";
import { pluginApi } from "@/lib/plugins";
import type { PluginMonitorSnapshot, PluginMonitorSubscription } from "@/types/plugins";

/** Register first, then subscribe, so an immediate collection cannot be lost. */
export async function subscribePluginMonitor(
  token: string,
  monitorId: string,
  intervalSeconds: number,
  update: (snapshot: PluginMonitorSnapshot) => void,
) {
  let ownedId: string | null = null;
  let revision = -1;
  let disposed = false;
  const early = new Map<string, PluginMonitorSnapshot>();
  const deliver = (snapshot: PluginMonitorSnapshot) => {
    if (disposed || snapshot.revision <= revision) return;
    revision = snapshot.revision;
    update(snapshot);
  };
  const unlisten = await listen<PluginMonitorSubscription>(
    "plugin-monitor-updated",
    ({ payload }) => {
      if (disposed) return;
      if (ownedId === payload.subscriptionId) deliver(payload.snapshot);
      else if (!ownedId && early.size < 32) {
        const previous = early.get(payload.subscriptionId);
        if (!previous || payload.snapshot.revision > previous.revision)
          early.set(payload.subscriptionId, payload.snapshot);
      }
    },
  );
  try {
    const subscription = await pluginApi.subscribeMonitor(token, monitorId, intervalSeconds);
    ownedId = subscription.subscriptionId;
    deliver(subscription.snapshot);
    const buffered = early.get(ownedId);
    if (buffered) deliver(buffered);
    early.clear();
    return {
      subscriptionId: ownedId,
      snapshot:
        buffered && buffered.revision > subscription.snapshot.revision
          ? buffered
          : subscription.snapshot,
      refresh: () => pluginApi.refreshMonitor(token, subscription.subscriptionId),
      dispose: async () => {
        if (disposed) return;
        disposed = true;
        unlisten();
        await pluginApi.unsubscribeMonitor(token, subscription.subscriptionId);
      },
    };
  } catch (error) {
    disposed = true;
    unlisten();
    throw error;
  }
}
