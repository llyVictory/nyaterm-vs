import type { ComponentProps, RefObject } from "react";
import { useTranslation } from "react-i18next";
import {
  MdAccountTree,
  MdArrowUpward,
  MdClose,
  MdCreateNewFolder,
  MdDelete,
  MdDownload,
  MdDriveFolderUpload,
  MdMyLocation,
  MdNoteAdd,
  MdRefresh,
  MdSearch,
  MdUpload,
  MdViewList,
  MdVisibility,
  MdVisibilityOff,
} from "react-icons/md";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";

type ToolbarIconButtonProps = ComponentProps<typeof Button> & {
  label: string;
};

function ToolbarIconButton({ label, children, ...props }: ToolbarIconButtonProps) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button aria-label={label} type="button" {...props}>
          {children}
        </Button>
      </TooltipTrigger>
      <TooltipContent side="top">{label}</TooltipContent>
    </Tooltip>
  );
}

function ToolbarDivider() {
  return (
    <span
      aria-hidden="true"
      className="mx-1 h-3 w-px shrink-0 rounded-full"
      style={{ backgroundColor: "var(--df-border)" }}
    />
  );
}

interface FileExplorerToolbarProps {
  isTreeView: boolean;
  selectedCount: number;
  isFileSearchActive: boolean;
  isFileSearchExpanded: boolean;
  showHiddenFiles: boolean;
  showTransferActions: boolean;
  fileSearchQuery: string;
  fileSearchInputRef: RefObject<HTMLInputElement | null>;
  onNewFile: () => void;
  onNewFolder: () => void;
  onUploadFiles: () => void;
  onUploadFolder: () => void;
  onDownloadSelected: () => void;
  onDeleteSelected: () => void;
  onGoUp: () => void;
  onRefresh: () => void;
  onLocatePath: () => void;
  locateLabel: string;
  canLocatePath: boolean;
  onToggleViewMode: () => void;
  onToggleHiddenFiles: () => void;
  onExpandSearch: () => void;
  onSearchQueryChange: (query: string) => void;
  onCollapseSearch: () => void;
}

export function FileExplorerToolbar({
  isTreeView,
  selectedCount,
  isFileSearchActive,
  isFileSearchExpanded,
  showHiddenFiles,
  showTransferActions,
  fileSearchQuery,
  fileSearchInputRef,
  onNewFile,
  onNewFolder,
  onUploadFiles,
  onUploadFolder,
  onDownloadSelected,
  onDeleteSelected,
  onGoUp,
  onRefresh,
  onLocatePath,
  locateLabel,
  canLocatePath,
  onToggleViewMode,
  onToggleHiddenFiles,
  onExpandSearch,
  onSearchQueryChange,
  onCollapseSearch,
}: FileExplorerToolbarProps) {
  const { t } = useTranslation();

  return (
    <div
      className="nyaterm-wallpaper-transparent-surface relative flex items-center px-1.5 py-1 border-b gap-0.5"
      style={{ backgroundColor: "var(--df-bg-panel)", borderColor: "var(--df-border)" }}
    >
      <ToolbarIconButton
        label={t("fileExplorer.newFile")}
        variant="ghost"
        size="icon"
        className="h-7 w-7 rounded-md text-muted-foreground hover:text-foreground"
        onClick={onNewFile}
      >
        <MdNoteAdd className="h-4 w-4" />
      </ToolbarIconButton>
      <ToolbarIconButton
        label={t("fileExplorer.newFolder")}
        variant="ghost"
        size="icon"
        className="h-7 w-7 rounded-md text-muted-foreground hover:text-foreground"
        onClick={onNewFolder}
      >
        <MdCreateNewFolder className="h-4 w-4" />
      </ToolbarIconButton>

      {showTransferActions && (
        <>
          <ToolbarDivider />

          <DropdownMenu>
            <Tooltip>
              <TooltipTrigger asChild>
                <DropdownMenuTrigger asChild>
                  <Button
                    aria-label={t("fileExplorer.upload")}
                    type="button"
                    variant="ghost"
                    size="icon"
                    className="h-7 w-7 rounded-md text-muted-foreground hover:text-foreground"
                  >
                    <MdUpload className="h-4 w-4" />
                  </Button>
                </DropdownMenuTrigger>
              </TooltipTrigger>
              <TooltipContent side="top">{t("fileExplorer.upload")}</TooltipContent>
            </Tooltip>
            <DropdownMenuContent align="start" className="min-w-44">
              <DropdownMenuItem onClick={onUploadFiles}>
                <MdUpload className="mr-2 h-4 w-4" />
                {t("fileExplorer.upload")}
              </DropdownMenuItem>
              <DropdownMenuItem onClick={onUploadFolder}>
                <MdDriveFolderUpload className="mr-2 h-4 w-4" />
                {t("fileExplorer.uploadFolder")}
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
          <ToolbarIconButton
            label={t("fileExplorer.downloadSelected")}
            variant="ghost"
            size="icon"
            className="h-7 w-7 rounded-md text-muted-foreground hover:text-foreground"
            onClick={onDownloadSelected}
            disabled={selectedCount === 0}
          >
            <MdDownload className="h-4 w-4" />
          </ToolbarIconButton>
        </>
      )}
      <ToolbarIconButton
        label={t("fileExplorer.delete")}
        variant="ghost"
        size="icon"
        className="h-7 w-7 rounded-md text-muted-foreground hover:text-destructive"
        onClick={onDeleteSelected}
        disabled={selectedCount === 0}
      >
        <MdDelete className="h-4 w-4" />
      </ToolbarIconButton>

      <ToolbarDivider />

      {!isTreeView && (
        <ToolbarIconButton
          label={t("fileExplorer.goUp")}
          variant="ghost"
          size="icon"
          className="h-7 w-7 rounded-md text-muted-foreground hover:text-foreground"
          onClick={onGoUp}
        >
          <MdArrowUpward className="h-4 w-4" />
        </ToolbarIconButton>
      )}
      <ToolbarIconButton
        label={t("fileExplorer.refresh")}
        variant="ghost"
        size="icon"
        className="h-7 w-7 rounded-md text-muted-foreground hover:text-foreground"
        onClick={onRefresh}
      >
        <MdRefresh className="h-4 w-4" />
      </ToolbarIconButton>

      {isTreeView && (
        <>
          <ToolbarDivider />
          <ToolbarIconButton
            label={
              canLocatePath
                ? locateLabel
                : t("fileExplorer.cwdTrackingUnavailable")
            }
            variant="ghost"
            size="icon"
            className="h-7 w-7 rounded-md text-muted-foreground hover:text-foreground disabled:opacity-40"
            onClick={onLocatePath}
            disabled={!canLocatePath}
          >
            <MdMyLocation className="h-4 w-4" />
          </ToolbarIconButton>
        </>
      )}

      <ToolbarDivider />

      <div className="ml-auto flex shrink-0 items-center gap-0.5">
        <ToolbarIconButton
          label={
            isTreeView
              ? t("fileExplorer.switchToListView")
              : t("fileExplorer.switchToTreeView")
          }
          variant="ghost"
          size="icon"
          className="h-7 w-7 rounded-md text-muted-foreground hover:text-foreground"
          onClick={onToggleViewMode}
        >
          {isTreeView ? <MdViewList className="h-4 w-4" /> : <MdAccountTree className="h-4 w-4" />}
        </ToolbarIconButton>
        {!isTreeView && (
          <ToolbarIconButton
            label={t("fileExplorer.search")}
            variant="ghost"
            size="icon"
            className={cn(
              "h-7 w-7 rounded-md hover:text-foreground",
              isFileSearchActive ? "bg-primary/10 text-primary" : "text-muted-foreground",
            )}
            onClick={onExpandSearch}
          >
            <MdSearch className="h-4 w-4 translate-y-px" />
          </ToolbarIconButton>
        )}
        <ToolbarIconButton
          label={
            showHiddenFiles ? t("fileExplorer.hideHiddenFiles") : t("fileExplorer.showHiddenFiles")
          }
          variant="ghost"
          size="icon"
          className={cn(
            "h-7 w-7 rounded-md hover:text-foreground",
            showHiddenFiles ? "bg-primary/10 text-primary" : "text-muted-foreground",
          )}
          onClick={onToggleHiddenFiles}
        >
          {showHiddenFiles ? (
            <MdVisibility className="h-4 w-4" />
          ) : (
            <MdVisibilityOff className="h-4 w-4" />
          )}
        </ToolbarIconButton>
      </div>

      {!isTreeView && isFileSearchExpanded && (
        <div
          className="nyaterm-wallpaper-control-surface absolute inset-x-1.5 top-1 bottom-1 z-20 flex items-center gap-1 rounded-md border px-1.5 shadow-sm"
          style={{
            backgroundColor: "var(--df-bg-panel)",
            borderColor: "var(--df-primary)",
          }}
        >
          <MdSearch className="h-4 w-4 shrink-0 translate-y-px text-primary" />
          <input
            ref={fileSearchInputRef}
            type="text"
            value={fileSearchQuery}
            onChange={(event) => onSearchQueryChange(event.target.value)}
            autoCapitalize="none"
            autoCorrect="off"
            spellCheck={false}
            autoComplete="off"
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                if (fileSearchQuery) {
                  onSearchQueryChange("");
                } else {
                  onCollapseSearch();
                }
              }
            }}
            placeholder={t("fileExplorer.searchPlaceholder")}
            className="h-full min-w-0 flex-1 bg-transparent px-1 text-xs text-[var(--df-text)] outline-none placeholder:text-[var(--df-text-dimmed)]"
          />
          <button
            type="button"
            className="flex h-5 w-5 shrink-0 items-center justify-center rounded text-[var(--df-text-dimmed)] transition-colors hover:bg-[var(--df-bg-hover)] hover:text-[var(--df-text)]"
            aria-label={fileSearchQuery ? t("fileExplorer.clearSearch") : t("common.close")}
            onClick={() => {
              if (fileSearchQuery) {
                onSearchQueryChange("");
                fileSearchInputRef.current?.focus();
              } else {
                onCollapseSearch();
              }
            }}
          >
            <MdClose className="h-3.5 w-3.5" />
          </button>
        </div>
      )}
    </div>
  );
}
