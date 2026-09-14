import { describe, expect, it } from "vitest";
import {
  buildTreePathChain,
  createTreeRootEntry,
  flattenFileExplorerTree,
  getTreeRootLabel,
  getTreeRootPath,
  isTreeDirectoryExpandable,
  treePathKey,
  type FileExplorerTreeDirectoryRecord,
} from "./fileExplorerTreeModel";

function file(name: string, isDir = false, isSymlink = false) {
  return {
    name,
    is_dir: isDir,
    is_symlink: isSymlink,
    size: 0,
    permissions: "",
    owner: "",
    group: "",
    mtime: 0,
  };
}

function records(
  entries: Array<[string, FileExplorerTreeDirectoryRecord]>,
) {
  return new Map(entries);
}

describe("file explorer tree path helpers", () => {
  it("uses the remote filesystem root for every selected path", () => {
    expect(getTreeRootPath("/home/nya/project", "/home/nya", "remote")).toBe(
      "/",
    );
    expect(getTreeRootLabel("/", "/home/nya", "remote")).toBe("/");
  });

  it("keeps the remote root when the selected path is outside home", () => {
    expect(getTreeRootPath("/etc", "/home/nya", "remote")).toBe("/");
    expect(buildTreePathChain("/", "/etc/ssh", "remote")).toEqual([
      "/",
      "/etc",
      "/etc/ssh",
    ]);
  });

  it("uses the POSIX filesystem root for local absolute paths", () => {
    expect(getTreeRootPath("/Users/nya/project", "/Users/nya", "local")).toBe(
      "/",
    );
    expect(getTreeRootPath("/home/nya/project", "/home/nya", "local")).toBe(
      "/",
    );
    expect(buildTreePathChain("/", "/home/nya/project", "local")).toEqual([
      "/",
      "/home",
      "/home/nya",
      "/home/nya/project",
    ]);
  });

  it("uses the local filesystem drive root and case-insensitive keys", () => {
    expect(getTreeRootPath("C:\\Users\\nya\\project", "C:\\Users\\nya", "local")).toBe(
      "C:\\",
    );
    expect(getTreeRootPath("D:\\Work\\project", "C:\\Users\\nya", "local")).toBe(
      "D:\\",
    );
    expect(buildTreePathChain("C:\\Users\\nya", "C:\\Users\\nya\\project", "local")).toEqual([
      "C:\\Users\\nya",
      "C:\\Users\\nya\\project",
    ]);
    expect(treePathKey("C:\\Work\\", "local")).toBe(treePathKey("c:/work", "local"));
  });
});

describe("file explorer tree rows", () => {
  it("flattens only expanded directories and preserves depth", () => {
    const rootPath = "/home/nya";
    const projectPath = "/home/nya/project";
    const root = {
      entry: createTreeRootEntry("~"),
      path: rootPath,
      label: "~",
    };
    const rootChildren = [file("project", true), file("notes.txt")];
    const projectChildren = [file("src", true), file("README.md")];
    const treeRows = flattenFileExplorerTree({
      root,
      records: records([
        [treePathKey(rootPath, "remote"), {
          path: rootPath,
          status: "loaded",
          children: rootChildren,
        }],
        [treePathKey(projectPath, "remote"), {
          path: projectPath,
          status: "loaded",
          children: projectChildren,
        }],
      ]),
      expandedPaths: new Set([treePathKey(rootPath, "remote"), treePathKey(projectPath, "remote")]),
      backend: "remote",
      showHiddenFiles: true,
    });

    expect(treeRows.map((row) => [row.entry.name, row.depth])).toEqual([
      ["~", 0],
      ["project", 1],
      ["src", 2],
      ["README.md", 2],
      ["notes.txt", 1],
    ]);
  });

  it("filters hidden files and does not recurse through symlink directories", () => {
    const rootPath = "/home/nya";
    const root = {
      entry: createTreeRootEntry("~"),
      path: rootPath,
      label: "~",
    };
    const link = file("current", true, true);
    const hidden = file(".secret");
    const rows = flattenFileExplorerTree({
      root,
      records: records([
        [treePathKey(rootPath, "remote"), {
          path: rootPath,
          status: "loaded",
          children: [link, hidden],
        }],
      ]),
      expandedPaths: new Set([treePathKey(rootPath, "remote")]),
      backend: "remote",
      showHiddenFiles: false,
    });

    expect(rows.map((row) => row.entry.name)).toEqual(["~", "current"]);
    expect(isTreeDirectoryExpandable(link)).toBe(false);
  });

});
