import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { SavedAccount, SavedConnection } from "@/types/global";
import { PasswordManagementTab } from "./PasswordManagementTab";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("@/lib/invoke", () => ({ invoke: invokeMock }));
vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));
vi.mock("./SecretUnlockFooter", () => ({ SecretUnlockFooter: () => null }));
vi.mock("./CopyButton", () => ({ CopyButton: () => null }));

const account: SavedAccount = {
  id: "account-1",
  name: "Production",
  username: "root",
  has_password: true,
};

const referencedConnection: SavedConnection = {
  id: "ssh-1",
  name: "Production SSH",
  type: "ssh",
  host: "example.com",
  port: 22,
  username: "fallback",
  auth: { mode: "key", account_id: account.id },
};

describe("PasswordManagementTab", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockImplementation((command: string) => {
      switch (command) {
        case "get_saved_passwords":
          return Promise.resolve([]);
        case "get_saved_connections":
          return Promise.resolve([]);
        case "save_password":
        case "delete_password":
          return Promise.resolve(undefined);
        default:
          return Promise.reject(new Error(`Unexpected command: ${command}`));
      }
    });
  });

  it("saves an account with a username and no password", async () => {
    render(<PasswordManagementTab secretsUnlocked />);

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("get_saved_passwords"));
    fireEvent.click(screen.getByRole("button", { name: "passwordManager.add" }));
    fireEvent.change(screen.getByPlaceholderText("passwordManager.namePlaceholder"), {
      target: { value: "Ops" },
    });
    fireEvent.change(screen.getByPlaceholderText("passwordManager.usernamePlaceholder"), {
      target: { value: "ops" },
    });
    fireEvent.click(screen.getByRole("button", { name: "common.save" }));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("save_password", {
        entry: {
          id: "",
          name: "Ops",
          username: "ops",
          password: "",
        },
      });
    });
  });

  it("warns before deleting an account referenced by a connection", async () => {
    invokeMock.mockImplementation((command: string) => {
      switch (command) {
        case "get_saved_passwords":
          return Promise.resolve([account]);
        case "get_saved_connections":
          return Promise.resolve([referencedConnection]);
        case "delete_password":
          return Promise.resolve(undefined);
        default:
          return Promise.reject(new Error(`Unexpected command: ${command}`));
      }
    });

    render(<PasswordManagementTab secretsUnlocked />);

    await screen.findByText(account.name);
    fireEvent.click(screen.getByRole("button", { name: "common.delete" }));

    expect(await screen.findByText("passwordManager.deleteReferencedWarning")).toBeTruthy();
    const deleteButtons = screen.getAllByRole("button", { name: "common.delete" });
    fireEvent.click(deleteButtons[deleteButtons.length - 1]);

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("delete_password", { id: account.id });
    });
  });
});
