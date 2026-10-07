import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/lib/backend/runtime", () => ({ runtime: "web" }));
vi.mock("@/lib/backend/api", () => ({ invoke: vi.fn() }));

describe("browser diagnostic transport", () => {
  beforeEach(() => {
    vi.resetModules();
    vi.useFakeTimers();
    for (const method of ["info", "warn", "error", "debug"] as const)
      vi.spyOn(console, method).mockImplementation(() => {});
  });
  afterEach(() => {
    vi.clearAllTimers();
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("holds entries before authentication and drains all batches for diagnostics", async () => {
    const { logger } = await import("./logger");
    const { configureBrowserLogs } = await import("./backend/browserDiagnostics");
    for (let index = 0; index < 130; index++)
      logger.info({
        domain: "app.lifecycle",
        event: "fixture.entry",
        message: "Fixture",
        ids: { request_id: logger.createRequestId() },
      });
    await logger.flush();
    const send = vi.fn().mockResolvedValue(undefined);
    configureBrowserLogs(send);
    await logger.flush();
    expect(send.mock.calls.flatMap(([entries]) => entries)).toHaveLength(130);
    expect(send.mock.calls.every(([entries]) => entries.length <= 50)).toBe(true);
    expect(send.mock.calls[0][0][0].ids.request_id).toMatch(/^[0-9a-f-]{36}$/);
  });

  it("waits for an upload already in flight and bounds stalled queues", async () => {
    const { logger } = await import("./logger");
    const { configureBrowserLogs } = await import("./backend/browserDiagnostics");
    let release!: () => void;
    const send = vi
      .fn()
      .mockImplementationOnce(
        () =>
          new Promise<void>((resolve) => {
            release = resolve;
          }),
      )
      .mockResolvedValue(undefined);
    configureBrowserLogs(send);
    await logger.flush();
    for (let index = 0; index < 1200; index++)
      logger.info({ domain: "app.lifecycle", event: "fixture.entry", message: "Fixture" });
    let completed = false;
    const flush = logger.flush().then(() => {
      completed = true;
    });
    await Promise.resolve();
    expect(completed).toBe(false);
    release();
    await flush;
    const entries = send.mock.calls.flatMap(([batch]) => batch);
    expect(entries.length).toBeLessThan(1200);
    expect(entries.some((entry) => entry.event === "logger.queue_overflow")).toBe(true);
  });

  it("retries offline batches twice with backoff and avoids recursive failure logs", async () => {
    const { logger } = await import("./logger");
    const { configureBrowserLogs } = await import("./backend/browserDiagnostics");
    const send = vi.fn().mockRejectedValue(new Error("offline"));
    configureBrowserLogs(send);
    logger.error({
      domain: "ui.error",
      event: "fixture.failure",
      message: "Fixture",
      data: { password: "secret" },
    });
    await logger.flush();
    expect(send).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(999);
    expect(send).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(send).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(2000);
    expect(send).toHaveBeenCalledTimes(3);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(send).toHaveBeenCalledTimes(3);
    expect(console.error).toHaveBeenCalledTimes(1);
    send.mockResolvedValue(undefined);
    logger.info({ domain: "app.lifecycle", event: "fixture.recovered", message: "Recovered" });
    await logger.flush();
    expect(
      send.mock.calls[send.mock.calls.length - 1]?.[0].some(
        (entry: { event: string }) => entry.event === "logger.queue_overflow",
      ),
    ).toBe(true);
    expect(JSON.stringify(send.mock.calls)).not.toContain('"password":"secret"');
  });
});
