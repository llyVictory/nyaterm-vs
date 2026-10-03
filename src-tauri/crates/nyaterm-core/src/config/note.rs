use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteFolder {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub sort_order: i64,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteDocument {
    pub id: String,
    pub parent_id: Option<String>,
    pub title: String,
    pub markdown: String,
    pub sort_order: i64,
    pub revision: u64,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct NotesSnapshot {
    #[serde(default)]
    pub folders: Vec<NoteFolder>,
    #[serde(default)]
    pub notes: Vec<NoteDocument>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteSummary {
    pub id: String,
    pub parent_id: Option<String>,
    pub title: String,
    pub sort_order: i64,
    pub revision: u64,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

impl From<NoteDocument> for NoteSummary {
    fn from(note: NoteDocument) -> Self {
        Self {
            id: note.id,
            parent_id: note.parent_id,
            title: note.title,
            sort_order: note.sort_order,
            revision: note.revision,
            created_at_ms: note.created_at_ms,
            updated_at_ms: note.updated_at_ms,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteTreePayload {
    pub folders: Vec<NoteFolder>,
    pub notes: Vec<NoteSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteUpdateResult {
    pub note: NoteDocument,
    pub changed: bool,
    pub tree_changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteNodeChange {
    pub changed: bool,
    pub tree_changed: bool,
    pub folder: Option<NoteFolder>,
    pub note: Option<NoteSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NotesChangedEvent {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_kind: Option<String>,
    pub ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub folders: Vec<NoteFolder>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<NoteSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tree_changed: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeleteNoteNodeResult {
    pub folder_count: usize,
    pub note_count: usize,
    pub ids: Vec<String>,
}
