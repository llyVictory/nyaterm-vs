import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  pick: vi.fn(),
  export: vi.fn(),
  import: vi.fn(),
  diagnostics: vi.fn(),
  nativeOpen: vi.fn(),
  nativeSave: vi.fn(),
  success: vi.fn(),
  error: vi.fn(),
}));
vi.mock("@/lib/backend/runtime", () => ({ runtime: "web" }));
vi.mock("@/lib/backend/browserArtifacts", () => ({ pickBrowserFile: mocks.pick }));
vi.mock("@/lib/backend/platform/dialog", () => ({
  open: mocks.nativeOpen,
  save: mocks.nativeSave,
}));
vi.mock("@/lib/backend/configTransfer", () => ({
  exportBrowserConfig: mocks.export,
  importBrowserConfig: mocks.import,
  exportBrowserDiagnostics: mocks.diagnostics,
}));
vi.mock("@/context/AppContext", () => ({
  useApp: () => ({ appSettings: { security: { master_password: null } } }),
}));
vi.mock("@/lib/logger", () => ({ logger: { error: vi.fn(), flush: vi.fn() } }));
vi.mock("@/lib/windowManager", () => ({ openSettings: vi.fn() }));
vi.mock("@/lib/invoke", () => ({ invoke: vi.fn() }));
vi.mock("sonner", () => ({ toast: { success: mocks.success, error: mocks.error } }));
vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (key: string) => key }) }));

import { useConfigTransfer } from "./useConfigTransfer";

function Harness() {
  const flow = useConfigTransfer();
  return (
    <>
      <button onClick={() => void flow.handleExport()}>Export</button>
      <button onClick={() => void flow.handleImport()}>Import</button>
      <button disabled={flow.transferBusy} onClick={() => void flow.handleExportDiagnostics()}>
        Diagnostics
      </button>
      {flow.passwordAlert}
    </>
  );
}
beforeEach(() => {
  vi.clearAllMocks();
  mocks.pick.mockResolvedValue(null);
  mocks.export.mockResolvedValue(undefined);
  mocks.import.mockResolvedValue(undefined);
  mocks.diagnostics.mockResolvedValue(undefined);
});
describe("Web configuration transfer", () => {
  it("exports with a confirmed password without invoking native files", async () => {
    render(<Harness />);
    fireEvent.click(screen.getByText("Export"));
    fireEvent.change(screen.getByLabelText("web.backupPassword"), {
      target: { value: "backup-pass" },
    });
    fireEvent.change(screen.getByLabelText("web.backupPasswordConfirm"), {
      target: { value: "wrong" },
    });
    expect(
      screen.getByRole("button", { name: "web.backupExportTitle" }).hasAttribute("disabled"),
    ).toBe(true);
    fireEvent.change(screen.getByLabelText("web.backupPasswordConfirm"), {
      target: { value: "backup-pass" },
    });
    await act(async () =>
      fireEvent.click(screen.getByRole("button", { name: "web.backupExportTitle" })),
    );
    expect(mocks.export).toHaveBeenCalledWith("backup-pass");
    expect(mocks.nativeSave).not.toHaveBeenCalled();
    expect(screen.queryByRole("dialog")).toBeNull();
  });
  it("treats picker cancellation as a normal outcome", async () => {
    render(<Harness />);
    await act(async () => fireEvent.click(screen.getByText("Import")));
    expect(mocks.import).not.toHaveBeenCalled();
    expect(mocks.error).not.toHaveBeenCalled();
    expect(mocks.nativeOpen).not.toHaveBeenCalled();
  });
  it("requires overwrite confirmation and permits retry after failure", async () => {
    const file = new File(["backup"], "fixture.nya");
    mocks.pick.mockResolvedValue(file);
    mocks.import.mockRejectedValueOnce(new Error("incorrect password"));
    render(<Harness />);
    await act(async () => fireEvent.click(screen.getByText("Import")));
    fireEvent.change(screen.getByLabelText("web.backupPassword"), {
      target: { value: "backup-pass" },
    });
    expect(
      screen.getByRole("button", { name: "web.backupImportTitle" }).hasAttribute("disabled"),
    ).toBe(true);
    fireEvent.click(screen.getByRole("checkbox"));
    await act(async () =>
      fireEvent.click(screen.getByRole("button", { name: "web.backupImportTitle" })),
    );
    expect(screen.getByRole("alert").textContent).toContain("web.backupImportFailed");
    await act(async () =>
      fireEvent.click(screen.getByRole("button", { name: "web.backupImportTitle" })),
    );
    expect(mocks.import).toHaveBeenCalledTimes(2);
    expect(mocks.import).toHaveBeenLastCalledWith(file, "backup-pass");
    expect(mocks.nativeOpen).not.toHaveBeenCalled();
  });
  it("downloads diagnostics without opening a save dialog", async () => {
    render(<Harness />);
    await act(async () => fireEvent.click(screen.getByText("Diagnostics")));
    expect(mocks.diagnostics).toHaveBeenCalledOnce();
    expect(mocks.nativeSave).not.toHaveBeenCalled();
  });
});
