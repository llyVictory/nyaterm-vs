import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import type { FileEntry } from "@/types/global";
import FileExplorerEntryContextMenu, {
  FileExplorerContextMenuActionBar,
} from "./FileExplorerEntryContextMenu";
import type { FileExplorerTreeRow } from "./fileExplorerTreeModel";

const mode = vi.hoisted(() => ({ web: false }));
vi.mock("@/lib/backend/runtime", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/backend/runtime")>()),
  supports: () => !mode.web,
}));
beforeEach(() => {
  mode.web = false;
});

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) =>
      ({
        "fileExplorer.cmCut": "Cut",
        "fileExplorer.cmCopy": "Copy",
        "fileExplorer.cmPaste": "Paste",
        "fileExplorer.cmRename": "Rename",
        "fileExplorer.cmDelete": "Delete",
        "fileExplorer.cmCopyInfo": "Copy Info",
        "fileExplorer.cmTerminal": "Terminal",
        "fileExplorer.cmEnterDirectory": "Enter Directory",
        "fileExplorer.cmOpenDirectoryNewTerminal": "Open New Terminal",
      })[key] ?? key,
  }),
}));

describe("FileExplorerContextMenuActionBar", () => {
  it("renders five compact actions and keeps unavailable actions disabled", async () => {
    renderActionBar({ canCopy: true, canPaste: false });

    const items = await screen.findAllByRole("menuitem");
    expect(items).toHaveLength(5);
    expect(menuItem("Cut").hasAttribute("data-disabled")).toBe(true);
    expect(menuItem("Copy").hasAttribute("data-disabled")).toBe(false);
    expect(menuItem("Paste").hasAttribute("data-disabled")).toBe(true);
    expect(menuItem("Rename").hasAttribute("data-disabled")).toBe(true);
    expect(menuItem("Delete").hasAttribute("data-disabled")).toBe(true);
  });

  it("runs an enabled paste action", async () => {
    const onPaste = vi.fn();
    renderActionBar({ onPaste, canPaste: true });

    fireEvent.click(await screen.findByText("Paste"));

    expect(onPaste).toHaveBeenCalledOnce();
  });
});

function entryRow(isDirectory: boolean): FileExplorerTreeRow {
  const entry: FileEntry = {
    name: isDirectory ? "folder" : "file.txt",
    is_dir: isDirectory,
    is_symlink: false,
    size: 0,
    permissions: "",
    owner: "",
    group: "",
    mtime: 0,
  };
  return {
    entry,
    path: `/home/${entry.name}`,
    parentPath: "/home",
    depth: 1,
    isRoot: false,
    isExpanded: false,
    isLoading: false,
    hasLoadedChildren: false,
  };
}

function renderEntryMenu(row: FileExplorerTreeRow) {
  const onCopyPath = vi.fn();
  const onSendToTerminal = vi.fn();
  const onEnterDirectoryInTerminal = vi.fn();
  const onOpenDirectoryInNewTerminal = vi.fn();
  render(
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <button type="button">Open entry menu</button>
      </ContextMenuTrigger>
      <FileExplorerEntryContextMenu
        target={row}
        selectedTargets={[row]}
        activeSessionId="session-1"
        editorType="internal"
        showTransferActions
        terminalInputEnabled
        sendTargetOptions={[]}
        getAiActions={() => []}
        onOpenDirectory={vi.fn()}
        onOpenDefault={vi.fn()}
        onPreview={vi.fn()}
        onOpenInternal={vi.fn()}
        onOpenExternal={vi.fn()}
        onRefresh={vi.fn()}
        onUpload={vi.fn()}
        onUploadFolder={vi.fn()}
        onUploadFolderContents={vi.fn()}
        onDownload={vi.fn()}
        onRename={vi.fn()}
        onMove={vi.fn()}
        onDelete={vi.fn()}
        onAddToFavorites={vi.fn()}
        onCopyPath={onCopyPath}
        onSendToTerminal={onSendToTerminal}
        onEnterDirectoryInTerminal={onEnterDirectoryInTerminal}
        onOpenDirectoryInNewTerminal={onOpenDirectoryInNewTerminal}
        onProperties={vi.fn()}
        onAIAction={vi.fn()}
      />
    </ContextMenu>,
  );
  fireEvent.contextMenu(
    screen.getByRole("button", { name: "Open entry menu" }),
  );
  return {
    onCopyPath,
    onSendToTerminal,
    onEnterDirectoryInTerminal,
    onOpenDirectoryInNewTerminal,
  };
}

describe("FileExplorerEntryContextMenu", () => {
  it("hides Web directory transfer and external editor entries", async () => {
    mode.web = true;
    renderEntryMenu(entryRow(true));
    expect(await screen.findByText("fileExplorer.cmOpen")).toBeTruthy();
    expect(screen.queryByText("fileExplorer.cmDownload")).toBeNull();
    fireEvent.click(screen.getByText("fileExplorer.cmUpload"));
    expect(await screen.findByText("fileExplorer.upload")).toBeTruthy();
    expect(screen.queryByText("fileExplorer.uploadFolder")).toBeNull();
    expect(screen.queryByText("fileExplorer.uploadFolderContents")).toBeNull();
  });
  it("keeps Web file downloads while hiding the external editor", async () => {
    mode.web = true;
    renderEntryMenu(entryRow(false));
    expect(await screen.findByText("fileExplorer.cmDownload")).toBeTruthy();
    expect(screen.queryByText("fileExplorer.cmOpenExternalEditor")).toBeNull();
  });
  it("groups copy information and terminal actions for directories", async () => {
    const row = entryRow(true);
    const actions = renderEntryMenu(row);
    fireEvent.click(await screen.findByText("Copy Info"));
    fireEvent.click(await screen.findByText("fileExplorer.cmCopyPath"));
    expect(actions.onCopyPath).toHaveBeenCalledWith(row, "full");

    fireEvent.contextMenu(
      screen.getByRole("button", { name: "Open entry menu" }),
    );
    fireEvent.click(await screen.findByText("Terminal"));
    fireEvent.click(await screen.findByText("Enter Directory"));
    expect(actions.onEnterDirectoryInTerminal).toHaveBeenCalledWith(row);

    fireEvent.contextMenu(
      screen.getByRole("button", { name: "Open entry menu" }),
    );
    fireEvent.click(await screen.findByText("Terminal"));
    fireEvent.click(await screen.findByText("Open New Terminal"));
    expect(actions.onOpenDirectoryInNewTerminal).toHaveBeenCalledWith(row);
  });

  it("keeps directory actions off ordinary files", async () => {
    renderEntryMenu(entryRow(false));
    fireEvent.click(await screen.findByText("Terminal"));
    expect(await screen.findByText("fileExplorer.cmTerminalPath")).toBeTruthy();
    expect(screen.queryByText("Enter Directory")).toBeNull();
    expect(screen.queryByText("Open New Terminal")).toBeNull();
  });
});

function menuItem(label: string): HTMLElement {
  const item = screen.getByText(label).closest('[role="menuitem"]');
  if (!(item instanceof HTMLElement)) {
    throw new Error(`Missing menu item: ${label}`);
  }
  return item;
}

function renderActionBar(
  props: React.ComponentProps<typeof FileExplorerContextMenuActionBar>,
) {
  render(
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <button type="button">Open menu</button>
      </ContextMenuTrigger>
      <ContextMenuContent>
        <FileExplorerContextMenuActionBar {...props} />
      </ContextMenuContent>
    </ContextMenu>,
  );

  fireEvent.contextMenu(screen.getByRole("button", { name: "Open menu" }));
}
