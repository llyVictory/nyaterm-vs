import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import {
  MdAutoAwesome,
  MdBookmarkAdd,
  MdContentCopy,
  MdCopyAll,
  MdCreateNewFolder,
  MdDelete,
  MdDownload,
  MdDriveFileMove,
  MdDriveFolderUpload,
  MdEdit,
  MdFileOpen,
  MdFolderCopy,
  MdInfo,
  MdKeyboardArrowRight,
  MdKeyboardDoubleArrowRight,
  MdKeyboardReturn,
  MdLink,
  MdNoteAdd,
  MdOpenInNew,
  MdRefresh,
  MdSend,
  MdUpload,
  MdVisibility,
} from "react-icons/md";
import type { AICustomActionConfig } from "@/types/global";
import {
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
} from "@/components/ui/context-menu";
import type { FileExplorerTreeRow } from "./fileExplorerTreeModel";

interface FileExplorerEntryContextMenuProps {
  target: FileExplorerTreeRow | null;
  selectedTargets: FileExplorerTreeRow[];
  activeSessionId: string | null;
  editorType: "external" | "internal";
  showTransferActions: boolean;
  terminalInputEnabled: boolean;
  sendTargetOptions: Array<{
    sessionId: string;
    label: string;
    meta: string;
  }>;
  getAiActions: (row: FileExplorerTreeRow) => AICustomActionConfig[];
  onOpenDirectory: (row: FileExplorerTreeRow) => void;
  onOpenDefault: (row: FileExplorerTreeRow) => void;
  onPreview: (row: FileExplorerTreeRow) => void;
  onOpenInternal: (row: FileExplorerTreeRow) => void;
  onOpenExternal: (row: FileExplorerTreeRow) => void;
  onCloseAutoFocus?: (event: Event) => void;
  onRefresh: (row?: FileExplorerTreeRow | null) => void;
  onNewFile?: (directoryPath: string) => void;
  onNewFolder?: (directoryPath: string) => void;
  onNewSymlink?: (directoryPath: string) => void;
  onUpload: (directoryPath: string) => void;
  onUploadFolder: (directoryPath: string) => void;
  onDownload: (rows: FileExplorerTreeRow[]) => void;
  onSendToPeer?: (rows: FileExplorerTreeRow[]) => void;
  onSendToTarget?: (
    rows: FileExplorerTreeRow[],
    targetSessionId: string,
  ) => void;
  onRename: (row: FileExplorerTreeRow) => void;
  onMove: (rows: FileExplorerTreeRow[]) => void;
  onDelete: (rows: FileExplorerTreeRow[]) => void;
  onAddToFavorites: (row: FileExplorerTreeRow) => void;
  onCopyPath: (row: FileExplorerTreeRow, mode: "dir" | "name" | "full") => void;
  onSendToTerminal?: (
    row: FileExplorerTreeRow,
    mode: "dir" | "name" | "full",
  ) => void;
  onProperties: (row: FileExplorerTreeRow) => void;
  onAIAction: (row: FileExplorerTreeRow, action: AICustomActionConfig) => void;
}

function rowsContain(rows: FileExplorerTreeRow[], target: FileExplorerTreeRow) {
  return rows.some((row) => row.path === target.path);
}

export default function FileExplorerEntryContextMenu({
  target,
  selectedTargets,
  activeSessionId,
  editorType,
  showTransferActions,
  terminalInputEnabled,
  sendTargetOptions,
  getAiActions,
  onOpenDirectory,
  onOpenDefault,
  onPreview,
  onOpenInternal,
  onOpenExternal,
  onCloseAutoFocus,
  onRefresh,
  onNewFile,
  onNewFolder,
  onNewSymlink,
  onUpload,
  onUploadFolder,
  onDownload,
  onSendToPeer,
  onSendToTarget,
  onRename,
  onMove,
  onDelete,
  onAddToFavorites,
  onCopyPath,
  onSendToTerminal,
  onProperties,
  onAIAction,
}: FileExplorerEntryContextMenuProps) {
  const { t } = useTranslation();
  const actionRows = useMemo(() => {
    if (!target || target.isRoot) return [];
    return rowsContain(selectedTargets, target) ? selectedTargets : [target];
  }, [selectedTargets, target]);
  const targetDirectory = target?.entry.is_dir ? target.path : target?.parentPath;
  const aiActions = target ? getAiActions(target) : [];
  const isFile = !!target && !target.entry.is_dir;
  const showOpenInternal = isFile && editorType === "external";
  const showOpenExternal = isFile && editorType === "internal";

  return (
    <ContextMenuContent
      className="min-w-[200px]"
      onCloseAutoFocus={onCloseAutoFocus}
    >
      {target ? (
        <>
          <ContextMenuItem
            onClick={() =>
              target.entry.is_dir ? onOpenDirectory(target) : onOpenDefault(target)
            }
          >
            <MdFileOpen className="text-[0.875rem] text-muted-foreground mr-2" />
            {t("fileExplorer.cmOpen")}
          </ContextMenuItem>
          {isFile && (
            <ContextMenuItem onClick={() => onPreview(target)}>
              <MdVisibility className="text-[0.875rem] text-muted-foreground mr-2" />
              {t("filePreview.preview")}
            </ContextMenuItem>
          )}
          {showOpenInternal && (
            <ContextMenuItem onClick={() => onOpenInternal(target)}>
              <MdEdit className="text-[0.875rem] text-muted-foreground mr-2" />
              {t("fileExplorer.cmOpenInternalEditor")}
            </ContextMenuItem>
          )}
          {showOpenExternal && (
            <ContextMenuItem onClick={() => onOpenExternal(target)}>
              <MdOpenInNew className="text-[0.875rem] text-muted-foreground mr-2" />
              {t("fileExplorer.cmOpenExternalEditor")}
            </ContextMenuItem>
          )}

          <ContextMenuSeparator />
          <ContextMenuItem onClick={() => onRefresh(target)}>
            <MdRefresh className="text-[0.875rem] text-muted-foreground mr-2" />
            {t("fileExplorer.cmRefresh")}
          </ContextMenuItem>

          {targetDirectory && (
            <>
              {onNewFile && (
                <ContextMenuItem onClick={() => onNewFile(targetDirectory)}>
                  <MdNoteAdd className="text-[0.875rem] text-muted-foreground mr-2" />
                  {t("fileExplorer.newFile")}
                </ContextMenuItem>
              )}
              {onNewFolder && (
                <ContextMenuItem onClick={() => onNewFolder(targetDirectory)}>
                  <MdCreateNewFolder className="text-[0.875rem] text-muted-foreground mr-2" />
                  {t("fileExplorer.newFolder")}
                </ContextMenuItem>
              )}
              {showTransferActions && actionRows.length > 0 && (
                <>
                  <ContextMenuSub>
                    <ContextMenuSubTrigger>
                      <MdUpload className="text-[0.875rem] text-muted-foreground mr-2" />
                      {t("fileExplorer.cmUpload")}
                    </ContextMenuSubTrigger>
                    <ContextMenuSubContent className="w-48">
                      <ContextMenuItem onClick={() => onUpload(targetDirectory)}>
                        <MdUpload className="text-[0.875rem] text-muted-foreground mr-2" />
                        {t("fileExplorer.upload")}
                      </ContextMenuItem>
                      <ContextMenuItem onClick={() => onUploadFolder(targetDirectory)}>
                        <MdDriveFolderUpload className="text-[0.875rem] text-muted-foreground mr-2" />
                        {t("fileExplorer.uploadFolder")}
                      </ContextMenuItem>
                    </ContextMenuSubContent>
                  </ContextMenuSub>
                  <ContextMenuItem onClick={() => onDownload(actionRows)}>
                    <MdDownload className="text-[0.875rem] text-muted-foreground mr-2" />
                    {t("fileExplorer.cmDownload")}
                  </ContextMenuItem>
                  <ContextMenuSeparator />
                </>
              )}
              {onNewSymlink && !target.isRoot && (
                <ContextMenuItem onClick={() => onNewSymlink(targetDirectory)}>
                  <MdLink className="text-[0.875rem] text-muted-foreground mr-2" />
                  {t("fileExplorer.newSymlink")}
                </ContextMenuItem>
              )}
            </>
          )}

          {onSendToPeer && actionRows.length > 0 && (
            <ContextMenuItem onClick={() => onSendToPeer(actionRows)}>
              <MdSend className="text-[0.875rem] text-muted-foreground mr-2" />
              {t("fileExplorer.sendToPeer")}
            </ContextMenuItem>
          )}

          {sendTargetOptions.length > 0 && actionRows.length > 0 && onSendToTarget && (
            <>
              <ContextMenuSub>
                <ContextMenuSubTrigger>
                  <MdSend className="text-[0.875rem] text-muted-foreground mr-2" />
                  {t("fileExplorer.sendToPeer")}
                </ContextMenuSubTrigger>
                <ContextMenuSubContent className="min-w-52">
                  {sendTargetOptions.map((sendTarget) => (
                    <ContextMenuItem
                      key={sendTarget.sessionId}
                      onClick={() => onSendToTarget(actionRows, sendTarget.sessionId)}
                    >
                      <span className="min-w-0 flex-1 truncate">{sendTarget.label}</span>
                      <span className="ml-2 shrink-0 text-[0.625rem] text-muted-foreground">
                        {sendTarget.meta}
                      </span>
                    </ContextMenuItem>
                  ))}
                </ContextMenuSubContent>
              </ContextMenuSub>
              <ContextMenuSeparator />
            </>
          )}

          {!target.isRoot && (
            <>
              <ContextMenuItem
                onClick={() => {
                  if (activeSessionId) onRename(target);
                }}
              >
                <MdEdit className="text-[0.875rem] text-muted-foreground mr-2" />
                {t("fileExplorer.cmRename")}
              </ContextMenuItem>
              <ContextMenuItem
                onClick={() => {
                  if (activeSessionId) onMove(actionRows);
                }}
              >
                <MdDriveFileMove className="text-[0.875rem] text-muted-foreground mr-2" />
                {t("fileExplorer.cmMove")}
              </ContextMenuItem>
              <ContextMenuItem
                variant="destructive"
                onClick={() => onDelete(actionRows)}
              >
                <MdDelete className="text-[0.875rem] mr-2" />
                {t("fileExplorer.cmDelete")}
              </ContextMenuItem>
              <ContextMenuSeparator />
            </>
          )}

          {target.entry.is_dir && !target.isRoot && (
            <>
              <ContextMenuItem onClick={() => onAddToFavorites(target)}>
                <MdBookmarkAdd className="text-[0.875rem] text-muted-foreground mr-2" />
                {t("fileExplorer.addToFavorites")}
              </ContextMenuItem>
              <ContextMenuSeparator />
            </>
          )}

          <ContextMenuItem onClick={() => onCopyPath(target, "full")}>
            <MdContentCopy className="text-[0.875rem] text-muted-foreground mr-2" />
            {t("fileExplorer.cmCopyPath")}
          </ContextMenuItem>
          <ContextMenuItem onClick={() => onCopyPath(target, "name")}>
            <MdCopyAll className="text-[0.875rem] text-muted-foreground mr-2" />
            {t("fileExplorer.cmCopyName")}
          </ContextMenuItem>
          <ContextMenuItem onClick={() => onCopyPath(target, "dir")}>
            <MdFolderCopy className="text-[0.875rem] text-muted-foreground mr-2" />
            {t("fileExplorer.cmCopyDirPath")}
          </ContextMenuItem>

          {terminalInputEnabled && onSendToTerminal && (
            <>
              <ContextMenuSeparator />
              <ContextMenuItem onClick={() => onSendToTerminal(target, "full")}>
                <MdKeyboardReturn className="text-[0.875rem] text-muted-foreground mr-2" />
                {t("fileExplorer.cmTerminalPath")}
              </ContextMenuItem>
              <ContextMenuItem onClick={() => onSendToTerminal(target, "name")}>
                <MdKeyboardArrowRight className="text-[0.875rem] text-muted-foreground mr-2" />
                {t("fileExplorer.cmTerminalName")}
              </ContextMenuItem>
              <ContextMenuItem onClick={() => onSendToTerminal(target, "dir")}>
                <MdKeyboardDoubleArrowRight className="text-[0.875rem] text-muted-foreground mr-2" />
                {t("fileExplorer.cmTerminalDirPath")}
              </ContextMenuItem>
            </>
          )}

          {aiActions.length > 0 && (
            <>
              <ContextMenuSeparator />
              <ContextMenuSub>
                <ContextMenuSubTrigger>
                  <MdAutoAwesome className="text-[0.875rem] text-muted-foreground mr-2" />
                  AI
                </ContextMenuSubTrigger>
                <ContextMenuSubContent>
                  {aiActions.map((action) => (
                    <ContextMenuItem
                      key={action.id}
                      onClick={() => onAIAction(target, action)}
                    >
                      {action.name}
                    </ContextMenuItem>
                  ))}
                </ContextMenuSubContent>
              </ContextMenuSub>
            </>
          )}

          <ContextMenuSeparator />
          <ContextMenuItem onClick={() => onProperties(target)}>
            <MdInfo className="text-[0.875rem] text-muted-foreground mr-2" />
            {t("fileExplorer.cmProperties")}
          </ContextMenuItem>
        </>
      ) : (
        <ContextMenuItem onClick={() => onRefresh(null)}>
          <MdRefresh className="mr-2 h-4 w-4" />
          {t("fileExplorer.cmRefresh")}
        </ContextMenuItem>
      )}
    </ContextMenuContent>
  );
}
