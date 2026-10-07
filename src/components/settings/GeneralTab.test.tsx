import { fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { GeneralTab } from "./GeneralTab";

const state = vi.hoisted(() => ({
  isWindows: true,
  updateAppSettings: vi.fn(),
  updateUi: vi.fn(),
}));

vi.mock("@/lib/platform", () => ({
  get isWindows() {
    return state.isWindows;
  },
}));

vi.mock("@/context/AppContext", () => ({
  useApp: () => ({
    appSettings: {
      general: {
        startup_restore: true,
        startup_restore_window_layout: true,
        minimize_to_tray: false,
        boss_key: null,
        confirm_on_close: true,
        rdp_client_mode: "builtin",
      },
      ui: {
        language: "en",
        header_status_visible: true,
        header_status_mode: "session",
      },
      diagnostics: { level: "info", retention_days: 7 },
    },
    updateAppSettings: state.updateAppSettings,
    updateUi: state.updateUi,
  }),
}));

vi.mock("@/hooks/useConfigTransfer", () => ({
  useConfigTransfer: () => ({
    handleExportDiagnostics: vi.fn(),
    handleOpenLogs: vi.fn(),
  }),
}));

vi.mock("react-i18next", async (importOriginal) => ({
  ...(await importOriginal<typeof import("react-i18next")>()),
  useTranslation: () => ({
    i18n: { changeLanguage: vi.fn() },
    t: (key: string) => key,
  }),
}));

describe("GeneralTab RDP client setting", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    state.isWindows = true;
  });

  it("shows the RDP client selector on Windows and saves Windows Remote Desktop", () => {
    render(<GeneralTab />);
    const field =
      screen.getByText("settings.rdpClient").parentElement?.parentElement;
    expect(field).not.toBeNull();
    const combo = within(field as HTMLElement).getByRole("combobox");

    fireEvent.keyDown(combo, { key: "ArrowDown" });
    fireEvent.click(
      screen.getByRole("option", { name: "settings.rdpClientWindows" }),
    );

    expect(state.updateAppSettings).toHaveBeenCalledWith({
      general: {
        startup_restore: true,
        startup_restore_window_layout: true,
        minimize_to_tray: false,
        boss_key: null,
        confirm_on_close: true,
        rdp_client_mode: "windows",
      },
    });
  });

  it("does not show the Windows-only RDP client selector off Windows", () => {
    state.isWindows = false;
    render(<GeneralTab />);
    expect(screen.queryByText("settings.rdpClient")).toBeNull();
  });
});
