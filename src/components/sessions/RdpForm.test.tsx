import { fireEvent, render, screen } from "@testing-library/react";
import type { ComponentProps } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { RdpForm } from "./RdpForm";

const platform = vi.hoisted(() => ({ windowsDesktop: true }));
vi.mock("@/lib/backend/runtime", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/backend/runtime")>()),
  canUseWindowsRdpClient: () => platform.windowsDesktop,
}));
vi.mock("@/lib/invoke", () => ({ invoke: vi.fn().mockResolvedValue([]) }));
vi.mock("@/components/sessions/SessionNetworkSection", () => ({
  SessionNetworkSection: () => null,
}));
vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

function renderForm(overrides: Partial<ComponentProps<typeof RdpForm>> = {}) {
  const props: ComponentProps<typeof RdpForm> = {
    clientMode: "default",
    defaultClientMode: "windows",
    setClientMode: vi.fn(),
    host: "rdp.example.com",
    setHost: vi.fn(),
    port: 3389,
    setPort: vi.fn(),
    username: "Administrator",
    setUsername: vi.fn(),
    domain: "",
    setDomain: vi.fn(),
    passwordId: "",
    setPasswordId: vi.fn(),
    password: "",
    setPassword: vi.fn(),
    hasPassword: false,
    setHasPassword: vi.fn(),
    useNla: true,
    setUseNla: vi.fn(),
    certificatePolicy: "prompt",
    setCertificatePolicy: vi.fn(),
    displayWidth: 1920,
    setDisplayWidth: vi.fn(),
    displayHeight: 1080,
    setDisplayHeight: vi.fn(),
    displayMode: "fit-window",
    setDisplayMode: vi.fn(),
    clipboardMode: "text-only",
    setClipboardMode: vi.fn(),
    reconnectEnabled: true,
    setReconnectEnabled: vi.fn(),
    reconnectMaxAttempts: 5,
    setReconnectMaxAttempts: vi.fn(),
    proxyId: "",
    setProxyId: vi.fn(),
    proxies: [],
    jumpHostId: "",
    setJumpHostId: vi.fn(),
    jumpHostOptions: [],
    ...overrides,
  };
  render(<RdpForm {...props} />);
  return props;
}

describe("RDP open method", () => {
  beforeEach(() => {
    platform.windowsDesktop = true;
  });

  it("shows the effective default and system-client parameter explanation", () => {
    renderForm();
    expect(
      screen.getByRole("combobox", { name: "dialog.rdpOpenWith" }).textContent,
    ).toContain("settings.rdpClientWindows");
    expect(screen.queryByText("settings.rdpSystemClientDesc")).not.toBeNull();
  });

  it("allows selecting built-in RDP", () => {
    const props = renderForm();
    fireEvent.keyDown(
      screen.getByRole("combobox", { name: "dialog.rdpOpenWith" }),
      { key: "ArrowDown" },
    );
    fireEvent.click(
      screen.getByRole("option", { name: "settings.rdpClientBuiltin" }),
    );
    expect(props.setClientMode).toHaveBeenCalledWith("builtin");
  });

  it("hides the system-client explanation for an explicit built-in choice", () => {
    renderForm({ clientMode: "builtin" });
    expect(screen.queryByText("settings.rdpSystemClientDesc")).toBeNull();
  });

  it("hides the selector without rewriting a stored Windows preference on other platforms", () => {
    platform.windowsDesktop = false;
    const props = renderForm({ clientMode: "windows" });
    expect(
      screen.queryByRole("combobox", { name: "dialog.rdpOpenWith" }),
    ).toBeNull();
    expect(props.setClientMode).not.toHaveBeenCalled();
  });
});
