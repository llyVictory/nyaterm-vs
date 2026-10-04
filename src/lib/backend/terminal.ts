import { backendURL } from "./runtime";
import { deliver } from "./events";

interface TerminalStream {
  id: string;
  socket?: WebSocket;
  ready?: Promise<void>;
  decoder: TextDecoder;
  stopped: boolean;
  closedDelivered: boolean;
  recovery?: AbortController;
}

/** A recovery cycle fits inside the server's 30-second attachment lease. */
export class BrowserTerminals {
  private streams = new Map<string, TerminalStream>();
  constructor(
    private probe: (id: string, signal: AbortSignal) => Promise<unknown>,
    private closeRemote: (id: string) => Promise<unknown>,
  ) {}

  attach(id: string): Promise<void> {
    let stream = this.streams.get(id);
    if (!stream) {
      stream = {
        id,
        decoder: new TextDecoder(),
        stopped: false,
        closedDelivered: false,
      };
      this.streams.set(id, stream);
    }
    if (stream.stopped) return Promise.reject(new Error("Terminal is closed"));
    if (stream.ready) return stream.ready;
    if (stream.socket?.readyState === WebSocket.OPEN) return Promise.resolve();
    return this.recover(stream);
  }

  private recover(stream: TerminalStream, delay = 0): Promise<void> {
    const controller = new AbortController();
    stream.recovery = controller;
    const deadline = window.setTimeout(() => controller.abort(), 25_000);
    const run = async () => {
      let retry = delay ? 1 : 0;
      try {
        if (delay) await this.wait(delay, controller.signal);
        for (;;) {
          controller.signal.throwIfAborted();
          try {
            await this.connect(stream, controller.signal);
            return;
          } catch (error) {
            controller.signal.throwIfAborted();
            try {
              await this.probe(stream.id, controller.signal);
            } catch (probeError) {
              const status = (probeError as { status?: number })?.status;
              if (status === 401 || status === 404) throw probeError;
            }
            controller.signal.throwIfAborted();
            await this.wait(
              Math.min(1000 * 2 ** retry++, 5000),
              controller.signal,
            );
            // Failed handshakes do not mean the remote session has ended.
            void error;
          }
        }
      } catch (error) {
        if (!stream.stopped) this.stop(stream.id, true);
        throw error;
      } finally {
        window.clearTimeout(deadline);
        if (stream.recovery === controller) {
          stream.recovery = undefined;
          stream.ready = undefined;
        }
      }
    };
    const ready = run();
    stream.ready = ready;
    // Socket-driven recovery has no awaiting caller; still retain rejection for attach/send.
    void ready.catch(() => {});
    return ready;
  }

  private wait(ms: number, signal: AbortSignal): Promise<void> {
    return new Promise((resolve, reject) => {
      const finish = () => {
        signal.removeEventListener("abort", cancel);
        resolve();
      };
      const timer = window.setTimeout(finish, ms);
      const cancel = () => {
        window.clearTimeout(timer);
        signal.removeEventListener("abort", cancel);
        reject(new Error("Terminal recovery cancelled"));
      };
      signal.addEventListener("abort", cancel, { once: true });
      if (signal.aborted) cancel();
    });
  }

  private connect(stream: TerminalStream, signal: AbortSignal): Promise<void> {
    const url = backendURL(
      `api/sessions/${encodeURIComponent(stream.id)}/terminal`,
    );
    url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
    const socket = new WebSocket(url);
    socket.binaryType = "arraybuffer";
    stream.socket = socket;
    return new Promise((resolve, reject) => {
      let opened = false;
      let settled = false;
      const current = () => !stream.stopped && stream.socket === socket;
      const cleanup = () => {
        window.clearTimeout(timer);
        signal.removeEventListener("abort", fail);
      };
      const fail = () => {
        if (settled) return;
        settled = true;
        cleanup();
        if (stream.socket === socket) stream.socket = undefined;
        socket.close();
        reject(new Error("Terminal WebSocket unavailable"));
      };
      const timer = window.setTimeout(fail, 5000);
      signal.addEventListener("abort", fail, { once: true });
      socket.onopen = () => {
        if (!current() || settled) {
          socket.close();
          return;
        }
        opened = settled = true;
        cleanup();
        resolve();
      };
      socket.onerror = () => {
        if (!opened) fail();
      };
      socket.onclose = () => {
        if (!opened) {
          fail();
          return;
        }
        if (!current()) return;
        stream.socket = undefined;
        // Usually ready has cleared by now. A same-tick close waits for its finally.
        void Promise.resolve(stream.ready)
          .catch(() => {})
          .then(() => {
            if (!stream.stopped && !stream.socket && !stream.ready)
              void this.recover(stream, 1000);
          });
      };
      socket.onmessage = ({ data }) => {
        if (!current()) return;
        try {
          if (typeof data !== "string") {
            deliver(`terminal-output-${stream.id}`, {
              data: stream.decoder.decode(data, { stream: true }),
              bytes: data.byteLength,
            });
          } else {
            const event = JSON.parse(data);
            if (event.type === "error")
              deliver(`session-error-${stream.id}`, event.error);
            if (event.type === "closed" || event.type === "error")
              this.stop(stream.id, true);
          }
        } catch {
          this.stop(stream.id, true);
          void this.closeRemote(stream.id).catch(() => {});
        }
      };
      if (signal.aborted) fail();
    });
  }

  async send(id: string, message: string | Uint8Array): Promise<void> {
    await this.attach(id);
    const socket = this.streams.get(id)?.socket;
    if (!socket || socket.readyState !== WebSocket.OPEN)
      throw new Error("Terminal is disconnected");
    if (socket.bufferedAmount > 1024 * 1024)
      throw new Error("Terminal input queue is full");
    socket.send(message);
  }

  stop(id: string, notify = false): void {
    let stream = this.streams.get(id);
    if (!stream) {
      stream = {
        id,
        decoder: new TextDecoder(),
        stopped: true,
        closedDelivered: false,
      };
      this.streams.set(id, stream);
    }
    stream.stopped = true;
    stream.recovery?.abort();
    stream.socket?.close();
    stream.socket = undefined;
    if (notify && !stream.closedDelivered) {
      stream.closedDelivered = true;
      deliver(`session-closed-${id}`, null);
    }
  }

  stopAll(): void {
    for (const id of this.streams.keys()) this.stop(id);
  }
}
