import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TransferTab } from "./TransferTab";

const state = vi.hoisted(() => ({
  web: true,
  update: vi.fn(),
  downloadDir: vi.fn(async () => "/downloads"),
}));
const transfer = {
  duplicate_strategy: "ask",
  editor_type: "external",
  default_editor: "desktop-editor",
  internal_editor_display: "workspace",
  download_path: "C:/saved",
  download_threads: 3,
  upload_threads: 4,
  max_transfer_retries: 2,
  transfer_buffer_size: 64,
  default_file_permissions: "644",
  ask_save_location: true,
  preserve_timestamps: true,
  resume_broken_transfer: true,
};
vi.mock("@/context/AppContext", () => ({
  useApp: () => ({
    appSettings: { transfer },
    updateAppSettings: state.update,
  }),
}));
vi.mock("@/lib/backend/runtime", async (original) => ({
  ...(await original<typeof import("@/lib/backend/runtime")>()),
  supports: () => !state.web,
}));
vi.mock("@/lib/backend/platform/path", () => ({
  downloadDir: state.downloadDir,
}));
vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));
beforeEach(() => {
  state.web = true;
  state.update.mockClear();
  state.downloadDir.mockClear();
});

describe("transfer capability settings", () => {
  it("shows only Web conflict and display settings, preserving desktop fields on edits", () => {
    render(<TransferTab />);
    expect(screen.getAllByRole("combobox")).toHaveLength(2);
    expect(screen.queryByText("settings.downloadPath")).toBeNull();
    expect(screen.queryByText("settings.editorType")).toBeNull();
    expect(screen.queryByText("settings.uploadConcurrentTasks")).toBeNull();
    expect(state.downloadDir).not.toHaveBeenCalled();
    fireEvent.keyDown(screen.getAllByRole("combobox")[0], { key: "ArrowDown" });
    fireEvent.click(
      screen.getByRole("option", { name: "settings.strategySkip" }),
    );
    expect(state.update).toHaveBeenCalledWith({
      transfer: { ...transfer, duplicate_strategy: "skip" },
    });
  });
  it("retains desktop path, editor and tuning settings", () => {
    state.web = false;
    render(<TransferTab />);
    expect(screen.getByText("settings.downloadPath")).toBeTruthy();
    expect(screen.getByText("settings.editorType")).toBeTruthy();
    expect(screen.getByText("settings.defaultEditor")).toBeTruthy();
    expect(screen.getByText("settings.uploadConcurrentTasks")).toBeTruthy();
    expect(state.downloadDir).toHaveBeenCalledOnce();
  });
});
