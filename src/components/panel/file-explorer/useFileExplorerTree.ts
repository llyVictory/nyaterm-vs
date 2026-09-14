import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent as ReactMouseEvent,
} from "react";
import type { FileEntry } from "@/types/global";
import type { FileExplorerBackendKind } from "./model";
import {
  buildTreePathChain,
  createTreeRootEntry,
  fileExplorerTreeSessionCacheStore,
  flattenFileExplorerTree,
  getTreeRootLabel,
  isTreeDirectoryExpandable,
  treePathKey,
  type FileExplorerTreeDirectoryRecord,
  type FileExplorerTreeEntry,
  type FileExplorerTreeRoot,
  type FileExplorerTreeRow,
} from "./fileExplorerTreeModel";
import {
  compareFileEntries,
  getExplorerParentDirectory,
  isSameExplorerDirectory,
  joinExplorerPath,
  normalizeExplorerPath,
} from "./model";

const TREE_SESSION_CACHE_LIMIT = 240;
const TREE_SORT_MODE = { column: "name" as const, direction: "asc" as const };

function sortTreeEntries(entries: FileEntry[]) {
  return [...entries].sort((left, right) =>
    compareFileEntries(left, right, TREE_SORT_MODE),
  );
}

type LoadTreeDirectory = (
  path: string,
  rawPathToken?: string,
) => Promise<FileEntry[]>;

export interface UseFileExplorerTreeOptions {
  sessionId: string;
  enabled: boolean;
  backend: FileExplorerBackendKind;
  rootPath: string;
  rootRawPathToken?: string;
  homeDir: string;
  currentPath: string;
  showHiddenFiles: boolean;
  initialRootEntries: FileEntry[] | null;
  loadDirectory: LoadTreeDirectory;
  onDirectorySelected: (
    target: FileExplorerTreeEntry,
    entries: FileEntry[],
  ) => void;
}

export interface FileExplorerTreeDirectorySnapshot {
  path: string;
  rawPathToken?: string;
  entries: FileEntry[];
}

export interface UseFileExplorerTreeResult {
  root: FileExplorerTreeRoot;
  rows: FileExplorerTreeRow[];
  selectedRows: FileExplorerTreeRow[];
  selectedPaths: Set<string>;
  getDirectorySnapshot: (
    path: string,
  ) => FileExplorerTreeDirectorySnapshot | null;
  selectContextRow: (row: FileExplorerTreeRow | null) => void;
  handleRowClick: (
    row: FileExplorerTreeRow,
    event: ReactMouseEvent<HTMLDivElement>,
  ) => void;
  handleRowKeyDown: (
    event: ReactKeyboardEvent<HTMLDivElement>,
    row: FileExplorerTreeRow,
  ) => void;
  selectAll: () => void;
  clearSelection: () => void;
  toggleDirectory: (row: FileExplorerTreeRow) => void;
  activateDirectory: (row: FileExplorerTreeRow) => void;
  revealPath: (
    path: string,
    options?: { targetType?: "directory" | "file" },
  ) => Promise<void>;
  refreshDirectory: (path: string) => Promise<FileEntry[]>;
  refreshAll: () => void;
  invalidateDirectories: (paths: string[]) => void;
}

function treeSessionCacheKey(
  sessionId: string,
  backend: FileExplorerBackendKind,
  rootPath: string,
) {
  return `${sessionId}:${backend}:${treePathKey(rootPath, backend)}`;
}

function cloneDirectoryRecord(
  record: FileExplorerTreeDirectoryRecord,
): FileExplorerTreeDirectoryRecord {
  return {
    ...record,
    children: [...record.children],
  };
}

function getCachedState(
  cacheKey: string,
  rootPath: string,
  backend: FileExplorerBackendKind,
  rootRawPathToken?: string,
) {
  const cached = fileExplorerTreeSessionCacheStore.get(cacheKey);
  if (!cached || treePathKey(cached.rootPath, backend) !== treePathKey(rootPath, backend)) {
    const directories = new Map<string, FileExplorerTreeDirectoryRecord>();
    directories.set(treePathKey(rootPath, backend), {
      path: rootPath,
      rawPathToken: rootRawPathToken,
      status: "idle",
      children: [],
    });
    return {
      directories,
      expandedPaths: new Set<string>([treePathKey(rootPath, backend)]),
      selectedPaths: new Set<string>(),
      anchorPath: null,
    };
  }

  const directories = new Map<string, FileExplorerTreeDirectoryRecord>();
  for (const [key, record] of cached.directories) {
    directories.set(key, cloneDirectoryRecord(record));
  }
  const rootKey = treePathKey(rootPath, backend);
  if (!directories.has(rootKey)) {
    directories.set(rootKey, {
      path: rootPath,
      rawPathToken: rootRawPathToken,
      status: "idle",
      children: [],
    });
  }
  return {
    directories,
    expandedPaths: new Set(cached.expandedPaths),
    selectedPaths: new Set(cached.selectedPaths),
    anchorPath: cached.anchorPath,
  };
}

function findChildEntry(
  parentPath: string,
  childPath: string,
  entries: FileEntry[],
  backend: FileExplorerBackendKind,
) {
  return entries.find((entry) =>
    isSameExplorerDirectory(
      joinExplorerPath(parentPath, entry.name, backend),
      childPath,
      backend,
    ),
  );
}

export function useFileExplorerTree({
  sessionId,
  enabled,
  backend,
  rootPath,
  rootRawPathToken,
  homeDir,
  currentPath,
  showHiddenFiles,
  initialRootEntries,
  loadDirectory,
  onDirectorySelected,
}: UseFileExplorerTreeOptions): UseFileExplorerTreeResult {
  const normalizedRootPath = normalizeExplorerPath(rootPath, backend);
  const cacheKey = treeSessionCacheKey(sessionId, backend, normalizedRootPath);
  const initialStateRef = useRef<ReturnType<typeof getCachedState> | null>(null);
  const initialCacheKeyRef = useRef<string | null>(null);
  const cacheKeyRef = useRef(cacheKey);
  cacheKeyRef.current = cacheKey;
  if (
    !initialStateRef.current ||
    initialStateRef.current.directories.size === 0 ||
    initialCacheKeyRef.current !== cacheKey
  ) {
    initialStateRef.current = getCachedState(
      cacheKey,
      normalizedRootPath,
      backend,
      rootRawPathToken,
    );
    initialCacheKeyRef.current = cacheKey;
  }
  const initialState = initialStateRef.current;
  if (!initialState) {
    throw new Error("Failed to initialize file explorer tree state");
  }

  const [directories, setDirectories] = useState(initialState.directories);
  const [stateCacheKey, setStateCacheKey] = useState(cacheKey);
  const [expandedPaths, setExpandedPaths] = useState(initialState.expandedPaths);
  const [selectedPaths, setSelectedPaths] = useState(initialState.selectedPaths);
  const [anchorPath, setAnchorPath] = useState<string | null>(initialState.anchorPath);
  const directoriesRef = useRef(directories);
  const expandedPathsRef = useRef(expandedPaths);
  const selectedPathsRef = useRef(selectedPaths);
  const anchorPathRef = useRef(anchorPath);
  const pendingRequestsRef = useRef(new Map<string, Promise<FileEntry[]>>());
  const requestGenerationRef = useRef(new Map<string, number>());
  const activationGenerationRef = useRef(0);
  const initializedCacheKeyRef = useRef<string | null>(null);
  const seededRootCacheKeyRef = useRef<string | null>(null);

  directoriesRef.current = directories;
  expandedPathsRef.current = expandedPaths;
  selectedPathsRef.current = selectedPaths;
  anchorPathRef.current = anchorPath;

  useEffect(() => {
    if (!enabled) {
      activationGenerationRef.current += 1;
      return;
    }
    if (stateCacheKey === cacheKey) return;
    const next = getCachedState(
      cacheKey,
      normalizedRootPath,
      backend,
      rootRawPathToken,
    );
    setDirectories(next.directories);
    setStateCacheKey(cacheKey);
    setExpandedPaths(next.expandedPaths);
    setSelectedPaths(next.selectedPaths);
    setAnchorPath(next.anchorPath);
    pendingRequestsRef.current.clear();
    requestGenerationRef.current.clear();
    activationGenerationRef.current += 1;
    initializedCacheKeyRef.current = null;
    seededRootCacheKeyRef.current = null;
  }, [
    backend,
    cacheKey,
    normalizedRootPath,
    rootRawPathToken,
    stateCacheKey,
    enabled,
  ]);

  const root = useMemo<FileExplorerTreeRoot>(
    () => ({
      entry: createTreeRootEntry(
        getTreeRootLabel(normalizedRootPath, homeDir, backend),
      ),
      path: normalizedRootPath,
      label: getTreeRootLabel(normalizedRootPath, homeDir, backend),
      rawPathToken: rootRawPathToken,
    }),
    [backend, homeDir, normalizedRootPath, rootRawPathToken],
  );

  const loadTreeDirectory = useCallback(
    async (path: string, rawPathToken?: string, force = false) => {
      if (!enabled) return [];
      const normalizedPath = normalizeExplorerPath(path, backend);
      if (!normalizedPath) return [];
      const key = treePathKey(normalizedPath, backend);
      const current = directoriesRef.current.get(key);
      const effectiveRawPathToken = rawPathToken ?? current?.rawPathToken;
      if (!force && current?.status === "loaded") {
        return current.children;
      }

      const pendingKey = `${cacheKey}:${key}`;
      if (!force) {
        const pending = pendingRequestsRef.current.get(pendingKey);
        if (pending) return pending;
      }

      const generation = (requestGenerationRef.current.get(key) ?? 0) + 1;
      requestGenerationRef.current.set(key, generation);
      setDirectories((previous) => {
        const next = new Map(previous);
        const previousRecord = next.get(key);
        next.set(key, {
          path: normalizedPath,
          rawPathToken: effectiveRawPathToken,
          status: "loading",
          children: previousRecord?.children ?? [],
          loadedAt: previousRecord?.loadedAt,
        });
        return next;
      });

      let pending: Promise<FileEntry[]>;
      pending = loadDirectory(normalizedPath, effectiveRawPathToken)
        .then((entries) => {
          const sortedEntries = sortTreeEntries(entries);
          if (requestGenerationRef.current.get(key) === generation) {
            setDirectories((previous) => {
              const next = new Map(previous);
              next.set(key, {
                path: normalizedPath,
                rawPathToken: effectiveRawPathToken,
                status: "loaded",
                children: sortedEntries,
                loadedAt: Date.now(),
              });
              return next;
            });
          }
          return sortedEntries;
        })
        .catch((error) => {
          if (requestGenerationRef.current.get(key) === generation) {
            setDirectories((previous) => {
              const next = new Map(previous);
              const previousRecord = next.get(key);
              next.set(key, {
                path: normalizedPath,
                rawPathToken: effectiveRawPathToken,
                status: "error",
                children: previousRecord?.children ?? [],
                loadedAt: previousRecord?.loadedAt,
                error: error instanceof Error ? error.message : String(error),
              });
              return next;
            });
          }
          throw error;
        })
        .finally(() => {
          if (pendingRequestsRef.current.get(pendingKey) === pending) {
            pendingRequestsRef.current.delete(pendingKey);
          }
        });
      pendingRequestsRef.current.set(pendingKey, pending);
      return pending;
    },
    [backend, cacheKey, enabled, loadDirectory],
  );

  useEffect(() => {
    if (!enabled || stateCacheKey !== cacheKey) return;
    if (initializedCacheKeyRef.current === cacheKey) return;
    initializedCacheKeyRef.current = cacheKey;
    const rootKey = treePathKey(normalizedRootPath, backend);
    const cachedRoot = directoriesRef.current.get(rootKey);
    if (cachedRoot?.status === "loaded") return;

    if (initialRootEntries !== null && seededRootCacheKeyRef.current !== cacheKey) {
      seededRootCacheKeyRef.current = cacheKey;
      setDirectories((previous) => {
        const next = new Map(previous);
        next.set(rootKey, {
          path: normalizedRootPath,
          rawPathToken: rootRawPathToken,
          status: "loaded",
          children: sortTreeEntries(initialRootEntries),
          loadedAt: Date.now(),
        });
        return next;
      });
      return;
    }

    void loadTreeDirectory(normalizedRootPath, rootRawPathToken).catch(() => {});
  }, [
    backend,
    cacheKey,
    stateCacheKey,
    enabled,
    initialRootEntries,
    loadTreeDirectory,
    normalizedRootPath,
    rootRawPathToken,
  ]);

  useEffect(() => {
    if (!sessionId || !normalizedRootPath || stateCacheKey !== cacheKey) return;
    fileExplorerTreeSessionCacheStore.set(cacheKey, {
      rootPath: normalizedRootPath,
      rootRawPathToken,
      directories: new Map<string, FileExplorerTreeDirectoryRecord>(
        [...directories].map(
          ([key, record]) => [key, cloneDirectoryRecord(record)] as const,
        ),
      ),
      expandedPaths: new Set(expandedPaths),
      selectedPaths: new Set(selectedPaths),
      anchorPath,
    });
    while (fileExplorerTreeSessionCacheStore.size > TREE_SESSION_CACHE_LIMIT) {
      const oldest = fileExplorerTreeSessionCacheStore.keys().next().value;
      if (!oldest) break;
      fileExplorerTreeSessionCacheStore.delete(oldest);
    }
  }, [
    anchorPath,
    cacheKey,
    directories,
    expandedPaths,
    normalizedRootPath,
    rootRawPathToken,
    selectedPaths,
    sessionId,
    stateCacheKey,
  ]);

  const rows = useMemo(
    () =>
      flattenFileExplorerTree({
        root,
        records: directories,
        expandedPaths,
        backend,
        showHiddenFiles,
      }),
    [
      backend,
      directories,
      expandedPaths,
      root,
      showHiddenFiles,
    ],
  );

  const selectedRows = useMemo(
    () =>
      rows.filter((row) => selectedPaths.has(treePathKey(row.path, backend))),
    [backend, rows, selectedPaths],
  );

  const getDirectorySnapshot = useCallback(
    (path: string): FileExplorerTreeDirectorySnapshot | null => {
      const normalizedPath = normalizeExplorerPath(path, backend);
      if (!normalizedPath) return null;
      const record = directoriesRef.current.get(
        treePathKey(normalizedPath, backend),
      );
      if (!record || record.status !== "loaded") return null;
      return {
        path: record.path,
        rawPathToken: record.rawPathToken,
        entries: [...record.children],
      };
    },
    [backend],
  );

  const selectContextRow = useCallback(
    (row: FileExplorerTreeRow | null) => {
      if (!row || row.isRoot) {
        setSelectedPaths(new Set());
        setAnchorPath(row?.path ?? null);
        return;
      }
      const key = treePathKey(row.path, backend);
      if (!selectedPathsRef.current.has(key)) {
        setSelectedPaths(new Set([key]));
      }
      setAnchorPath(row.path);
    },
    [backend],
  );

  const setSelection = useCallback(
    (row: FileExplorerTreeRow, event: ReactMouseEvent<HTMLDivElement>) => {
      const key = treePathKey(row.path, backend);
      const additive = event.ctrlKey || event.metaKey;
      const current = selectedPathsRef.current;
      let next: Set<string>;
      if (event.shiftKey && anchorPathRef.current) {
        const anchorKey = treePathKey(anchorPathRef.current, backend);
        const anchorIndex = rows.findIndex(
          (candidate) => treePathKey(candidate.path, backend) === anchorKey,
        );
        const targetIndex = rows.findIndex(
          (candidate) => treePathKey(candidate.path, backend) === key,
        );
        next = additive ? new Set(current) : new Set<string>();
        if (anchorIndex >= 0 && targetIndex >= 0) {
          const [start, end] = anchorIndex <= targetIndex
            ? [anchorIndex, targetIndex]
            : [targetIndex, anchorIndex];
          for (let index = start; index <= end; index += 1) {
            const candidate = rows[index];
            if (candidate && !candidate.isRoot) {
              next.add(treePathKey(candidate.path, backend));
            }
          }
        }
      } else if (additive && !row.isRoot) {
        next = new Set(current);
        if (next.has(key)) next.delete(key);
        else next.add(key);
      } else {
        next = row.isRoot ? new Set<string>() : new Set([key]);
      }
      setSelectedPaths(next);
      setAnchorPath(row.path);
    },
    [backend, rows],
  );

  const handleRowClick = useCallback(
    (row: FileExplorerTreeRow, event: ReactMouseEvent<HTMLDivElement>) => {
      if (event.button !== 0) return;
      // A tree row click changes selection only. Directory activation is an
      // explicit double-click/Enter/context-menu action; expansion remains
      // owned by the disclosure button.
      setSelection(row, event);
    },
    [setSelection],
  );

  const selectAll = useCallback(() => {
    const next = new Set<string>();
    for (const row of rows) {
      if (!row.isRoot) next.add(treePathKey(row.path, backend));
    }
    setSelectedPaths(next);
    setAnchorPath(rows.find((row) => !row.isRoot)?.path ?? null);
  }, [backend, rows]);

  const clearSelection = useCallback(() => {
    setSelectedPaths(new Set());
    setAnchorPath(null);
  }, []);

  const toggleDirectory = useCallback(
    (row: FileExplorerTreeRow) => {
      if (row.isRoot || !isTreeDirectoryExpandable(row.entry)) return;
      const key = treePathKey(row.path, backend);
      const wasExpanded = expandedPathsRef.current.has(key);
      setExpandedPaths((previous) => {
        const next = new Set(previous);
        if (wasExpanded) next.delete(key);
        else next.add(key);
        return next;
      });
      if (!wasExpanded) {
        void loadTreeDirectory(row.path, row.rawPathToken).catch(() => {});
      }
    },
    [backend, loadTreeDirectory],
  );

  const activateDirectory = useCallback(
    (row: FileExplorerTreeRow) => {
      if (!row.entry.is_dir) return;
      const activationGeneration = ++activationGenerationRef.current;
      const requestCacheKey = cacheKey;
      void (async () => {
        try {
          const entries = await loadTreeDirectory(row.path, row.rawPathToken);
          if (
            cacheKeyRef.current !== requestCacheKey ||
            activationGenerationRef.current !== activationGeneration
          ) {
            return;
          }
          onDirectorySelected(row, entries);
        } catch {
          // The row retains its inline error state.
        }
      })();
    },
    [cacheKey, loadTreeDirectory, onDirectorySelected],
  );

  const revealPath = useCallback(
    async (
      path: string,
      options?: { targetType?: "directory" | "file" },
    ) => {
      const normalizedPath = normalizeExplorerPath(path, backend);
      if (!normalizedPath || !normalizedRootPath) return;
      const chainTargetPath =
        options?.targetType === "file"
          ? getExplorerParentDirectory(normalizedPath, backend)
          : normalizedPath;
      const chain = buildTreePathChain(
        normalizedRootPath,
        chainTargetPath,
        backend,
      );
      if (chain.length === 0) return;

      let rawPathToken = rootRawPathToken;
      const nextExpanded = new Set(expandedPathsRef.current);
      for (let index = 0; index < chain.length; index += 1) {
        if (cacheKeyRef.current !== cacheKey) return;
        const directoryPath = chain[index];
        const entries = await loadTreeDirectory(directoryPath, rawPathToken);
        const directoryKey = treePathKey(directoryPath, backend);
        nextExpanded.add(directoryKey);
        const childPath = chain[index + 1];
        if (!childPath) continue;
        const child = findChildEntry(directoryPath, childPath, entries, backend);
        rawPathToken = child?.raw_path_token;
      }
      if (cacheKeyRef.current !== cacheKey) return;
      setExpandedPaths(nextExpanded);
      setSelectedPaths(new Set([treePathKey(normalizedPath, backend)]));
      setAnchorPath(normalizedPath);
    },
    [
      backend,
      cacheKey,
      loadTreeDirectory,
      normalizedRootPath,
      rootRawPathToken,
    ],
  );

  useEffect(() => {
    if (!enabled || stateCacheKey !== cacheKey || !currentPath) return;
    void revealPath(currentPath).catch(() => {});
  }, [cacheKey, currentPath, enabled, revealPath, stateCacheKey]);

  const refreshDirectory = useCallback(
    async (path: string) => {
      const normalizedPath = normalizeExplorerPath(path, backend);
      if (!normalizedPath) return [];
      const key = treePathKey(normalizedPath, backend);
      const record = directoriesRef.current.get(key);
      return loadTreeDirectory(normalizedPath, record?.rawPathToken, true);
    },
    [backend, loadTreeDirectory],
  );

  const invalidateDirectories = useCallback(
    (paths: string[]) => {
      const normalizedPaths = paths
        .map((path) => normalizeExplorerPath(path, backend))
        .filter((path): path is string => !!path);
      if (normalizedPaths.length === 0) return;
      const records = normalizedPaths.map((path) => {
        const key = treePathKey(path, backend);
        return { key, path, record: directoriesRef.current.get(key) };
      });
      setDirectories((previous) => {
        const next = new Map(previous);
        for (const { key, path, record } of records) {
          next.set(key, {
            path,
            rawPathToken: record?.rawPathToken,
            status: "idle",
            children: [],
            loadedAt: record?.loadedAt,
          });
        }
        return next;
      });
      for (const { path, record } of records) {
        const key = treePathKey(path, backend);
        if (key === treePathKey(normalizedRootPath, backend) || expandedPathsRef.current.has(key)) {
          void loadTreeDirectory(path, record?.rawPathToken, true).catch(() => {});
        }
      }
    },
    [backend, loadTreeDirectory, normalizedRootPath],
  );

  const refreshAll = useCallback(() => {
    const paths = [...directoriesRef.current.values()].map((record) => record.path);
    invalidateDirectories(paths.length > 0 ? paths : [normalizedRootPath]);
  }, [invalidateDirectories, normalizedRootPath]);

  const handleRowKeyDown = useCallback(
    (event: ReactKeyboardEvent<HTMLDivElement>, row: FileExplorerTreeRow) => {
      if (event.key === "Enter") {
        event.preventDefault();
        if (row.entry.is_dir) {
          activateDirectory(row);
          if (isTreeDirectoryExpandable(row.entry)) toggleDirectory(row);
        }
      } else if (event.key === "ArrowRight") {
        event.preventDefault();
        if (row.entry.is_dir && !row.isExpanded) toggleDirectory(row);
      } else if (event.key === "ArrowLeft") {
        event.preventDefault();
        if (row.entry.is_dir && row.isExpanded) toggleDirectory(row);
      } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        const index = rows.findIndex(
          (candidate) => treePathKey(candidate.path, backend) === treePathKey(row.path, backend),
        );
        const offset = event.key === "ArrowDown" ? 1 : -1;
        const next = rows[Math.max(0, Math.min(rows.length - 1, index + offset))];
        if (next) {
          const syntheticEvent = {
            button: 0,
            ctrlKey: event.ctrlKey,
            metaKey: event.metaKey,
            shiftKey: event.shiftKey,
          } as ReactMouseEvent<HTMLDivElement>;
          setSelection(next, syntheticEvent);
        }
      } else if (event.key === "F2") {
        event.preventDefault();
        // The parent owns rename state and receives the selected row through the context menu/action path.
        setSelectedPaths(new Set([treePathKey(row.path, backend)]));
        setAnchorPath(row.path);
      }
    },
    [activateDirectory, backend, rows, setSelection, toggleDirectory],
  );

  return {
    root,
    rows,
    selectedRows,
    selectedPaths,
    getDirectorySnapshot,
    selectContextRow,
    handleRowClick,
    handleRowKeyDown,
    selectAll,
    clearSelection,
    toggleDirectory,
    activateDirectory,
    revealPath,
    refreshDirectory,
    refreshAll,
    invalidateDirectories,
  };
}
