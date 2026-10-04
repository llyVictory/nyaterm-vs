/** Authentication-aware transport, kept separate to avoid logger / HTTP recursion. */
type LogSender = (entries: unknown[], keepalive: boolean) => Promise<void>;
let sender: LogSender | undefined;
const listeners = new Set<() => void>();
export function configureBrowserLogs(next: LogSender | undefined): void {
  sender = next;
  if (sender) for (const listener of listeners) listener();
}
export function browserLogsReady(): boolean {
  return !!sender;
}
export function onBrowserLogsReady(listener: () => void): void {
  listeners.add(listener);
}
export async function sendBrowserLogs(entries: unknown[], keepalive = false): Promise<void> {
  if (!sender) throw new Error("Browser log transport is not authenticated");
  await sender(entries, keepalive);
}

export function createBrowserRequestId(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}
