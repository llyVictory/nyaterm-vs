import { useEffect, useLayoutEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { MdFileOpen, MdRefresh, MdSend } from "react-icons/md";
import { getFileIcon } from "@/components/icons";
import { formatSize } from "@/lib/utils";
import FileExplorerEntryContextMenu from "./FileExplorerEntryContextMenu";
import type { FileExplorerTreeRow } from "./fileExplorerTreeModel";
import type { AICustomActionConfig, FileEntry } from "@/types/global";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "../../ui/context-menu";
import { HoverCard, HoverCardContent, HoverCardTrigger } from "../../ui/hover-card";

interface FileListItemProps {
  entry: FileEntry;
  isSelected: boolean;
  selectedCount: number;
  isParentDirectoryEntry?: boolean;
  entryPath: string;
  entryParentPath: string;
  activeSessionId: string | null;
  editorType: "external" | "internal";
  columnTemplate: string;
  rowWidth: number;
  onSelectionStart: (entry: FileEntry, event: React.MouseEvent) => void;
  onSelectionDrag: (entry: FileEntry, event: React.MouseEvent) => void;
  onContextMenuSelect: (entry: FileEntry, event: React.MouseEvent) => void;
  onItemClick: (entry: FileEntry) => void;
  onOpenDefault: (entry: FileEntry) => void;
  onPreview: (entry: FileEntry) => void;
  onOpenInternal: (entry: FileEntry) => void;
  onOpenExternal: (entry: FileEntry) => void;
  onRefresh: () => void;
  showTransferActions: boolean;
  onUpload: () => void;
  onUploadFolder: () => void;
  onDownload: (entry: FileEntry) => void;
  showPeerSendAction?: boolean;
  onSendToPeer?: (entry: FileEntry) => void;
  sendTargetOptions?: Array<{
    sessionId: string;
    label: string;
    meta: string;
  }>;
  onSendToTarget?: (entry: FileEntry, targetSessionId: string) => void;
  onRename: (entry: FileEntry) => void;
  onMove: (entry: FileEntry) => void;
  onDelete: (entry: FileEntry) => void;
  onAddToFavorites: (entry: FileEntry) => void;
  onCopyPath: (entry: FileEntry, mode: "dir" | "name" | "full") => void;
  onSendToTerminal?: (entry: FileEntry, mode: "dir" | "name" | "full") => void;
  onProperties: (entry: FileEntry) => void;
  aiActions: AICustomActionConfig[];
  onAIAction: (entry: FileEntry, action: AICustomActionConfig) => void;
  inlineRename?: {
    value: string;
    isSubmitting: boolean;
  } | null;
  onInlineRenameChange: (value: string) => void;
  onInlineRenameSubmit: () => void;
  onInlineRenameCancel: () => void;
}

function formatModifiedTime(unix: number): string {
  if (!unix) return "-";
  const d = new Date(unix * 1000);
  const pad = (n: number) => n.toString().padStart(2, "0");
  return `${d.getFullYear()}/${pad(d.getMonth() + 1)}/${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

function getFilenameSelectionEnd(name: string): number {
  const lastDot = name.lastIndexOf(".");
  return lastDot > 0 ? lastDot : name.length;
}

export function FileListItem({
  entry,
  isSelected,
  selectedCount,
  isParentDirectoryEntry = false,
  entryPath,
  entryParentPath,
  activeSessionId,
  editorType,
  columnTemplate,
  rowWidth,
  onSelectionStart,
  onSelectionDrag,
  onContextMenuSelect,
  onItemClick,
  onOpenDefault,
  onPreview,
  onOpenInternal,
  onOpenExternal,
  onRefresh,
  showTransferActions,
  onUpload,
  onUploadFolder,
  onDownload,
  showPeerSendAction = false,
  onSendToPeer,
  sendTargetOptions = [],
  onSendToTarget,
  onRename,
  onMove,
  onDelete,
  onAddToFavorites,
  onCopyPath,
  onSendToTerminal,
  onProperties,
  aiActions,
  onAIAction,
  inlineRename,
  onInlineRenameChange,
  onInlineRenameSubmit,
  onInlineRenameCancel,
}: FileListItemProps) {
  const { t } = useTranslation();
  const renameInputRef = useRef<HTMLInputElement | null>(null);
  const renameBlurGuardUntilRef = useRef(0);
  const renameClickTimerRef = useRef<number | null>(null);
  const wasSingleSelectedOnMouseDownRef = useRef(false);
  const preventNextContextMenuAutoFocusRef = useRef(false);
  const entryIcon = getFileIcon(entry);
  const modifiedTime = formatModifiedTime(entry.mtime);
  const fileSize = isParentDirectoryEntry || entry.is_dir ? "-" : formatSize(entry.size);
  const permissions = isParentDirectoryEntry ? "" : entry.permissions || "-";
  const owner = isParentDirectoryEntry ? "" : entry.owner || "-";
  const group = isParentDirectoryEntry ? "" : entry.group || "-";
  const isRenaming = !!inlineRename;
  const contextRow: FileExplorerTreeRow = {
    entry,
    path: entryPath,
    parentPath: entryParentPath,
    depth: 0,
    isRoot: false,
    isExpanded: false,
    isLoading: false,
    hasLoadedChildren: false,
  };

  useLayoutEffect(() => {
    if (!isRenaming) {
      return;
    }

    renameBlurGuardUntilRef.current = performance.now() + 350;
    const input = renameInputRef.current;
    if (!input) return;
    input.focus();
    input.setSelectionRange(0, getFilenameSelectionEnd(entry.name));
  }, [entry.name, isRenaming]);

  useEffect(() => {
    if (!isRenaming) {
      return;
    }

    let frame = 0;
    const timeout = window.setTimeout(() => {
      frame = window.requestAnimationFrame(() => {
        const input = renameInputRef.current;
        if (!input || document.activeElement === input) return;
        input.focus();
        input.setSelectionRange(0, getFilenameSelectionEnd(entry.name));
      });
      renameBlurGuardUntilRef.current = performance.now() + 350;
    }, 0);

    return () => {
      window.clearTimeout(timeout);
      if (frame !== 0) {
        window.cancelAnimationFrame(frame);
      }
    };
  }, [entry.name, isRenaming]);

  useEffect(() => {
    return () => {
      if (renameClickTimerRef.current !== null) {
        window.clearTimeout(renameClickTimerRef.current);
      }
    };
  }, []);

  const clearPendingRenameClick = () => {
    if (renameClickTimerRef.current === null) return;
    window.clearTimeout(renameClickTimerRef.current);
    renameClickTimerRef.current = null;
  };

  const handleNameClick = (event: React.MouseEvent<HTMLSpanElement>) => {
    if (
      event.button !== 0 ||
      event.detail !== 1 ||
      isRenaming ||
      !activeSessionId ||
      !wasSingleSelectedOnMouseDownRef.current ||
      isParentDirectoryEntry
    ) {
      return;
    }

    clearPendingRenameClick();
    renameClickTimerRef.current = window.setTimeout(() => {
      renameClickTimerRef.current = null;
      onRename(entry);
    }, 220);
  };

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <li
          className="group relative grid h-[30px] items-center rounded transition-colors cursor-pointer select-none"
          style={{
            gridTemplateColumns: columnTemplate,
            width: rowWidth,
            backgroundColor: isSelected
              ? "color-mix(in srgb, var(--df-primary) 10%, transparent)"
              : undefined,
            color: isSelected ? "var(--df-primary)" : "var(--df-text)",
          }}
          onMouseEnter={(e) => {
            if (!isSelected) e.currentTarget.style.backgroundColor = "var(--df-bg-hover)";
            onSelectionDrag(entry, e);
          }}
          onMouseLeave={(e) => {
            if (!isSelected) e.currentTarget.style.backgroundColor = "";
          }}
          onMouseDown={(e) => {
            wasSingleSelectedOnMouseDownRef.current =
              e.button === 0 && isSelected && selectedCount === 1;
            clearPendingRenameClick();
            if (isRenaming) {
              e.stopPropagation();
              return;
            }
            onSelectionStart(entry, e);
          }}
          onDoubleClick={() => {
            clearPendingRenameClick();
            if (isRenaming) return;
            if (entry.is_dir) {
              onItemClick(entry);
            } else {
              onOpenDefault(entry);
            }
          }}
          onContextMenu={(e) => {
            if (isRenaming) {
              e.preventDefault();
              e.stopPropagation();
              return;
            }
            onContextMenuSelect(entry, e);
          }}
          title={isParentDirectoryEntry ? t("fileExplorer.goUp") : undefined}
        >
          <div className="flex min-w-0 items-center gap-2 px-2">
            <entryIcon.icon
              className="shrink-0 text-base"
              style={{ color: isSelected ? "var(--df-primary)" : entryIcon.color }}
            />
            {isRenaming ? (
              <input
                ref={renameInputRef}
                type="text"
                className="h-6 min-w-0 flex-1 rounded border border-[var(--df-primary)] bg-[var(--df-bg-panel)] px-1.5 text-xs text-[var(--df-text)] outline-none"
                value={inlineRename.value}
                autoCapitalize="none"
                autoCorrect="off"
                spellCheck={false}
                autoComplete="off"
                onChange={(event) => onInlineRenameChange(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    event.preventDefault();
                    event.stopPropagation();
                    onInlineRenameSubmit();
                  } else if (event.key === "Escape") {
                    event.preventDefault();
                    event.stopPropagation();
                    onInlineRenameCancel();
                  }
                }}
                onBlur={() => {
                  if (performance.now() < renameBlurGuardUntilRef.current) {
                    window.setTimeout(() => {
                      renameInputRef.current?.focus();
                    }, 0);
                    return;
                  }
                  if (!inlineRename.isSubmitting) {
                    onInlineRenameCancel();
                  }
                }}
                onMouseDown={(event) => event.stopPropagation()}
                onClick={(event) => event.stopPropagation()}
                onDoubleClick={(event) => event.stopPropagation()}
                onContextMenu={(event) => event.stopPropagation()}
                disabled={inlineRename.isSubmitting}
              />
            ) : (
              <>
                {isParentDirectoryEntry ? (
                  <span className="min-w-0 flex-1 truncate text-xs" onClick={handleNameClick}>
                    {entry.name}
                  </span>
                ) : (
                  <HoverCard openDelay={450} closeDelay={100}>
                    <HoverCardTrigger asChild>
                      <span
                        className="min-w-0 flex-1 truncate text-xs"
                        onClick={handleNameClick}
                      >
                        {entry.name}
                      </span>
                    </HoverCardTrigger>
                    <HoverCardContent
                      side="top"
                      align="start"
                      className="w-80 max-w-[calc(100vw-2rem)] p-3"
                    >
                      <div className="mb-2 break-all text-sm font-medium leading-snug">
                        {entry.name}
                      </div>
                      <div className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-1 text-xs">
                        <span className="text-muted-foreground">{t("fileExplorer.mtime")}</span>
                        <span className="min-w-0 break-words font-mono">{modifiedTime}</span>
                        <span className="text-muted-foreground">{t("fileExplorer.size")}</span>
                        <span className="min-w-0 break-words">{fileSize}</span>
                        <span className="text-muted-foreground">{t("fileExplorer.permissions")}</span>
                        <span className="min-w-0 break-all font-mono">{permissions}</span>
                        <span className="text-muted-foreground">{t("fileExplorer.owner")}</span>
                        <span className="min-w-0 break-all">{owner}</span>
                        <span className="text-muted-foreground">{t("fileExplorer.group")}</span>
                        <span className="min-w-0 break-all">{group}</span>
                      </div>
                    </HoverCardContent>
                  </HoverCard>
                )}
                {showPeerSendAction && !isParentDirectoryEntry && onSendToPeer && (
                  <button
                    type="button"
                    className="flex h-5 w-5 shrink-0 items-center justify-center rounded text-[var(--df-text-dimmed)] opacity-0 transition-opacity hover:bg-[var(--df-bg-hover)] hover:text-[var(--df-primary)] group-hover:opacity-100 group-focus-within:opacity-100"
                    aria-label={t("fileExplorer.sendToPeer")}
                    title={t("fileExplorer.sendToPeer")}
                    onClick={(event) => {
                      event.preventDefault();
                      event.stopPropagation();
                      onSendToPeer(entry);
                    }}
                  >
                    <MdSend className="h-3.5 w-3.5" />
                  </button>
                )}
              </>
            )}
          </div>
          <span
            className="truncate px-2 font-mono text-[0.625rem] tabular-nums"
            style={{ color: "var(--df-text-dimmed)" }}
          >
            {isParentDirectoryEntry ? "" : modifiedTime}
          </span>
          <span
            className="truncate px-2 text-right text-[0.625rem] tabular-nums"
            style={{ color: "var(--df-text-dimmed)" }}
          >
            {isParentDirectoryEntry ? "" : fileSize}
          </span>
          <span
            className="truncate px-2 font-mono text-[0.625rem]"
            style={{ color: "var(--df-text-dimmed)" }}
          >
            {permissions}
          </span>
          <span
            className="truncate px-2 text-[0.625rem]"
            style={{ color: "var(--df-text-dimmed)" }}
          >
            {owner}
          </span>
          <span
            className="truncate px-2 text-[0.625rem]"
            style={{ color: "var(--df-text-dimmed)" }}
          >
            {group}
          </span>
        </li>
      </ContextMenuTrigger>
      {isParentDirectoryEntry ? (
        <ContextMenuContent
          className="min-w-[200px]"
          onCloseAutoFocus={(event) => {
            if (!preventNextContextMenuAutoFocusRef.current) {
              return;
            }
            preventNextContextMenuAutoFocusRef.current = false;
            event.preventDefault();
          }}
        >
          <ContextMenuItem onClick={() => onItemClick(entry)}>
            <MdFileOpen className="text-[0.875rem] text-muted-foreground mr-2" />
            {t("fileExplorer.goUp")}
          </ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem onClick={onRefresh}>
            <MdRefresh className="text-[0.875rem] text-muted-foreground mr-2" />
            {t("fileExplorer.cmRefresh")}
          </ContextMenuItem>
        </ContextMenuContent>
      ) : (
        <FileExplorerEntryContextMenu
          target={contextRow}
          selectedTargets={[contextRow]}
          activeSessionId={activeSessionId}
          editorType={editorType}
          showTransferActions={showTransferActions}
          terminalInputEnabled={!!onSendToTerminal}
          sendTargetOptions={sendTargetOptions}
          getAiActions={() => aiActions}
          onOpenDirectory={(row) => onItemClick(row.entry)}
          onOpenDefault={(row) => onOpenDefault(row.entry)}
          onPreview={(row) => onPreview(row.entry)}
          onOpenInternal={(row) => onOpenInternal(row.entry)}
          onOpenExternal={(row) => onOpenExternal(row.entry)}
          onCloseAutoFocus={(event) => {
            if (!preventNextContextMenuAutoFocusRef.current) return;
            preventNextContextMenuAutoFocusRef.current = false;
            event.preventDefault();
          }}
          onRefresh={() => onRefresh()}
          onUpload={() => onUpload()}
          onUploadFolder={() => onUploadFolder()}
          onDownload={(rows) => {
            const row = rows[0];
            if (row) onDownload(row.entry);
          }}
          onSendToPeer={
            showPeerSendAction && onSendToPeer
              ? (rows) => {
                  const row = rows[0];
                  if (row) onSendToPeer(row.entry);
                }
              : undefined
          }
          onSendToTarget={
            onSendToTarget
              ? (rows, targetSessionId) => {
                  const row = rows[0];
                  if (row) onSendToTarget(row.entry, targetSessionId);
                }
              : undefined
          }
          onRename={(row) => {
            preventNextContextMenuAutoFocusRef.current = true;
            onRename(row.entry);
          }}
          onMove={(rows) => {
            const row = rows[0];
            if (row) onMove(row.entry);
          }}
          onDelete={(rows) => {
            const row = rows[0];
            if (row) onDelete(row.entry);
          }}
          onAddToFavorites={(row) => onAddToFavorites(row.entry)}
          onCopyPath={(row, mode) => onCopyPath(row.entry, mode)}
          onSendToTerminal={
            onSendToTerminal
              ? (row, mode) => onSendToTerminal(row.entry, mode)
              : undefined
          }
          onProperties={(row) => onProperties(row.entry)}
          onAIAction={(row, action) => onAIAction(row.entry, action)}
        />
      )}
    </ContextMenu>
  );
}
