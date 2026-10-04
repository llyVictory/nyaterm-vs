import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { FileExplorerToolbar } from "./FileExplorerToolbar";

const mode = vi.hoisted(() => ({ web: true }));
vi.mock("@/lib/backend/runtime", async (original) => ({
  ...(await original<typeof import("@/lib/backend/runtime")>()),
  supports: () => !mode.web,
}));
vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));
function toolbar(directory: boolean) {
  const noop = vi.fn();
  return (
    <FileExplorerToolbar
      isTreeView={false}
      selectedCount={2}
      selectionHasDirectory={directory}
      isFileSearchActive={false}
      isFileSearchExpanded={false}
      showHiddenFiles={false}
      showTransferActions
      fileSearchQuery=""
      fileSearchInputRef={{ current: null }}
      onNewFile={noop}
      onNewFolder={noop}
      onUploadFiles={noop}
      onUploadFolder={noop}
      onUploadFolderContents={noop}
      onDownloadSelected={noop}
      onDeleteSelected={noop}
      onGoUp={noop}
      onRefresh={noop}
      onLocatePath={noop}
      locateLabel="cwd"
      canLocatePath
      onToggleViewMode={noop}
      onToggleHiddenFiles={noop}
      onExpandSearch={noop}
      onSearchQueryChange={noop}
      onCollapseSearch={noop}
    />
  );
}
describe("file transfer toolbar", () => {
  it("disables mixed directory download in Web, preserving delete and file downloads", () => {
    mode.web = true;
    const { rerender } = render(toolbar(true));
    expect(
      (
        screen.getByLabelText(
          "fileExplorer.downloadSelected",
        ) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    expect(
      (screen.getByLabelText("fileExplorer.delete") as HTMLButtonElement)
        .disabled,
    ).toBe(false);
    rerender(toolbar(false));
    expect(
      (
        screen.getByLabelText(
          "fileExplorer.downloadSelected",
        ) as HTMLButtonElement
      ).disabled,
    ).toBe(false);
    fireEvent.pointerDown(screen.getByLabelText("fileExplorer.upload"), {
      button: 0,
      ctrlKey: false,
      pointerType: "mouse",
    });
    expect(screen.queryByText("fileExplorer.uploadFolder")).toBeNull();
    expect(screen.queryByText("fileExplorer.uploadFolderContents")).toBeNull();
  });
  it("keeps recursive transfers available on desktop", () => {
    mode.web = false;
    render(toolbar(true));
    expect(
      (
        screen.getByLabelText(
          "fileExplorer.downloadSelected",
        ) as HTMLButtonElement
      ).disabled,
    ).toBe(false);
    fireEvent.pointerDown(screen.getByLabelText("fileExplorer.upload"), {
      button: 0,
      ctrlKey: false,
      pointerType: "mouse",
    });
    expect(screen.getByText("fileExplorer.uploadFolder")).toBeTruthy();
  });
});
