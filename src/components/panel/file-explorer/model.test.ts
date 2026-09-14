import { describe, expect, it, vi } from "vitest";
import {
  buildMoveSuccessRefreshPlan,
  buildMoveOperations,
  buildMoveTargetPath,
  canTrackTerminalCwd,
  isMoveToSameDirectory,
  normalizeFileExplorerViewMode,
  subscribeFileExplorerSessionSnapshots,
  syncExplorerDirectoryToTerminalCwd,
  syncExplorerDirectoryToTerminalCwdChange,
} from "./model";

describe("file explorer view mode", () => {
  it("falls back to list for missing or unknown values", () => {
    expect(normalizeFileExplorerViewMode(undefined)).toBe("list");
    expect(normalizeFileExplorerViewMode("grid")).toBe("list");
  });

  it("accepts the tree view mode", () => {
    expect(normalizeFileExplorerViewMode("tree")).toBe("tree");
  });
});

describe("file explorer move path helpers", () => {
  it("moves a single file to a target directory", () => {
    expect(buildMoveTargetPath("/data", "A.txt", "remote")).toBe(
      "/data/A.txt",
    );
  });

  it("moves a single directory to a target directory", () => {
    expect(buildMoveTargetPath("/data", "A", "remote")).toBe("/data/A");
  });

  it("builds operations for multiple files", () => {
    expect(
      buildMoveOperations(
        {
          backend: "remote",
          items: [
            { oldPath: "/home/A.txt", name: "A.txt", isDirectory: false },
            { oldPath: "/home/B.txt", name: "B.txt", isDirectory: false },
          ],
        },
        "/data",
      ),
    ).toEqual([
      {
        oldPath: "/home/A.txt",
        name: "A.txt",
        isDirectory: false,
        newPath: "/data/A.txt",
      },
      {
        oldPath: "/home/B.txt",
        name: "B.txt",
        isDirectory: false,
        newPath: "/data/B.txt",
      },
    ]);
  });

  it("builds operations for mixed files and folders", () => {
    expect(
      buildMoveOperations(
        {
          backend: "remote",
          items: [
            { oldPath: "/home/A", name: "A", isDirectory: true },
            {
              oldPath: "/home/config.yaml",
              name: "config.yaml",
              isDirectory: false,
            },
          ],
        },
        "/opt/backup",
      ).map((operation) => operation.newPath),
    ).toEqual(["/opt/backup/A", "/opt/backup/config.yaml"]);
  });

  it("preserves each entry basename automatically", () => {
    expect(
      buildMoveOperations(
        {
          backend: "remote",
          items: [
            { oldPath: "/home/user/.env", name: ".env", isDirectory: false },
          ],
        },
        "/tmp",
      )[0]?.newPath,
    ).toBe("/tmp/.env");
  });

  it("joins remote Unix paths without local separators", () => {
    expect(buildMoveTargetPath("/", "config.yaml", "remote")).toBe(
      "/config.yaml",
    );
    expect(buildMoveTargetPath("/opt/files/", "config.yaml", "remote")).toBe(
      "/opt/files/config.yaml",
    );
  });

  it("joins local Windows drive and UNC paths with Windows separators", () => {
    expect(buildMoveTargetPath("C:\\Users\\me", "config.yaml", "local")).toBe(
      "C:\\Users\\me\\config.yaml",
    );
    expect(
      buildMoveTargetPath("\\\\server\\share", "config.yaml", "local"),
    ).toBe("\\\\server\\share\\config.yaml");
  });

  it("detects same source and target directories", () => {
    expect(isMoveToSameDirectory("/home/user", "/home/user/", "remote")).toBe(
      true,
    );
    expect(isMoveToSameDirectory("C:\\Data", "c:\\data\\", "local")).toBe(
      true,
    );
    expect(isMoveToSameDirectory("/home/user", "/opt/backup", "remote")).toBe(
      false,
    );
  });

  it("plans selection cleanup and source/target refresh after move", () => {
    expect(
      buildMoveSuccessRefreshPlan("/home/user", "/opt/backup/", "remote"),
    ).toEqual({
      sourceDirectory: "/home/user",
      targetDirectory: "/opt/backup",
      shouldClearSelection: true,
    });
  });
});

describe("file explorer terminal cwd sync", () => {
  it("uses the Local managed cwd integration capability instead of command injection", () => {
    expect(
      canTrackTerminalCwd({
        session_type: "Local",
        injection_active: false,
        dynamic_title_integration_active: true,
      }),
    ).toBe(true);
    expect(
      canTrackTerminalCwd({
        session_type: "Local",
        injection_active: true,
        dynamic_title_integration_active: false,
      }),
    ).toBe(false);
  });

  it("keeps SSH cwd tracking tied to command injection", () => {
    expect(
      canTrackTerminalCwd({
        session_type: "SSH",
        injection_active: true,
        dynamic_title_integration_active: false,
      }),
    ).toBe(true);
    expect(
      canTrackTerminalCwd({
        session_type: "SSH",
        injection_active: false,
        dynamic_title_integration_active: true,
      }),
    ).toBe(false);
  });

  it("registers sessions-changed before the initial snapshot and observes Local ready", async () => {
    let resolveListen!: (dispose: () => void) => void;
    let sessionsChanged!: () => void;
    let integrationActive = false;
    let cwdTrackingActive = false;
    const readSessions = vi.fn(async () => [
      {
        session_type: "Local" as const,
        injection_active: false,
        dynamic_title_integration_active: integrationActive,
      },
    ]);
    const listenSessionsChanged = vi.fn(
      (handler: () => void) =>
        new Promise<() => void>((resolve) => {
          sessionsChanged = handler;
          resolveListen = resolve;
        }),
    );

    const stop = subscribeFileExplorerSessionSnapshots({
      listenSessionsChanged,
      readSessions,
      onSessions: (sessions) => {
        cwdTrackingActive = canTrackTerminalCwd(sessions[0]);
      },
      onError: vi.fn(),
    });

    await Promise.resolve();
    expect(readSessions).not.toHaveBeenCalled();

    resolveListen(() => {});
    await Promise.resolve();
    await Promise.resolve();
    expect(readSessions).toHaveBeenCalledTimes(1);
    expect(cwdTrackingActive).toBe(false);

    integrationActive = true;
    sessionsChanged();
    await Promise.resolve();
    await Promise.resolve();
    expect(readSessions).toHaveBeenCalledTimes(2);
    expect(cwdTrackingActive).toBe(true);

    stop();
  });

  it("loads the terminal cwd silently when auto-sync is enabled and the path changed", async () => {
    const readTerminalCwd = vi.fn().mockResolvedValue("/new");
    const loadDirectory = vi.fn().mockResolvedValue(true);

    await expect(
      syncExplorerDirectoryToTerminalCwd({
        enabled: true,
        canBrowseFiles: true,
        sessionId: "session-1",
        backend: "remote",
        currentPath: "/old",
        readTerminalCwd,
        loadDirectory,
      }),
    ).resolves.toBe(true);

    expect(readTerminalCwd).toHaveBeenCalledWith("session-1");
    expect(loadDirectory).toHaveBeenCalledWith("/new", { silent: true });
  });

  it("does not reload when the terminal cwd matches the visible directory", async () => {
    const readTerminalCwd = vi.fn().mockResolvedValue("/old/");
    const loadDirectory = vi.fn().mockResolvedValue(true);

    await expect(
      syncExplorerDirectoryToTerminalCwd({
        enabled: true,
        canBrowseFiles: true,
        sessionId: "session-1",
        backend: "remote",
        currentPath: "/old",
        readTerminalCwd,
        loadDirectory,
      }),
    ).resolves.toBe(false);

    expect(readTerminalCwd).toHaveBeenCalledWith("session-1");
    expect(loadDirectory).not.toHaveBeenCalled();
  });

  it("does not reload a Windows Local path for drive-case or trailing-separator differences", async () => {
    const readTerminalCwd = vi.fn().mockResolvedValue("c:\\Work\\Project\\");
    const loadDirectory = vi.fn().mockResolvedValue(true);

    await expect(
      syncExplorerDirectoryToTerminalCwd({
        enabled: true,
        canBrowseFiles: true,
        sessionId: "session-1",
        backend: "local",
        currentPath: "C:\\Work\\Project",
        readTerminalCwd,
        loadDirectory,
      }),
    ).resolves.toBe(false);

    expect(loadDirectory).not.toHaveBeenCalled();
  });

  it("does not read the terminal cwd when auto-sync is disabled", async () => {
    const readTerminalCwd = vi.fn().mockResolvedValue("/new");
    const loadDirectory = vi.fn().mockResolvedValue(true);

    await expect(
      syncExplorerDirectoryToTerminalCwd({
        enabled: false,
        canBrowseFiles: true,
        sessionId: "session-1",
        backend: "remote",
        currentPath: "/old",
        readTerminalCwd,
        loadDirectory,
      }),
    ).resolves.toBe(false);

    expect(readTerminalCwd).not.toHaveBeenCalled();
    expect(loadDirectory).not.toHaveBeenCalled();
  });

  it("keeps the current directory when the terminal cwd is empty", async () => {
    const readTerminalCwd = vi.fn().mockResolvedValue(null);
    const loadDirectory = vi.fn().mockResolvedValue(true);

    await expect(
      syncExplorerDirectoryToTerminalCwd({
        enabled: true,
        canBrowseFiles: true,
        sessionId: "session-1",
        backend: "remote",
        currentPath: "/old",
        readTerminalCwd,
        loadDirectory,
      }),
    ).resolves.toBe(false);

    expect(readTerminalCwd).toHaveBeenCalledWith("session-1");
    expect(loadDirectory).not.toHaveBeenCalled();
  });

  it("keeps the current directory when reading the terminal cwd fails", async () => {
    const readTerminalCwd = vi.fn().mockRejectedValue(new Error("unavailable"));
    const loadDirectory = vi.fn().mockResolvedValue(true);

    await expect(
      syncExplorerDirectoryToTerminalCwd({
        enabled: true,
        canBrowseFiles: true,
        sessionId: "session-1",
        backend: "remote",
        currentPath: "/old",
        readTerminalCwd,
        loadDirectory,
      }),
    ).resolves.toBe(false);

    expect(readTerminalCwd).toHaveBeenCalledWith("session-1");
    expect(loadDirectory).not.toHaveBeenCalled();
  });

  it("loads cwd change events silently when the path changed", () => {
    const loadDirectory = vi.fn().mockResolvedValue(true);

    expect(
      syncExplorerDirectoryToTerminalCwdChange({
        backend: "remote",
        currentPath: "/old",
        cwd: "/new",
        loadDirectory,
      }),
    ).toBe(true);

    expect(loadDirectory).toHaveBeenCalledWith("/new", { silent: true });
  });

  it("ignores cwd change events for the visible directory", () => {
    const loadDirectory = vi.fn().mockResolvedValue(true);

    expect(
      syncExplorerDirectoryToTerminalCwdChange({
        backend: "remote",
        currentPath: "/old",
        cwd: "/old/",
        loadDirectory,
      }),
    ).toBe(false);

    expect(loadDirectory).not.toHaveBeenCalled();
  });

  it("ignores Windows Local cwd events that differ only by drive case or trailing separator", () => {
    const loadDirectory = vi.fn().mockResolvedValue(true);

    expect(
      syncExplorerDirectoryToTerminalCwdChange({
        backend: "local",
        currentPath: "C:\\Work\\Project",
        cwd: "c:\\Work\\Project\\",
        loadDirectory,
      }),
    ).toBe(false);

    expect(loadDirectory).not.toHaveBeenCalled();
  });
});
