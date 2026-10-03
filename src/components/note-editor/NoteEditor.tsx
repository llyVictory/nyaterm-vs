import "@mdxeditor/editor/style.css";

import { MDXEditor, type MDXEditorMethods } from "@mdxeditor/editor";
import { getCurrentWindow } from "@/lib/backend/platform/window";
import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { MdClose, MdNote, MdRefresh, MdSave } from "react-icons/md";
import { toast } from "sonner";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { getErrorMessage } from "@/lib/errors";
import { invoke } from "@/lib/invoke";
import { logger } from "@/lib/logger";
import { listenNotesChanged } from "@/lib/noteEvents";
import type { NoteDocument } from "@/types/notes";
import NoteEditorToolbarStatus, { type NoteSaveStatus } from "./NoteEditorToolbarStatus";
import { noteEditorPlugins } from "./noteEditorPlugins";

const AUTOSAVE_DEBOUNCE_MS = 800;

interface NoteEditorProps {
  noteId: string;
  onTitleChange?: (title: string) => void;
}

export interface NoteEditorHandle {
  requestClose: () => void;
}

const NoteEditor = forwardRef<NoteEditorHandle, NoteEditorProps>(function NoteEditor(
  { noteId, onTitleChange },
  ref,
) {
  const { t } = useTranslation();
  const editorRef = useRef<MDXEditorMethods>(null);
  const latestMarkdownRef = useRef("");
  const latestTitleRef = useRef("");
  const revisionRef = useRef(0);
  const parentIdRef = useRef<string | null>(null);
  const dirtyRef = useRef(false);
  const savingPromiseRef = useRef<Promise<boolean> | null>(null);
  const performSaveRef = useRef<(force?: boolean) => Promise<boolean>>(async () => false);
  const debounceRef = useRef<number | null>(null);
  const forceCloseRef = useRef(false);
  const suppressChangeRef = useRef(false);
  const deletedRef = useRef(false);
  const [note, setNote] = useState<NoteDocument | null>(null);
  const [title, setTitle] = useState("");
  const [status, setStatus] = useState<NoteSaveStatus>("saved");
  const [error, setError] = useState("");
  const [conflictOpen, setConflictOpen] = useState(false);
  const [closeFailureOpen, setCloseFailureOpen] = useState(false);

  const plugins = useMemo(() => noteEditorPlugins(), []);
  const statusLabels = {
    saved: t("notes.saved"),
    saving: t("notes.saving"),
    unsaved: t("notes.unsaved"),
    failed: t("notes.saveFailed"),
    external: t("notes.externalUpdate"),
    deleted: t("notes.deletedStatus"),
  };

    const loadNote = useCallback(
      async (applyToEditor = true) => {
        const next = await invoke<NoteDocument>("get_note", { noteId });
        latestTitleRef.current = next.title;
        latestMarkdownRef.current = next.markdown;
        revisionRef.current = next.revision;
        parentIdRef.current = next.parent_id ?? null;
        dirtyRef.current = false;
        deletedRef.current = false;
        setNote(next);
        setTitle(next.title);
        onTitleChange?.(next.title);
        setStatus("saved");
        setError("");
        if (applyToEditor) {
          suppressChangeRef.current = true;
          editorRef.current?.setMarkdown(next.markdown);
          window.setTimeout(() => {
            suppressChangeRef.current = false;
          }, 0);
        }
        document.title = `${next.title} - ${t("notes.title")} - NyaTerm`;
        getCurrentWindow()
          .setTitle(`${next.title} - ${t("notes.title")} - NyaTerm`)
          .catch(() => {});
      },
      [noteId, onTitleChange, t],
    );

    useEffect(() => {
      loadNote(false).catch((err) => {
        setError(getErrorMessage(err));
        setStatus("failed");
      });
    }, [loadNote]);

    const markDirty = useCallback(() => {
      if (deletedRef.current) return;
      dirtyRef.current = true;
      setStatus("unsaved");
    }, []);

    const performSave = useCallback(
      async (force = false) => {
        if (deletedRef.current) return false;
        if (!dirtyRef.current && !force) return true;
        if (savingPromiseRef.current) {
          await savingPromiseRef.current;
          if (!dirtyRef.current && !force) return true;
          return performSaveRef.current(force);
        }

        const markdown = latestMarkdownRef.current;
        const nextTitle = latestTitleRef.current.trim() || t("notes.untitled");
        const expectedRevision = revisionRef.current;
        dirtyRef.current = false;
        setStatus("saving");
        const promise = invoke<NoteDocument>("update_note", {
          noteId,
          title: nextTitle,
          markdown,
          expectedRevision,
          force,
        })
          .then((saved) => {
            revisionRef.current = saved.revision;
            parentIdRef.current = saved.parent_id ?? null;
            if (
              latestMarkdownRef.current === markdown &&
              latestTitleRef.current.trim() === nextTitle
            ) {
              setNote(saved);
              setTitle(saved.title);
              latestTitleRef.current = saved.title;
              latestMarkdownRef.current = saved.markdown;
              onTitleChange?.(saved.title);
              setStatus("saved");
            } else {
              setNote((current) =>
                current
                  ? {
                      ...saved,
                      title: latestTitleRef.current,
                      markdown: latestMarkdownRef.current,
                    }
                  : saved,
              );
              dirtyRef.current = true;
              setStatus("unsaved");
              window.setTimeout(() => {
                void performSaveRef.current();
              }, 0);
            }
            setError("");
            return true;
          })
          .catch((err) => {
            dirtyRef.current = true;
            const message = getErrorMessage(err);
            setError(message);
            if (message.toLowerCase().includes("revision conflict")) {
              setConflictOpen(true);
              setStatus("external");
            } else {
              setStatus("failed");
            }
            logger.error({
              domain: "ui.error",
              event: "note.save_failed",
              message: "Failed to save note",
              error: err,
            });
            return false;
          })
          .finally(() => {
            savingPromiseRef.current = null;
          });
        savingPromiseRef.current = promise;
        return promise;
      },
      [noteId, onTitleChange, t],
    );
    performSaveRef.current = performSave;

    const scheduleSave = useCallback(() => {
      if (debounceRef.current) window.clearTimeout(debounceRef.current);
      debounceRef.current = window.setTimeout(() => {
        void performSave();
      }, AUTOSAVE_DEBOUNCE_MS);
    }, [performSave]);

    const flushSave = useCallback(
      async (force = false) => {
        if (debounceRef.current) {
          window.clearTimeout(debounceRef.current);
          debounceRef.current = null;
        }
        return performSave(force);
      },
      [performSave],
    );

    const handleMarkdownChange = (markdown: string) => {
      if (suppressChangeRef.current) return;
      latestMarkdownRef.current = markdown;
      markDirty();
      scheduleSave();
    };

    const handleTitleChange = (value: string) => {
      setTitle(value);
      onTitleChange?.(value);
      latestTitleRef.current = value;
      markDirty();
      scheduleSave();
    };

    const closeWindow = useCallback(() => {
      forceCloseRef.current = true;
      getCurrentWindow()
        .close()
        .catch(() => {});
    }, []);

    const saveAndMaybeClose = useCallback(async () => {
      const saved = await flushSave();
      if (saved) {
        closeWindow();
        return;
      }
      setCloseFailureOpen(true);
    }, [closeWindow, flushSave]);

    useImperativeHandle(
      ref,
      () => ({
        requestClose: () => {
          void saveAndMaybeClose();
        },
      }),
      [saveAndMaybeClose],
    );

    useEffect(() => {
      const currentWindow = getCurrentWindow();
      let unlisten: (() => void) | undefined;
      currentWindow
        .onCloseRequested((event) => {
          if (
            forceCloseRef.current ||
            (!dirtyRef.current && !savingPromiseRef.current)
          )
            return;
          event.preventDefault();
          void saveAndMaybeClose();
        })
        .then((dispose) => {
          unlisten = dispose;
        })
        .catch(() => {});
      return () => {
        unlisten?.();
      };
    }, [saveAndMaybeClose]);

    useEffect(() => {
      const handleKeyDown = (event: KeyboardEvent) => {
        if (
          (event.ctrlKey || event.metaKey) &&
          event.key.toLowerCase() === "s"
        ) {
          event.preventDefault();
          void flushSave();
        }
      };
      window.addEventListener("keydown", handleKeyDown);
      return () => window.removeEventListener("keydown", handleKeyDown);
    }, [flushSave]);

    useEffect(() => {
      let unlisten: (() => void) | undefined;
      listenNotesChanged((event) => {
        if (!event.ids.includes(noteId) && event.kind !== "replaced") return;
        if (event.kind === "deleted") {
          deletedRef.current = true;
          dirtyRef.current = false;
          setStatus("deleted");
          return;
        }
        if (event.kind === "updated" && savingPromiseRef.current) return;
        if (
          event.kind === "updated" &&
          revisionRef.current &&
          !dirtyRef.current
        ) {
          void loadNote(true);
          return;
        }
        if (dirtyRef.current) {
          setStatus("external");
          setConflictOpen(true);
        } else {
          void loadNote(true);
        }
      })
        .then((dispose) => {
          unlisten = dispose;
        })
        .catch(() => {});
      return () => {
        unlisten?.();
      };
    }, [loadNote, noteId]);

    const saveCopy = async () => {
      try {
        const created = await invoke<NoteDocument>("create_note", {
          parentId: parentIdRef.current,
          title: `${latestTitleRef.current || t("notes.untitled")} ${t("notes.conflictCopySuffix")}`,
          markdown: latestMarkdownRef.current,
        });
        dirtyRef.current = false;
        setConflictOpen(false);
        toast.success(t("notes.copySaved"));
        closeWindow();
        await import("@/lib/windowManager").then(({ openNoteEditor }) =>
          openNoteEditor(created.id, created.title),
        );
      } catch (err) {
        toast.error(getErrorMessage(err));
      }
    };

    if (deletedRef.current) {
      return (
        <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-3 p-6 text-center">
          <MdNote className="text-3xl text-muted-foreground" />
          <div className="text-base font-medium">{t("notes.deletedTitle")}</div>
          <div className="max-w-md text-sm text-muted-foreground">
            {t("notes.deletedDescription")}
          </div>
          <div className="flex gap-2">
            <Button variant="outline" onClick={closeWindow}>
              <MdClose />
              {t("common.close")}
            </Button>
            <Button onClick={() => void saveCopy()}>
              {t("notes.saveCopy")}
            </Button>
          </div>
        </div>
      );
    }

    return (
      <div className="relative flex min-h-0 flex-1 flex-col overflow-hidden bg-background text-foreground">
        <div className="flex min-h-0 shrink-0 items-center gap-3 border-b bg-muted/15 px-3 py-2">
          <input
            value={title}
            onChange={(event) => handleTitleChange(event.target.value)}
            onBlur={() => void flushSave()}
            className="min-w-0 flex-1 bg-transparent text-base font-medium outline-none"
            placeholder={t("notes.untitled")}
          />
          <NoteEditorToolbarStatus status={status} labels={statusLabels} />
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={() => void loadNote(true)}
          >
            <MdRefresh />
          </Button>
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={() => void flushSave()}
          >
            <MdSave />
          </Button>
        </div>
        {error ? (
          <div className="border-b border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive">
            {error}
          </div>
        ) : null}
        {!note ? (
          <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
            {t("common.loading")}
          </div>
        ) : (
          <div
            className="nyaterm-note-editor-shell min-h-0 flex-1 overflow-auto"
            onBlur={() => void flushSave()}
          >
            <MDXEditor
              ref={editorRef}
              className="nyaterm-note-mdxeditor"
              markdown={note.markdown}
              onChange={handleMarkdownChange}
              plugins={plugins}
              contentEditableClassName="nyaterm-note-content"
            />
          </div>
        )}
        <div className="pointer-events-none absolute bottom-3 right-4">
          <NoteEditorToolbarStatus status={status} labels={statusLabels} />
        </div>

        <AlertDialog open={conflictOpen} onOpenChange={setConflictOpen}>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>{t("notes.revisionConflict")}</AlertDialogTitle>
              <AlertDialogDescription>
                {t("notes.revisionConflictDescription")}
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
              <AlertDialogAction
                variant="outline"
                onClick={() => void loadNote(true)}
              >
                {t("notes.reload")}
              </AlertDialogAction>
              <AlertDialogAction
                variant="outline"
                onClick={() => void saveCopy()}
              >
                {t("notes.saveCopy")}
              </AlertDialogAction>
              <AlertDialogAction onClick={() => void flushSave(true)}>
                {t("notes.overwrite")}
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>

        <AlertDialog open={closeFailureOpen} onOpenChange={setCloseFailureOpen}>
          <AlertDialogContent size="sm">
            <AlertDialogHeader>
              <AlertDialogTitle>
                {t("notes.closeBlockedTitle")}
              </AlertDialogTitle>
              <AlertDialogDescription>
                {t("notes.closeBlockedDescription")}
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
              <AlertDialogAction
                variant="outline"
                onClick={() => void saveAndMaybeClose()}
              >
                {t("notes.retry")}
              </AlertDialogAction>
              <AlertDialogAction variant="destructive" onClick={closeWindow}>
                {t("notes.discardAndClose")}
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </div>
    );
  },
);

export default NoteEditor;
