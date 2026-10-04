import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { BrowserTerminals } from "./terminal";
import { browserListen } from "./events";

class Socket {
  static OPEN = 1;
  static instances: Socket[] = [];
  readyState = 0;
  bufferedAmount = 0;
  binaryType = "";
  onopen?: () => void;
  onclose?: () => void;
  onerror?: () => void;
  onmessage?: (event: { data: unknown }) => void;
  send = vi.fn();
  constructor() {
    Socket.instances.push(this);
  }
  open() {
    this.readyState = 1;
    this.onopen?.();
  }
  close() {
    this.readyState = 3;
    this.onclose?.();
  }
}
let terminals: BrowserTerminals;
let probe: ReturnType<typeof vi.fn<(...args: unknown[]) => Promise<unknown>>>;
let closeRemote: ReturnType<
  typeof vi.fn<(...args: unknown[]) => Promise<unknown>>
>;
let unlisten: Array<() => void>;
beforeEach(() => {
  vi.useFakeTimers();
  vi.stubGlobal("WebSocket", Socket);
  vi.stubGlobal("BroadcastChannel", undefined);
  Socket.instances = [];
  probe = vi.fn().mockResolvedValue({ connected: true });
  closeRemote = vi.fn().mockResolvedValue(null);
  terminals = new BrowserTerminals(probe, closeRemote);
  unlisten = [];
});
afterEach(() => {
  terminals.stopAll();
  for (const stop of unlisten) stop();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});
async function closed(id = "test") {
  const callback = vi.fn();
  unlisten.push(await browserListen(`session-closed-${id}`, callback));
  return callback;
}
async function open() {
  const ready = terminals.attach("test");
  Socket.instances[Socket.instances.length - 1].open();
  await ready;
}

describe("browser terminal recovery", () => {
  it("shares concurrent attachment and retries a failed initial handshake", async () => {
    const callback = await closed();
    const ready = terminals.attach("test");
    expect(terminals.attach("test")).toBe(ready);
    Socket.instances[0].onerror?.();
    await vi.advanceTimersByTimeAsync(1000);
    expect(Socket.instances).toHaveLength(2);
    Socket.instances[1].open();
    await ready;
    expect(callback).not.toHaveBeenCalled();
  });
  it("survives multiple recovery handshake failures and ignores old callbacks", async () => {
    const callback = await closed();
    await open();
    const old = Socket.instances[0];
    old.close();
    await vi.advanceTimersByTimeAsync(1000);
    Socket.instances[1].onerror?.();
    await vi.advanceTimersByTimeAsync(2000);
    Socket.instances[2].onerror?.();
    await vi.advanceTimersByTimeAsync(4000);
    Socket.instances[3].open();
    await terminals.attach("test");
    old.onmessage?.({ data: JSON.stringify({ type: "closed" }) });
    old.onclose?.();
    await terminals.send("test", "ok");
    expect(Socket.instances[3].send).toHaveBeenCalledWith("ok");
    expect(callback).not.toHaveBeenCalled();
  });
  it("retries temporary session query failures", async () => {
    probe.mockRejectedValue({ status: 503 });
    const ready = terminals.attach("test");
    Socket.instances[0].onerror?.();
    await vi.advanceTimersByTimeAsync(1000);
    Socket.instances[1].open();
    await ready;
    expect(probe).toHaveBeenCalledOnce();
  });
  it("bounds hanging handshakes and emits closure once when the lease budget ends", async () => {
    const callback = await closed();
    const ready = terminals.attach("test");
    const rejected = expect(ready).rejects.toThrow();
    await vi.advanceTimersByTimeAsync(25_000);
    await rejected;
    expect(probe).toHaveBeenCalled();
    expect(callback).toHaveBeenCalledTimes(1);
    terminals.stop("test", true);
    expect(callback).toHaveBeenCalledTimes(1);
    expect(Socket.instances.every((socket) => socket.readyState === 3)).toBe(
      true,
    );
  });
  it.each([401, 404])(
    "stops immediately for a %i session probe",
    async (status) => {
      const callback = await closed();
      probe.mockRejectedValue({ status });
      const ready = terminals.attach("test");
      const rejected = expect(ready).rejects.toEqual({ status });
      Socket.instances[0].onerror?.();
      await rejected;
      await vi.advanceTimersByTimeAsync(30_000);
      expect(Socket.instances).toHaveLength(1);
      expect(callback).toHaveBeenCalledTimes(1);
    },
  );
  it.each(["stop", "logout"])(
    "cancels delayed retries on %s",
    async (action) => {
      const callback = await closed();
      await open();
      Socket.instances[0].close();
      await vi.advanceTimersByTimeAsync(500);
      if (action === "stop") terminals.stop("test");
      else terminals.stopAll();
      await vi.advanceTimersByTimeAsync(30_000);
      expect(Socket.instances).toHaveLength(1);
      expect(callback).not.toHaveBeenCalled();
    },
  );
  it("cancels a pending handshake on server closure", async () => {
    const callback = await closed();
    const ready = terminals.attach("test");
    const rejected = expect(ready).rejects.toThrow();
    terminals.stop("test", true);
    await rejected;
    await vi.advanceTimersByTimeAsync(30_000);
    expect(callback).toHaveBeenCalledTimes(1);
    expect(Socket.instances).toHaveLength(1);
  });
  it("keeps UTF-8 decoding across recovered attachments", async () => {
    const output: string[] = [];
    unlisten.push(
      await browserListen<{ data: string }>(
        "terminal-output-test",
        ({ payload }) => output.push(payload.data),
      ),
    );
    await open();
    const bytes = new TextEncoder().encode("你好");
    Socket.instances[0].onmessage?.({ data: bytes.slice(0, 2).buffer });
    Socket.instances[0].close();
    await vi.advanceTimersByTimeAsync(1000);
    Socket.instances[1].open();
    await terminals.attach("test");
    Socket.instances[1].onmessage?.({ data: bytes.slice(2).buffer });
    expect(output.join("")).toBe("你好");
  });
  it("preserves input backpressure", async () => {
    await open();
    Socket.instances[0].bufferedAmount = 2 * 1024 * 1024;
    await expect(terminals.send("test", "input")).rejects.toThrow(
      "queue is full",
    );
    expect(Socket.instances[0].send).not.toHaveBeenCalled();
  });
});
