import { createEvent, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { KeyManagementTab } from "./KeyManagementTab";
import { PasswordManagementTab } from "./PasswordManagementTab";

const { invokeMock, errorMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  errorMock: vi.fn(),
}));
vi.mock("@/lib/invoke", () => ({ invoke: invokeMock }));
vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));
vi.mock("sonner", () => ({ toast: { error: errorMock } }));
vi.mock("./SecretUnlockFooter", () => ({ SecretUnlockFooter: () => null }));
vi.mock("./CopyButton", () => ({ CopyButton: () => null }));
vi.mock("@/components/dialog/security-auth/KeyEditorDialog", () => ({
  KeyEditorDialog: () => null,
}));
vi.mock("@/components/dialog/security-auth/KeyDeleteDialog", () => ({
  KeyDeleteDialog: () => null,
}));
vi.mock("@/components/dialog/security-auth/PrivateKeyViewDialog", () => ({
  PrivateKeyViewDialog: () => null,
}));

const entries = ["Alpha", "Beta", "Gamma"].map((name, index) => ({
  id: name.toLowerCase(),
  name,
  username: "root",
  sort_order: index,
  has_password: true,
  has_key_data: true,
}));

function row(name: string) {
  const element = screen.getByText(name).closest<HTMLDivElement>(".security-auth-action-row");
  if (!element) throw new Error(`Missing row ${name}`);
  return element;
}

function drag(source: string, target: string, position: "before" | "after") {
  const handle = row(source).querySelector("button");
  if (!handle) throw new Error("Missing drag handle");
  const dataTransfer = { setData: vi.fn(), effectAllowed: "", dropEffect: "" };
  fireEvent.dragStart(handle, { dataTransfer });
  for (const type of ["dragOver", "drop"] as const) {
    const event = createEvent[type](row(target), { dataTransfer });
    Object.defineProperty(event, "clientY", {
      value: position === "before" ? -1 : 1,
    });
    fireEvent(row(target), event);
  }
}

function names() {
  return Array.from(document.querySelectorAll(".security-auth-action-row")).map(
    (element) => entries.find((entry) => element.textContent?.includes(entry.name))?.name,
  );
}

describe.each([
  {
    Panel: PasswordManagementTab,
    list: "get_saved_passwords",
    command: "reorder_passwords",
    label: "passwordManager.dragToSort",
    failure: "passwordManager.reorderFailed",
  },
  {
    Panel: KeyManagementTab,
    list: "get_ssh_keys",
    command: "reorder_ssh_keys",
    label: "settings.keyDragToSort",
    failure: "settings.keyReorderFailed",
  },
])("$command", ({ Panel, list, command, label, failure }) => {
  beforeEach(() => {
    invokeMock.mockReset();
    errorMock.mockReset();
    invokeMock.mockImplementation((name: string) => {
      if (name === list) return Promise.resolve(entries);
      if (name === "get_saved_connections") return Promise.resolve([]);
      if (name === command) return Promise.resolve();
      return Promise.reject(new Error(`Unexpected command ${name}`));
    });
  });

  it.each([
    ["Gamma", "Alpha", "before", ["Gamma", "Alpha", "Beta"]],
    ["Alpha", "Gamma", "after", ["Beta", "Gamma", "Alpha"]],
  ] as const)("moves %s relative to %s while locked", async (source, target, position, expected) => {
    render(<Panel />);
    await screen.findByText("Alpha");
    drag(source, target, position);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(command, {
        updates: expected.map((name, sort_order) => ({
          id: name.toLowerCase(),
          sort_order,
        })),
      }),
    );
    expect(names()).toEqual(expected);
  });

  it("skips self drops and unchanged order", async () => {
    render(<Panel />);
    await screen.findByText("Alpha");
    drag("Alpha", "Alpha", "before");
    drag("Alpha", "Beta", "before");
    expect(invokeMock).not.toHaveBeenCalledWith(command, expect.anything());
    expect(names()).toEqual(["Alpha", "Beta", "Gamma"]);
  });

  it("clears drag state on cancellation", async () => {
    render(<Panel />);
    await screen.findByText("Alpha");
    const handle = screen.getAllByRole("button", { name: label })[0];
    fireEvent.dragStart(handle, { dataTransfer: { setData: vi.fn() } });
    expect(row("Alpha").style.opacity).toBe("0.5");
    fireEvent.dragEnd(handle);
    expect(row("Alpha").style.opacity).toBe("");
    expect(invokeMock).not.toHaveBeenCalledWith(command, expect.anything());
  });

  it("disables actions while saving and prevents a second reorder", async () => {
    let resolveSave = () => {};
    invokeMock.mockImplementation((name: string) => {
      if (name === command)
        return new Promise<void>((resolve) => {
          resolveSave = resolve;
        });
      return Promise.resolve(name === list ? entries : []);
    });
    render(<Panel />);
    await screen.findByText("Alpha");
    drag("Gamma", "Alpha", "before");
    expect(
      screen
        .getAllByRole("button", { name: label })
        .every((button) => button.hasAttribute("disabled")),
    ).toBe(true);
    expect(row("Alpha").querySelectorAll("button:enabled")).toHaveLength(0);
    drag("Alpha", "Beta", "after");
    expect(invokeMock.mock.calls.filter(([name]) => name === command)).toHaveLength(1);
    resolveSave();
    await waitFor(() =>
      expect(screen.getAllByRole("button", { name: label })[0].hasAttribute("disabled")).toBe(
        false,
      ),
    );
  });

  it("restores the previous order even when the failure reload also fails", async () => {
    let loads = 0;
    invokeMock.mockImplementation((name: string) => {
      if (name === list)
        return ++loads === 1 ? Promise.resolve(entries) : Promise.reject(new Error("offline"));
      if (name === command) return Promise.reject(new Error("save failed"));
      return Promise.resolve([]);
    });
    render(<Panel />);
    await screen.findByText("Alpha");
    drag("Gamma", "Alpha", "before");
    await waitFor(() => expect(errorMock).toHaveBeenCalledWith(failure));
    expect(names()).toEqual(["Alpha", "Beta", "Gamma"]);
    await waitFor(() => expect(loads).toBe(2));
  });

  it("disables dragging while editing", async () => {
    render(<Panel secretsUnlocked />);
    await screen.findByText("Alpha");
    const handle = screen.getAllByRole("button", { name: label })[0];
    fireEvent.click(screen.getAllByRole("button", { name: "common.edit" })[0]);
    await waitFor(() => expect(handle.hasAttribute("disabled")).toBe(true));
  });

  it("disables the handle for a single entry", async () => {
    invokeMock.mockResolvedValue([entries[0]]);
    render(<Panel />);
    await screen.findByText("Alpha");
    expect(screen.getByRole("button", { name: label }).hasAttribute("disabled")).toBe(true);
    expect(screen.getByRole("button", { name: label }).getAttribute("draggable")).toBe("false");
  });
});
