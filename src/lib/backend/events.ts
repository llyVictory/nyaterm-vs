import type { Event, EventCallback, UnlistenFn } from "@tauri-apps/api/event";

type Handler = (event: Event<unknown>) => void;
const handlers = new Map<string, Set<Handler>>();
const pending = new Map<string, Array<{ payload: unknown; bytes: number }>>();
let eventId = 0;
let channel: BroadcastChannel | undefined;
export function deliver(event: string, payload: unknown): void {
  if (
    !handlers.get(event)?.size &&
    /^(terminal-output-|session-closed-|connection-error-)/.test(event)
  ) {
    const bytes = (payload as { bytes?: number } | null)?.bytes ?? 0;
    const queue = pending.get(event) ?? [];
    if (
      queue.length >= 64 ||
      queue.reduce((sum, item) => sum + item.bytes, bytes) > 2 * 1024 * 1024
    ) {
      throw new Error("Terminal output listener is unavailable");
    }
    queue.push({ payload, bytes });
    pending.set(event, queue);
    return;
  }
  const message = { event, payload, id: ++eventId };
  for (const handler of handlers.get(event) ?? []) handler(message);
}
function getChannel(): BroadcastChannel | undefined {
  if (!channel && typeof BroadcastChannel !== "undefined") {
    channel = new BroadcastChannel("nyaterm-ui-events");
    channel.onmessage = ({ data }) => deliver(data.event, data.payload);
  }
  return channel;
}
export async function browserListen<T>(
  event: string,
  callback: EventCallback<T>,
): Promise<UnlistenFn> {
  getChannel();
  let listeners = handlers.get(event);
  if (!listeners) {
    listeners = new Set();
    handlers.set(event, listeners);
  }
  listeners.add(callback as Handler);
  const queued = pending.get(event);
  pending.delete(event);
  for (const item of queued ?? []) deliver(event, item.payload);
  return () => {
    listeners?.delete(callback as Handler);
    if (!listeners?.size) handlers.delete(event);
  };
}
export async function browserEmit<T>(
  event: string,
  payload?: T,
): Promise<void> {
  deliver(event, payload);
  getChannel()?.postMessage({ event, payload });
}
