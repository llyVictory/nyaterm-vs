import {
  type KeyboardEvent,
  type MouseEvent,
  type Ref,
  useEffect,
  useRef,
} from "react";
import { MdChevronRight, MdFolderOpen } from "react-icons/md";
import { getFileIcon } from "@/components/icons";
import { cn } from "@/lib/utils";
import type { FileExplorerTreeRow } from "./fileExplorerTreeModel";

interface FileExplorerTreeItemProps {
  row: FileExplorerTreeRow;
  selected: boolean;
  tabIndex: 0 | -1;
  itemRef?: Ref<HTMLDivElement>;
  onClick: (event: MouseEvent<HTMLDivElement>) => void;
  onDoubleClick: () => void;
  onFocus: () => void;
  onKeyDown: (event: KeyboardEvent<HTMLDivElement>) => void;
  onToggle: () => void;
  onRetry: () => void;
  inlineRename?: {
    value: string;
    isSubmitting: boolean;
  } | null;
  onInlineRenameChange: (value: string) => void;
  onInlineRenameSubmit: () => void;
  onInlineRenameCancel: () => void;
  labels: {
    collapse: string;
    expand: string;
    loading: string;
    retry: string;
  };
}

export default function FileExplorerTreeItem({
  row,
  selected,
  tabIndex,
  itemRef,
  onClick,
  onDoubleClick,
  onFocus,
  onKeyDown,
  onToggle,
  onRetry,
  inlineRename,
  onInlineRenameChange,
  onInlineRenameSubmit,
  onInlineRenameCancel,
  labels,
}: FileExplorerTreeItemProps) {
  const icon = getFileIcon(row.entry);
  const EntryIcon = icon.icon;
  const renameInputRef = useRef<HTMLInputElement | null>(null);
  const renameBlurGuardUntilRef = useRef(0);
  const canToggle = row.entry.is_dir && !row.entry.is_symlink && !row.isRoot;
  const hasError = row.directoryStatus === "error";
  const isDirectory = row.entry.is_dir;
  const isOpenDirectory = isDirectory && !row.entry.is_symlink && row.isExpanded;
  const rename = inlineRename;
  const isRenaming = !!rename;

  useEffect(() => {
    if (!isRenaming) return;
    renameBlurGuardUntilRef.current = performance.now() + 350;
    const input = renameInputRef.current;
    if (!input) return;
    input.focus();
    input.select();
    const frame = window.requestAnimationFrame(() => {
      if (document.activeElement !== input) {
        input.focus();
        input.select();
      }
    });
    return () => window.cancelAnimationFrame(frame);
  }, [isRenaming]);

  return (
    <div
      ref={itemRef}
      data-file-tree-path={row.path}
      className={cn(
        "group flex h-7 min-w-0 cursor-default items-center gap-1 rounded px-1 text-xs outline-none transition-colors",
        selected
          ? "bg-[color-mix(in_srgb,var(--df-primary)_18%,transparent)] text-[var(--df-text)]"
          : "text-[var(--df-text-muted)] hover:bg-[var(--df-bg-hover)] hover:text-[var(--df-text)]",
      )}
      style={{ paddingLeft: 4 + row.depth * 14 }}
      role="treeitem"
      aria-level={row.depth + 1}
      aria-selected={selected}
      aria-expanded={isDirectory && canToggle ? row.isExpanded : undefined}
      tabIndex={tabIndex}
      onFocus={onFocus}
      onClick={(event) => {
        event.currentTarget.focus();
        onClick(event);
      }}
      onDoubleClick={onDoubleClick}
      onKeyDown={onKeyDown}
    >
      {isDirectory && canToggle ? (
        <button
          type="button"
          aria-label={row.isExpanded ? labels.collapse : labels.expand}
          className="flex h-5 w-5 shrink-0 items-center justify-center rounded text-[var(--df-text-dimmed)] transition hover:bg-[var(--df-bg-hover)] hover:text-[var(--df-text)]"
          onClick={(event) => {
            event.stopPropagation();
            onToggle();
          }}
        >
          <MdChevronRight
            className={cn("text-base transition-transform", row.isExpanded && "rotate-90")}
          />
        </button>
      ) : (
        <span className="h-5 w-5 shrink-0" />
      )}

      <span className="flex h-5 w-5 shrink-0 items-center justify-center">
        {isOpenDirectory ? (
          <MdFolderOpen
            className="text-base"
            style={{ color: selected ? "var(--df-primary)" : "#fbbf24" }}
          />
        ) : (
          <EntryIcon
            className="text-base"
            style={{ color: selected ? "var(--df-primary)" : icon.color }}
          />
        )}
      </span>

      {rename ? (
        <input
          ref={renameInputRef}
          value={rename.value}
          disabled={rename.isSubmitting}
          className="h-5 min-w-0 flex-1 rounded border border-[var(--df-primary)] bg-[var(--df-bg-panel)] px-1 text-xs text-[var(--df-text)] outline-none"
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
            if (!rename.isSubmitting) onInlineRenameCancel();
          }}
          onMouseDown={(event) => event.stopPropagation()}
          onClick={(event) => event.stopPropagation()}
          onDoubleClick={(event) => event.stopPropagation()}
          onContextMenu={(event) => event.stopPropagation()}
        />
      ) : (
        <span className="min-w-0 flex-1 truncate" title={row.path}>
          {row.entry.name}
        </span>
      )}

      {row.isLoading && (
        <span
          aria-hidden="true"
          title={labels.loading}
          className="mr-1 size-3 shrink-0 animate-spin rounded-full border border-muted-foreground/30 border-t-muted-foreground"
        />
      )}
      {hasError && (
        <button
          type="button"
          className="mr-1 shrink-0 rounded px-1 text-[0.625rem] text-destructive hover:bg-destructive/10"
          title={row.directoryError}
          onClick={(event) => {
            event.preventDefault();
            event.stopPropagation();
            onRetry();
          }}
        >
          {labels.retry}
        </button>
      )}
    </div>
  );
}
