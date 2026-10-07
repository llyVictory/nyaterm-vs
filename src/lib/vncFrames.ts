import { Channel } from "@tauri-apps/api/core";
import { runtime } from "./backend/runtime";
import { subscribeBrowserVnc } from "./backend/vnc";
import { invoke } from "./invoke";

const owners = new Map<string, object>();
const operations = new Map<string, Promise<unknown>>();
function enqueue(
  id: string,
  operation: () => Promise<unknown>,
): Promise<unknown> {
  const next = (operations.get(id) ?? Promise.resolve())
    .catch(() => {})
    .then(operation);
  operations.set(id, next);
  void next
    .finally(() => {
      if (operations.get(id) === next) operations.delete(id);
    })
    .catch(() => {});
  return next;
}
export async function subscribeVncFrames(
  sessionId: string,
  onFrame: (frame: ArrayBuffer) => void,
  onError: (error: Error) => void,
  signal?: AbortSignal,
): Promise<() => void> {
  if (runtime === "web")
    return subscribeBrowserVnc(sessionId, onFrame, onError, signal);
  const owner = {};
  owners.set(sessionId, owner);
  const channel = new Channel<ArrayBuffer>((frame) => {
    if (owners.get(sessionId) === owner && !signal?.aborted) onFrame(frame);
  });
  const stop = () => {
    signal?.removeEventListener("abort", stop);
    channel.onmessage = () => {};
    if (owners.get(sessionId) !== owner) return;
    owners.delete(sessionId);
    void enqueue(sessionId, () =>
      invoke("vnc_detach_frame_channel", { sessionId }),
    ).catch(onError);
  };
  signal?.addEventListener("abort", stop, { once: true });
  if (signal?.aborted) {
    stop();
    return stop;
  }
  try {
    await enqueue(sessionId, async () => {
      if (owners.get(sessionId) === owner && !signal?.aborted)
        await invoke("vnc_attach_frame_channel", {
          sessionId,
          frameChannel: channel,
        });
    });
  } catch (error) {
    stop();
    throw error;
  }
  return stop;
}
