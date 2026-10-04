import type { TransferSettings } from "@/types/global";

export type InternalEditorDisplay = TransferSettings["internal_editor_display"];
export type FileEditorOpenTarget =
  | "external"
  | "internal-workspace"
  | "internal-window";

type EditorOpenSettings = Pick<TransferSettings, "editor_type"> &
  Partial<Pick<TransferSettings, "internal_editor_display">>;

export function resolveInternalEditorDisplay(
  value?: TransferSettings["internal_editor_display"] | string,
): InternalEditorDisplay {
  return value === "window" ? "window" : "workspace";
}

export function resolveFileEditorOpenTarget(
  settings: EditorOpenSettings,
  nativeFiles = true,
): FileEditorOpenTarget {
  if (nativeFiles && (settings.editor_type || "external") !== "internal") {
    return "external";
  }

  return resolveInternalEditorDisplay(settings.internal_editor_display) ===
    "window"
    ? "internal-window"
    : "internal-workspace";
}
