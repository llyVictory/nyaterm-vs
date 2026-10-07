import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import RemoteFileEditorPage from "./RemoteFileEditorPage";

const state = vi.hoisted(() => ({
  translate: (key: string) => key,
  web: true,
  invoke: vi.fn(),
  download: vi.fn(),
  close: vi.fn(async () => {}),
  info: vi.fn(),
  error: vi.fn(),
  log: vi.fn(),
  openPath: vi.fn(),
  tempDir: vi.fn(async () => "/temp"),
}));
vi.mock("@/lib/invoke", () => ({ invoke: state.invoke }));
vi.mock("@/lib/backend/files", () => ({ downloadBrowserFile: state.download }));
vi.mock("@/lib/backend/runtime", async (original) => ({
  ...(await original<typeof import("@/lib/backend/runtime")>()),
  supports: () => !state.web,
}));
vi.mock("@/lib/backend/platform/path", () => ({
  tempDir: state.tempDir,
  join: async (...parts: string[]) => parts.join("/"),
}));
vi.mock("@/lib/backend/platform/opener", () => ({ openPath: state.openPath }));
vi.mock("@/lib/backend/platform/window", () => ({
  getCurrentWindow: () => ({
    label: "editor",
    close: state.close,
    setTitle: async () => {},
    onCloseRequested: async () => () => {},
  }),
}));
vi.mock("@/context/AppContext", () => ({
  useApp: () => ({
    appSettings: {
      transfer: { default_editor: "desktop", internal_editor_font_size: 14 },
    },
    updateAppSettings: vi.fn(),
  }),
}));
vi.mock("@/hooks/useChildWindowCommand", () => ({
  useChildWindowCommand: () => {},
}));
vi.mock("@/hooks/useFileEditorZoom", () => ({ useFileEditorZoom: () => {} }));
vi.mock("@/components/layout/ChildWindowHeader", () => ({
  default: () => null,
}));
vi.mock("@/lib/logger", () => ({ logger: { error: state.log } }));
vi.mock("sonner", () => ({ toast: { info: state.info, error: state.error } }));
vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: state.translate }),
}));
vi.mock("@/lib/codeMirrorFileView", () => ({
  codeMirrorFileViewExtensions: () => [],
  getCursorPosition: () => ({ line: 1, column: 1 }),
  getDisplayLanguage: (language: string) => language,
}));
vi.mock("@codemirror/view", () => ({
  EditorView: class {
    static updateListener = { of: (listener: unknown) => listener };
    state: unknown;
    constructor({ state }: { state: unknown }) {
      this.state = state;
    }
    setState(state: unknown) {
      this.state = state;
    }
    focus() {}
    destroy() {}
  },
}));
beforeEach(() => {
  vi.clearAllMocks();
  state.web = true;
  state.download.mockResolvedValue(undefined);
  window.history.replaceState(
    {},
    "",
    `/?window=file-editor&data=${encodeURIComponent(JSON.stringify({ sessionId: "owned", remotePath: "/file.bin", name: "file.bin", size: 4, mtime: 0 }))}`,
  );
});
describe("remote editor capability fallback", () => {
  it.each(["binary", "encoding"])(
    "downloads unsupported %s files in Web child windows",
    async (reason) => {
      state.invoke.mockResolvedValue({ status: "unsupported", reason });
      render(<RemoteFileEditorPage />);
      await waitFor(() =>
        expect(state.download).toHaveBeenCalledWith("owned", "/file.bin"),
      );
      expect(state.info).toHaveBeenCalledWith(
        "fileExplorer.webUnsupportedDownload",
      );
      expect(screen.queryByText("fileEditor.openExternal")).toBeNull();
      expect(state.tempDir).not.toHaveBeenCalled();
      expect(state.openPath).not.toHaveBeenCalled();
      await waitFor(() => expect(state.close).toHaveBeenCalled());
    },
  );
  it("shows download failure while retaining the editor window and logs", async () => {
    state.invoke.mockResolvedValue({ status: "unsupported", reason: "binary" });
    state.download.mockRejectedValue(new Error("Permission denied"));
    render(<RemoteFileEditorPage />);
    await screen.findByText("Permission denied");
    expect(state.error).toHaveBeenCalledWith("Permission denied");
    expect(state.log).toHaveBeenCalled();
    expect(state.close).not.toHaveBeenCalled();
  });
  it("shows and logs file open failures", async () => {
    state.invoke.mockRejectedValue(new Error("SSH closed"));
    render(<RemoteFileEditorPage />);
    await screen.findByText("SSH closed");
    expect(state.error).toHaveBeenCalledWith("SSH closed");
    expect(state.log).toHaveBeenCalled();
  });
  it("retains desktop external editing and watcher fallback", async () => {
    state.web = false;
    state.invoke.mockImplementation(async (command: string) =>
      command === "open_remote_file_text"
        ? { status: "unsupported", reason: "binary" }
        : command === "sanitize_download_file_name"
          ? "file.bin"
          : undefined,
    );
    render(<RemoteFileEditorPage />);
    await waitFor(() => expect(state.openPath).toHaveBeenCalled());
    expect(state.invoke).toHaveBeenCalledWith(
      "start_file_watch",
      expect.objectContaining({ sessionId: "owned", remotePath: "/file.bin" }),
    );
    expect(state.download).not.toHaveBeenCalled();
  });
});
