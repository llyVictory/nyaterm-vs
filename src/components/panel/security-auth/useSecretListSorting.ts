import { type Dispatch, type DragEvent, type SetStateAction, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { invoke } from "@/lib/invoke";

type SortableEntry = { id: string; sort_order: number };
type DropTarget = { id: string; position: "before" | "after" };

function dropPosition(event: DragEvent<HTMLElement>): DropTarget["position"] {
  const rect = event.currentTarget.getBoundingClientRect();
  return event.clientY < rect.top + rect.height / 2 ? "before" : "after";
}

export function useSecretListSorting<T extends SortableEntry>(
  entries: T[],
  setEntries: Dispatch<SetStateAction<T[]>>,
  reload: () => Promise<void>,
  editing: boolean,
  command: "reorder_passwords" | "reorder_ssh_keys",
  failureMessage: string,
) {
  const { t } = useTranslation();
  const sourceRef = useRef<string | null>(null);
  const savingRef = useRef(false);
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [target, setTarget] = useState<DropTarget | null>(null);
  const [reordering, setReordering] = useState(false);
  const actionsDisabled = editing || reordering;

  const resetDrag = () => {
    sourceRef.current = null;
    setDraggingId(null);
    setTarget(null);
  };

  const onDrop = async (event: DragEvent<HTMLDivElement>, targetId: string) => {
    event.preventDefault();
    const sourceId = sourceRef.current;
    const position = dropPosition(event);
    resetDrag();
    if (!sourceId || sourceId === targetId || editing || savingRef.current) return;

    const source = entries.find((entry) => entry.id === sourceId);
    const next = entries.filter((entry) => entry.id !== sourceId);
    const targetIndex = next.findIndex((entry) => entry.id === targetId);
    if (!source || targetIndex < 0) return;
    next.splice(targetIndex + (position === "after" ? 1 : 0), 0, source);
    if (next.every((entry, index) => entry.id === entries[index].id)) return;

    const reordered = next.map((entry, index) => ({
      ...entry,
      sort_order: index,
    }));
    savingRef.current = true;
    setReordering(true);
    setEntries(reordered);
    try {
      await invoke(command, {
        updates: reordered.map(({ id, sort_order }) => ({ id, sort_order })),
      });
    } catch {
      setEntries(entries);
      toast.error(t(failureMessage));
      await reload();
    } finally {
      savingRef.current = false;
      setReordering(false);
    }
  };

  return {
    actionsDisabled,
    reordering,
    handleProps: (id: string) => ({
      draggable: !actionsDisabled && entries.length > 1,
      disabled: actionsDisabled || entries.length < 2,
      onDragStart: (event: DragEvent<HTMLButtonElement>) => {
        if (editing || savingRef.current || entries.length < 2) {
          event.preventDefault();
          return;
        }
        sourceRef.current = id;
        setDraggingId(id);
        event.dataTransfer.effectAllowed = "move";
        event.dataTransfer.setData("text/plain", id);
      },
      onDragEnd: resetDrag,
    }),
    rowProps: (id: string) => ({
      style: {
        opacity: draggingId === id ? 0.5 : undefined,
        boxShadow:
          target?.id === id
            ? `inset 0 ${target.position === "before" ? "2px" : "-2px"} 0 var(--df-primary)`
            : undefined,
      },
      onDragOver: (event: DragEvent<HTMLDivElement>) => {
        if (!sourceRef.current || sourceRef.current === id || editing || savingRef.current) return;
        event.preventDefault();
        event.dataTransfer.dropEffect = "move";
        setTarget({ id, position: dropPosition(event) });
      },
      onDrop: (event: DragEvent<HTMLDivElement>) => {
        void onDrop(event, id);
      },
    }),
  };
}
