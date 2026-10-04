import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ authenticate: vi.fn(), startEvents: vi.fn() }));
vi.mock("./http", () => ({
  ...mocks,
  BackendRequestError: class extends Error {
    constructor(
      message: string,
      public status: number,
    ) {
      super(message);
    }
  },
}));
vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (key: string) => key }) }));

import { BrowserGate } from "./BrowserGate";
import { BackendRequestError } from "./http";

beforeEach(() => {
  vi.clearAllMocks();
  mocks.authenticate
    .mockRejectedValueOnce(new BackendRequestError("required", 401))
    .mockResolvedValue(undefined);
  mocks.startEvents.mockResolvedValue(undefined);
});
const mount = () =>
  render(
    <BrowserGate>
      <div>Workspace</div>
    </BrowserGate>,
  );
async function signIn() {
  await screen.findByRole("button", { name: "web.signIn" });
  fireEvent.change(screen.getByLabelText("web.password"), {
    target: { value: "fixture-password" },
  });
  fireEvent.submit(screen.getByLabelText("web.password").closest("form") as HTMLFormElement);
}
describe("Browser login", () => {
  it("retries the event connection without signing in again", async () => {
    mocks.startEvents.mockRejectedValueOnce(new Error("offline"));
    mount();
    await signIn();
    expect((await screen.findByRole("alert")).textContent).toContain("web.eventConnectionFailed");
    expect(screen.queryByLabelText("web.password")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "web.retryConnection" }));
    expect(await screen.findByText("Workspace")).toBeTruthy();
    expect(mocks.authenticate).toHaveBeenCalledTimes(2);
    expect(mocks.startEvents).toHaveBeenCalledTimes(2);
  });
  it.each([
    [401, "web.invalidPassword"],
    [429, "web.signInRateLimited"],
    [503, "web.serverUnavailable"],
  ])("distinguishes status %s", async (status, key) => {
    mocks.authenticate
      .mockReset()
      .mockRejectedValueOnce(new BackendRequestError("required", 401))
      .mockRejectedValueOnce(new BackendRequestError("failed", Number(status)));
    mount();
    await signIn();
    expect((await screen.findByRole("alert")).textContent).toContain(String(key));
    expect(screen.queryByText("Workspace")).toBeNull();
  });
  it("toggles visibility and prevents duplicate submissions during authentication", async () => {
    let resolve!: () => void;
    mocks.authenticate
      .mockReset()
      .mockRejectedValueOnce(new BackendRequestError("required", 401))
      .mockImplementationOnce(
        () =>
          new Promise<void>((done) => {
            resolve = done;
          }),
      );
    mount();
    await screen.findByRole("button", { name: "web.signIn" });
    fireEvent.click(screen.getByRole("button", { name: "web.showPassword" }));
    expect(screen.getByLabelText("web.password").getAttribute("type")).toBe("text");
    await signIn();
    fireEvent.submit(screen.getByLabelText("web.password").closest("form") as HTMLFormElement);
    expect(mocks.authenticate).toHaveBeenCalledTimes(2);
    await act(async () => resolve());
    await waitFor(() => expect(screen.getByText("Workspace")).toBeTruthy());
  });
});
