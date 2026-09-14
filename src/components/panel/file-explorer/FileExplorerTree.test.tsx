import { fireEvent, render, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import FileExplorerTree from "./FileExplorerTree";
import type { FileExplorerTreeRow } from "./fileExplorerTreeModel";

const virtualizerMock = vi.hoisted(() => ({
  scrollToIndex: vi.fn(),
  visibleIndexes: null as number[] | null,
}));

vi.mock("@tanstack/react-virtual", () => ({
  useVirtualizer: ({ count }: { count: number }) => ({
    getTotalSize: () => count * 28,
    getVirtualItems: () => {
      const indexes =
        virtualizerMock.visibleIndexes ?? Array.from({ length: count }, (_, index) => index);
      return indexes.map((index) => ({
        index,
        key: index,
        size: 28,
        start: index * 28,
      }));
    },
    measure: vi.fn(),
    scrollToIndex: virtualizerMock.scrollToIndex,
  }),
}));

function file(name: string) {
  return {
    name,
    is_dir: false,
    is_symlink: false,
    size: 0,
    permissions: "",
    owner: "",
    group: "",
    mtime: 0,
  };
}

function row(path: string, name: string, depth: number, isRoot = false): FileExplorerTreeRow {
  return {
    entry: isRoot ? { ...file(name), is_dir: true } : file(name),
    path,
    parentPath: isRoot ? "" : "/",
    depth,
    isRoot,
    isExpanded: isRoot,
    isLoading: false,
    hasLoadedChildren: isRoot,
    directoryStatus: "loaded",
  };
}

describe("FileExplorerTree keyboard focus", () => {
  it("moves DOM focus with consecutive ArrowDown navigation", async () => {
    virtualizerMock.visibleIndexes = null;
    virtualizerMock.scrollToIndex.mockReset();
    const rows = [
      row("/", "/", 0, true),
      row("/alpha.txt", "alpha.txt", 1),
      row("/beta.txt", "beta.txt", 1),
    ];
    const scrollContainerRef = {
      current: document.createElement("div"),
    };

    const view = render(
      <FileExplorerTree
        rows={rows}
        selectedPaths={new Set()}
        backend="remote"
        scrollContainerRef={scrollContainerRef}
        revealRequest={null}
        onRowClick={vi.fn()}
        onRowKeyDown={vi.fn()}
        onSelectAll={vi.fn()}
        onDeleteSelected={vi.fn()}
        onToggleDirectory={vi.fn()}
        onActivateDirectory={vi.fn()}
        onOpenFile={vi.fn()}
        onRequestRename={vi.fn()}
        onRetry={vi.fn()}
        inlineRename={null}
        onInlineRenameChange={vi.fn()}
        onInlineRenameSubmit={vi.fn()}
        onInlineRenameCancel={vi.fn()}
        onContextMenuRow={vi.fn()}
        onContextMenuSelect={vi.fn()}
        labels={{
          collapse: "Collapse",
          expand: "Expand",
          loading: "Loading",
          retry: "Retry",
          emptyDirectory: "Empty",
          tree: "Files",
        }}
      />,
    );

    const items = view.getAllByRole("treeitem");
    items[0].focus();
    fireEvent.keyDown(items[0], { key: "ArrowDown" });

    await waitFor(() => {
      expect(document.activeElement).toBe(items[1]);
      expect(items[1].tabIndex).toBe(0);
    });

    fireEvent.keyDown(items[1], { key: "ArrowDown" });

    await waitFor(() => {
      expect(document.activeElement).toBe(items[2]);
      expect(items[2].tabIndex).toBe(0);
      expect(items[0].tabIndex).toBe(-1);
      expect(items[1].tabIndex).toBe(-1);
    });

    expect(virtualizerMock.scrollToIndex).toHaveBeenLastCalledWith(2, { align: "auto" });
  });

  it("moves DOM focus after a virtualized target row mounts", async () => {
    virtualizerMock.visibleIndexes = [0, 1];
    virtualizerMock.scrollToIndex.mockReset();
    virtualizerMock.scrollToIndex.mockImplementation((index: number) => {
      if (index === 2) virtualizerMock.visibleIndexes = [1, 2];
    });
    const rows = [
      row("/", "/", 0, true),
      row("/alpha.txt", "alpha.txt", 1),
      row("/beta.txt", "beta.txt", 1),
    ];
    const view = render(
      <FileExplorerTree
        rows={rows}
        selectedPaths={new Set()}
        backend="remote"
        scrollContainerRef={{ current: document.createElement("div") }}
        revealRequest={null}
        onRowClick={vi.fn()}
        onRowKeyDown={vi.fn()}
        onSelectAll={vi.fn()}
        onDeleteSelected={vi.fn()}
        onToggleDirectory={vi.fn()}
        onActivateDirectory={vi.fn()}
        onOpenFile={vi.fn()}
        onRequestRename={vi.fn()}
        onRetry={vi.fn()}
        inlineRename={null}
        onInlineRenameChange={vi.fn()}
        onInlineRenameSubmit={vi.fn()}
        onInlineRenameCancel={vi.fn()}
        onContextMenuRow={vi.fn()}
        onContextMenuSelect={vi.fn()}
        labels={{
          collapse: "Collapse",
          expand: "Expand",
          loading: "Loading",
          retry: "Retry",
          emptyDirectory: "Empty",
          tree: "Files",
        }}
      />,
    );

    const alpha = view.getByRole("treeitem", { name: /alpha\.txt/i });
    expect(view.queryByRole("treeitem", { name: /beta\.txt/i })).toBeNull();
    alpha.focus();
    fireEvent.keyDown(alpha, { key: "ArrowDown" });

    expect(virtualizerMock.scrollToIndex).toHaveBeenCalledWith(2, { align: "auto" });
    await waitFor(() => {
      const beta = view.getByRole("treeitem", { name: /beta\.txt/i });
      expect(beta.tabIndex).toBe(0);
      expect(document.activeElement).toBe(beta);
    });
  });
});
