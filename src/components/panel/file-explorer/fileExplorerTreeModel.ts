import type { FileEntry } from "@/types/global";
import type { FileExplorerBackendKind } from "./model";
import {
  isSameExplorerDirectory,
  joinExplorerPath,
  normalizeExplorerPath,
  pathStartsWithDirectory,
} from "./model";

export type FileExplorerTreeEntry = {
  entry: FileEntry;
  path: string;
  parentPath: string;
  rawPathToken?: string;
};

export type FileExplorerTreeDirectoryStatus = "idle" | "loading" | "loaded" | "error";

export type FileExplorerTreeDirectoryRecord = {
  path: string;
  rawPathToken?: string;
  status: FileExplorerTreeDirectoryStatus;
  children: FileEntry[];
  error?: string;
  loadedAt?: number;
};

export type FileExplorerTreeRow = FileExplorerTreeEntry & {
  depth: number;
  isRoot: boolean;
  isExpanded: boolean;
  isLoading: boolean;
  hasLoadedChildren: boolean;
  directoryStatus?: FileExplorerTreeDirectoryStatus;
  directoryError?: string;
};

export type FileExplorerTreeSessionCache = {
  rootPath: string;
  rootRawPathToken?: string;
  directories: Map<string, FileExplorerTreeDirectoryRecord>;
  expandedPaths: Set<string>;
  selectedPaths: Set<string>;
  anchorPath: string | null;
};

export type FileExplorerTreeRoot = {
  entry: FileEntry;
  path: string;
  label: string;
  rawPathToken?: string;
};

export const fileExplorerTreeSessionCacheStore = new Map<
  string,
  FileExplorerTreeSessionCache
>();

export function clearFileExplorerTreeSessionCache(sessionId: string | null | undefined) {
  if (!sessionId) return;
  for (const key of [...fileExplorerTreeSessionCacheStore.keys()]) {
    if (key.startsWith(`${sessionId}:`)) {
      fileExplorerTreeSessionCacheStore.delete(key);
    }
  }
}

function isWindowsStylePath(path: string) {
  return (
    /^[a-zA-Z]:[\\/]/.test(path) ||
    path.startsWith("\\\\") ||
    path.includes("\\")
  );
}

export function treePathKey(path: string, backend: FileExplorerBackendKind) {
  const normalized = normalizeExplorerPath(path, backend);
  if (backend === "local" && isWindowsStylePath(normalized)) {
    return normalized.replace(/\//g, "\\").toLocaleLowerCase();
  }
  return normalized;
}

export function isTreeDirectoryExpandable(entry: FileEntry) {
  return entry.is_dir && !entry.is_symlink;
}

export function createTreeRootEntry(label: string): FileEntry {
  return {
    name: label,
    is_dir: true,
    is_symlink: false,
    size: 0,
    permissions: "",
    owner: "",
    group: "",
    mtime: 0,
  };
}

export function createTreeEntry(
  parentPath: string,
  entry: FileEntry,
  backend: FileExplorerBackendKind,
): FileExplorerTreeEntry {
  return {
    entry,
    path: joinExplorerPath(parentPath, entry.name, backend),
    parentPath,
    rawPathToken: entry.raw_path_token,
  };
}

function localRootPath(path: string) {
  if (path.startsWith("/") && !isWindowsStylePath(path)) {
    return "/";
  }
  const normalized = path.replace(/\//g, "\\");
  if (/^[a-zA-Z]:/.test(normalized)) {
    return `${normalized.slice(0, 2)}\\`;
  }
  if (normalized.startsWith("\\\\")) {
    const parts = normalized.split("\\").filter(Boolean);
    if (parts.length >= 2) return `\\\\${parts[0]}\\${parts[1]}`;
  }
  if (normalized.startsWith("\\")) return "\\";
  return normalized;
}

export function getTreeRootPath(
  currentPath: string,
  homeDir: string,
  backend: FileExplorerBackendKind,
) {
  if (backend === "remote") return "/";

  const normalizedCurrent = normalizeExplorerPath(currentPath || homeDir, backend);
  return normalizedCurrent ? localRootPath(normalizedCurrent) : "";
}

export function getTreeRootLabel(
  rootPath: string,
  homeDir: string,
  backend: FileExplorerBackendKind,
) {
  const normalizedRoot = normalizeExplorerPath(rootPath, backend);
  const normalizedHome = normalizeExplorerPath(homeDir, backend);
  if (
    normalizedRoot &&
    normalizedHome &&
    treePathKey(normalizedRoot, backend) === treePathKey(normalizedHome, backend)
  ) {
    return "~";
  }
  return normalizedRoot || "~";
}

export function buildTreePathChain(
  rootPath: string,
  targetPath: string,
  backend: FileExplorerBackendKind,
) {
  const root = normalizeExplorerPath(rootPath, backend);
  const target = normalizeExplorerPath(targetPath, backend);
  if (!root || !target || isSameExplorerDirectory(root, target, backend)) {
    return root ? [root] : [];
  }
  if (!pathStartsWithDirectory(target, root, backend)) return [root];

  const suffix =
    backend === "remote"
      ? root === "/"
        ? target.slice(1)
        : target.slice(root.length).replace(/^\/+/, "")
      : target.slice(root.length).replace(/^[\\/]+/, "");
  const parts = suffix.split(/[\\/]/).filter(Boolean);
  const chain = [root];
  let accumulated = root;
  for (const part of parts) {
    accumulated = joinExplorerPath(accumulated, part, backend);
    chain.push(accumulated);
  }
  return chain;
}

export function flattenFileExplorerTree({
  root,
  records,
  expandedPaths,
  backend,
  showHiddenFiles,
}: {
  root: FileExplorerTreeRoot;
  records: ReadonlyMap<string, FileExplorerTreeDirectoryRecord>;
  expandedPaths: ReadonlySet<string>;
  backend: FileExplorerBackendKind;
  showHiddenFiles: boolean;
}): FileExplorerTreeRow[] {
  const rows: FileExplorerTreeRow[] = [];

  const visit = (
    entry: FileEntry,
    parentPath: string,
    depth: number,
    isRoot: boolean,
    path: string,
    rawPathToken?: string,
  ) => {
    const key = treePathKey(path, backend);
    const record = records.get(key);
    const isExpanded = isRoot || expandedPaths.has(key);
    const isLoading = record?.status === "loading";
    const hasLoadedChildren = (record?.children.length ?? 0) > 0;

    rows.push({
      entry,
      path,
      parentPath,
      rawPathToken,
      depth,
      isRoot,
      isExpanded,
      isLoading,
      hasLoadedChildren,
      directoryStatus: record?.status,
      directoryError: record?.error,
    });

    if (!isTreeDirectoryExpandable(entry) || !isExpanded || !record) return;
    for (const child of record.children) {
      if (!showHiddenFiles && child.name.startsWith(".")) continue;
      const childLocation = createTreeEntry(path, child, backend);
      visit(
        childLocation.entry,
        childLocation.parentPath,
        depth + 1,
        false,
        childLocation.path,
        childLocation.rawPathToken,
      );
    }
  };

  visit(root.entry, "", 0, true, root.path, root.rawPathToken);

  return rows;
}

export function getTreeRowSelection(
  rows: FileExplorerTreeRow[],
  anchorPath: string,
  targetPath: string,
  backend: FileExplorerBackendKind,
  additive = false,
) {
  const anchorKey = treePathKey(anchorPath, backend);
  const targetKey = treePathKey(targetPath, backend);
  const anchorIndex = rows.findIndex((row) => treePathKey(row.path, backend) === anchorKey);
  const targetIndex = rows.findIndex((row) => treePathKey(row.path, backend) === targetKey);
  const next = additive ? new Set<string>() : new Set<string>();
  if (anchorIndex < 0 || targetIndex < 0) return next;

  const [start, end] = anchorIndex <= targetIndex
    ? [anchorIndex, targetIndex]
    : [targetIndex, anchorIndex];
  for (let index = start; index <= end; index += 1) {
    const row = rows[index];
    if (row && !row.isRoot) next.add(treePathKey(row.path, backend));
  }
  return next;
}
