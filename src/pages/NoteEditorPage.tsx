import { getCurrentWindow } from "@/lib/backend/platform/window";
import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { MdNote } from "react-icons/md";
import ChildWindowHeader from "@/components/layout/ChildWindowHeader";
import NoteEditor, { type NoteEditorHandle } from "@/components/note-editor/NoteEditor";

export default function NoteEditorPage() {
  const { t } = useTranslation();
  const editorRef = useRef<NoteEditorHandle | null>(null);
  const [title, setTitle] = useState("");
  const noteId = new URLSearchParams(window.location.search).get("noteId");
  if (!noteId) return null;

  const closeWindow = () => {
    getCurrentWindow()
      .close()
      .catch(() => {});
  };

  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden bg-background text-foreground">
      <ChildWindowHeader
        title={title.trim() || t("notes.title")}
        icon={<MdNote className="text-base" />}
        alwaysOnTopControl
        windowControls
        onClose={() => {
          if (editorRef.current) {
            editorRef.current.requestClose();
            return;
          }
          closeWindow();
        }}
      />
      <NoteEditor ref={editorRef} noteId={noteId} onTitleChange={setTitle} />
    </div>
  );
}
