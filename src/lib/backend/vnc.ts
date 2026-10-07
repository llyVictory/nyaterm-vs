import { backendURL } from "./runtime";
import { deliver } from "./events";

interface Stream {
  socket?: WebSocket;
  ready: Promise<void>;
  closed: boolean;
  everOpened: boolean;
  retry?: number;
  stop: () => void;
  onFrame: (frame: ArrayBuffer) => void;
  onError: (error: Error) => void;
}
const streams = new Map<string, Stream>();

function connect(id: string, stream: Stream): Promise<void> {
  const url = backendURL(`api/sessions/${encodeURIComponent(id)}/vnc`);
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  const socket = new WebSocket(url);
  socket.binaryType = "arraybuffer";
  stream.socket = socket;
  let opened = false;
  const ready = new Promise<void>((resolve, reject) => {
    socket.onopen = () => {
      if (stream.closed) {
        socket.close();
        return;
      }
      opened = true;
      stream.everOpened = true;
      resolve();
    };
    socket.onerror = () => reject(new Error("VNC WebSocket unavailable"));
    socket.onmessage = ({ data }) => {
      if (stream.closed || stream.socket !== socket) return;
      try {
        if (typeof data !== "string") stream.onFrame(data);
        else {
          const value = JSON.parse(data);
          if (value.type === "event") deliver(value.event, value.payload);
          if (value.type === "error") stream.onError(new Error(value.error));
        }
      } catch (error) {
        stream.onError(
          error instanceof Error ? error : new Error(String(error)),
        );
      }
    };
    socket.onclose = () => {
      if (!opened) reject(new Error("VNC WebSocket closed"));
      if (stream.closed || stream.socket !== socket || !stream.everOpened)
        return;
      deliver(`vnc-state-${id}`, {
        sessionId: id,
        state: "reconnecting",
        message: "",
      });
      stream.retry = window.setTimeout(() => {
        if (stream.closed) return;
        stream.ready = connect(id, stream);
        void stream.ready.catch(stream.onError);
      }, 1000);
    };
  });
  return ready;
}
export async function subscribeBrowserVnc(
  id: string,
  onFrame: Stream["onFrame"],
  onError: Stream["onError"],
  signal?: AbortSignal,
): Promise<() => void> {
  closeBrowserVnc(id);
  const stream: Stream = {
    ready: Promise.resolve(),
    closed: false,
    everOpened: false,
    onFrame,
    onError,
    stop: () => {},
  };
  const stop = () => {
    stream.closed = true;
    window.clearTimeout(stream.retry);
    stream.socket?.close();
    signal?.removeEventListener("abort", stop);
    if (streams.get(id) === stream) streams.delete(id);
  };
  stream.stop = stop;
  if (signal?.aborted)
    throw new DOMException("VNC subscription cancelled", "AbortError");
  signal?.addEventListener("abort", stop, { once: true });
  streams.set(id, stream);
  // A previous socket can still be detaching at the server (React StrictMode,
  // pane moves). Retry initial attachment without marking the VNC session failed.
  stream.ready = (async () => {
    for (let attempt = 0; ; attempt++) {
      if (stream.closed)
        throw new DOMException("VNC subscription cancelled", "AbortError");
      try {
        await connect(id, stream);
        return;
      } catch (error) {
        stream.socket?.close();
        if (stream.closed || attempt >= 4) throw error;
        await new Promise<void>((resolve) => window.setTimeout(resolve, 250));
      }
    }
  })();
  try {
    await stream.ready;
  } catch (error) {
    stop();
    throw error;
  }
  return stop;
}

export async function sendBrowserVnc(
  id: string,
  value: unknown,
): Promise<void> {
  const stream = streams.get(id);
  if (!stream || stream.closed) throw new Error("VNC display is detached");
  await stream.ready;
  const socket = stream.socket;
  if (!socket || socket.readyState !== WebSocket.OPEN)
    throw new Error("VNC is disconnected");
  if (socket.bufferedAmount > 1024 * 1024)
    throw new Error("VNC input queue is full");
  socket.send(JSON.stringify(value));
}
export function closeBrowserVnc(id: string): void {
  const stream = streams.get(id);
  if (!stream) return;
  stream.stop();
}
export function closeAllBrowserVnc(): void {
  for (const id of streams.keys()) closeBrowserVnc(id);
}
