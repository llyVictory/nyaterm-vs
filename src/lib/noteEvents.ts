import { listen } from "@/lib/backend/api";
import type { NotesChangedEvent } from "@/types/notes";

export const NOTES_CHANGED_EVENT = "notes-changed";

export function listenNotesChanged(handler: (event: NotesChangedEvent) => void) {
  return listen<NotesChangedEvent>(NOTES_CHANGED_EVENT, ({ payload }) => handler(payload));
}
