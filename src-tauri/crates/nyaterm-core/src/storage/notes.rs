use crate::config::{
    DeleteNoteNodeResult, NoteDocument, NoteFolder, NoteNodeChange, NoteSummary, NoteUpdateResult,
    NotesSnapshot,
};
use crate::error::{AppError, AppResult};
use redb::{ReadableDatabase, ReadableTable};
use std::collections::{HashMap, HashSet};

use super::Storage;
use super::tables::*;
use super::util::*;

const DEFAULT_NOTE_TITLE: &str = "新建笔记";
const DEFAULT_FOLDER_NAME: &str = "新建文件夹";
const MAX_NOTE_NAME_CHARS: usize = 120;
const NOTE_SUMMARY_INDEX_VERSION: u32 = 1;

trait NoteListItem {
    fn id(&self) -> &str;
    fn parent_id(&self) -> Option<&str>;
    fn title(&self) -> &str;
    fn sort_order(&self) -> i64;
}

impl NoteListItem for NoteDocument {
    fn id(&self) -> &str {
        &self.id
    }

    fn parent_id(&self) -> Option<&str> {
        self.parent_id.as_deref()
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn sort_order(&self) -> i64 {
        self.sort_order
    }
}

impl NoteListItem for NoteSummary {
    fn id(&self) -> &str {
        &self.id
    }

    fn parent_id(&self) -> Option<&str> {
        self.parent_id.as_deref()
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn sort_order(&self) -> i64 {
        self.sort_order
    }
}

impl Storage {
    pub fn list_note_folders(&self) -> AppResult<Vec<NoteFolder>> {
        let mut folders = self.list_json_by_prefix(NOTE_FOLDERS_TABLE, NOTE_FOLDER_PREFIX)?;
        sort_note_folders(&mut folders);
        Ok(folders)
    }

    pub fn list_notes(&self) -> AppResult<Vec<NoteDocument>> {
        let mut notes = self.list_json_by_prefix(NOTES_TABLE, NOTE_DOCUMENT_PREFIX)?;
        sort_notes(&mut notes);
        Ok(notes)
    }

    pub fn list_note_summaries(&self) -> AppResult<Vec<NoteSummary>> {
        self.ensure_note_summary_index()?;
        let mut notes = self.list_json_by_prefix(NOTE_SUMMARIES_TABLE, NOTE_SUMMARY_PREFIX)?;
        sort_note_summaries(&mut notes);
        Ok(notes)
    }

    pub fn get_note(&self, note_id: &str) -> AppResult<Option<NoteDocument>> {
        self.read_json(NOTES_TABLE, &entity_key(NOTE_DOCUMENT_PREFIX, note_id))
    }

    pub fn create_note_folder(
        &self,
        parent_id: Option<String>,
        name: Option<String>,
    ) -> AppResult<NoteFolder> {
        self.ensure_note_summary_index()?;
        let txn = self.db.begin_write().map_err(storage_error)?;
        let mut folders = read_note_folders_in_txn(&txn)?;
        let notes = read_note_summaries_in_txn(&txn)?;
        validate_parent_exists(&folders, parent_id.as_deref())?;
        let sibling_names = sibling_names(&folders, &notes, parent_id.as_deref(), None);
        let name = normalize_or_unique_name(name, DEFAULT_FOLDER_NAME, &sibling_names)?;
        let sort_order = next_sort_order_for_parent(&folders, &notes, parent_id.as_deref());
        let now = current_time_ms();
        let folder = NoteFolder {
            id: uuid::Uuid::new_v4().to_string(),
            parent_id,
            name,
            sort_order,
            created_at_ms: now,
            updated_at_ms: now,
        };
        write_note_folder_in_txn(&txn, &folder)?;
        txn.commit().map_err(storage_error)?;
        folders.push(folder.clone());
        Ok(folder)
    }

    pub fn create_note(
        &self,
        parent_id: Option<String>,
        title: Option<String>,
        markdown: Option<String>,
    ) -> AppResult<NoteDocument> {
        self.ensure_note_summary_index()?;
        let txn = self.db.begin_write().map_err(storage_error)?;
        let folders = read_note_folders_in_txn(&txn)?;
        let mut notes = read_note_summaries_in_txn(&txn)?;
        validate_parent_exists(&folders, parent_id.as_deref())?;
        let sibling_names = sibling_names(&folders, &notes, parent_id.as_deref(), None);
        let title = normalize_or_unique_name(title, DEFAULT_NOTE_TITLE, &sibling_names)?;
        let sort_order = next_sort_order_for_parent(&folders, &notes, parent_id.as_deref());
        let now = current_time_ms();
        let note = NoteDocument {
            id: uuid::Uuid::new_v4().to_string(),
            parent_id,
            title,
            markdown: markdown.unwrap_or_default(),
            sort_order,
            revision: 1,
            created_at_ms: now,
            updated_at_ms: now,
        };
        write_note_in_txn(&txn, &note)?;
        write_note_summary_in_txn(&txn, &NoteSummary::from(note.clone()))?;
        txn.commit().map_err(storage_error)?;
        notes.push(NoteSummary::from(note.clone()));
        Ok(note)
    }

    pub fn update_note(
        &self,
        note_id: &str,
        title: String,
        markdown: String,
        expected_revision: u64,
        force: bool,
    ) -> AppResult<NoteUpdateResult> {
        self.ensure_note_summary_index()?;
        let txn = self.db.begin_write().map_err(storage_error)?;
        let folders = read_note_folders_in_txn(&txn)?;
        let notes = read_note_summaries_in_txn(&txn)?;
        let mut note = read_note_in_txn(&txn, note_id)?
            .ok_or_else(|| AppError::Config(format!("Note '{note_id}' does not exist")))?;
        if !force && note.revision != expected_revision {
            return Err(AppError::Config(format!(
                "Revision conflict: expected {}, found {}",
                expected_revision, note.revision
            )));
        }

        let title = normalize_note_name(&title)?;
        validate_unique_sibling_name(
            &folders,
            &notes,
            note.parent_id.as_deref(),
            &title,
            Some(("note", note_id)),
        )?;

        let changed = note.title != title || note.markdown != markdown;
        let tree_changed = note.title != title;
        if changed {
            note.title = title;
            note.markdown = markdown;
            note.revision = note.revision.saturating_add(1);
            note.updated_at_ms = current_time_ms();
            write_note_in_txn(&txn, &note)?;
            write_note_summary_in_txn(&txn, &NoteSummary::from(note.clone()))?;
        }
        txn.commit().map_err(storage_error)?;
        Ok(NoteUpdateResult {
            note,
            changed,
            tree_changed,
        })
    }

    pub fn rename_note_node(
        &self,
        node_kind: &str,
        node_id: &str,
        name: String,
    ) -> AppResult<NoteNodeChange> {
        self.ensure_note_summary_index()?;
        let txn = self.db.begin_write().map_err(storage_error)?;
        let folders = read_note_folders_in_txn(&txn)?;
        let notes = read_note_summaries_in_txn(&txn)?;
        let name = normalize_note_name(&name)?;
        let change = match node_kind {
            "folder" => {
                let Some(mut folder) = folders.iter().find(|item| item.id == node_id).cloned()
                else {
                    return Err(AppError::Config(format!(
                        "Folder '{node_id}' does not exist"
                    )));
                };
                validate_unique_sibling_name(
                    &folders,
                    &notes,
                    folder.parent_id.as_deref(),
                    &name,
                    Some(("folder", node_id)),
                )?;
                let changed = folder.name != name;
                if changed {
                    folder.name = name;
                    folder.updated_at_ms = current_time_ms();
                    write_note_folder_in_txn(&txn, &folder)?;
                }
                NoteNodeChange {
                    changed,
                    tree_changed: changed,
                    folder: Some(folder),
                    note: None,
                }
            }
            "note" => {
                let summary = notes
                    .iter()
                    .find(|item| item.id == node_id)
                    .ok_or_else(|| AppError::Config(format!("Note '{node_id}' does not exist")))?;
                validate_unique_sibling_name(
                    &folders,
                    &notes,
                    summary.parent_id.as_deref(),
                    &name,
                    Some(("note", node_id)),
                )?;
                let mut note = read_note_in_txn(&txn, node_id)?
                    .ok_or_else(|| AppError::Config(format!("Note '{node_id}' does not exist")))?;
                let changed = note.title != name;
                if note.title != name {
                    note.title = name;
                    note.revision = note.revision.saturating_add(1);
                    note.updated_at_ms = current_time_ms();
                    write_note_in_txn(&txn, &note)?;
                    write_note_summary_in_txn(&txn, &NoteSummary::from(note.clone()))?;
                }
                NoteNodeChange {
                    changed,
                    tree_changed: changed,
                    folder: None,
                    note: Some(NoteSummary::from(note)),
                }
            }
            _ => return Err(AppError::Config("Invalid note node kind".to_string())),
        };
        txn.commit().map_err(storage_error)?;
        Ok(change)
    }

    pub fn move_note_node(
        &self,
        node_kind: &str,
        node_id: &str,
        parent_id: Option<String>,
        sort_order: i64,
    ) -> AppResult<NoteNodeChange> {
        self.ensure_note_summary_index()?;
        let txn = self.db.begin_write().map_err(storage_error)?;
        let folders = read_note_folders_in_txn(&txn)?;
        let notes = read_note_summaries_in_txn(&txn)?;
        validate_parent_exists(&folders, parent_id.as_deref())?;

        let change = match node_kind {
            "folder" => {
                let Some(mut folder) = folders.iter().find(|item| item.id == node_id).cloned()
                else {
                    return Err(AppError::Config(format!(
                        "Folder '{node_id}' does not exist"
                    )));
                };
                if parent_id.as_deref() == Some(node_id) {
                    return Err(AppError::Config(
                        "A folder cannot be moved into itself".to_string(),
                    ));
                }
                if let Some(parent_id) = parent_id.as_deref() {
                    validate_not_descendant_folder(&folders, node_id, parent_id)?;
                }
                validate_unique_sibling_name(
                    &folders,
                    &notes,
                    parent_id.as_deref(),
                    &folder.name,
                    Some(("folder", node_id)),
                )?;
                let changed = folder.parent_id != parent_id || folder.sort_order != sort_order;
                if changed {
                    folder.parent_id = parent_id;
                    folder.sort_order = sort_order;
                    folder.updated_at_ms = current_time_ms();
                    write_note_folder_in_txn(&txn, &folder)?;
                }
                NoteNodeChange {
                    changed,
                    tree_changed: changed,
                    folder: Some(folder),
                    note: None,
                }
            }
            "note" => {
                let summary = notes
                    .iter()
                    .find(|item| item.id == node_id)
                    .ok_or_else(|| AppError::Config(format!("Note '{node_id}' does not exist")))?;
                validate_unique_sibling_name(
                    &folders,
                    &notes,
                    parent_id.as_deref(),
                    &summary.title,
                    Some(("note", node_id)),
                )?;
                let mut note = read_note_in_txn(&txn, node_id)?
                    .ok_or_else(|| AppError::Config(format!("Note '{node_id}' does not exist")))?;
                let changed = note.parent_id != parent_id || note.sort_order != sort_order;
                if changed {
                    note.parent_id = parent_id;
                    note.sort_order = sort_order;
                    note.revision = note.revision.saturating_add(1);
                    note.updated_at_ms = current_time_ms();
                    write_note_in_txn(&txn, &note)?;
                    write_note_summary_in_txn(&txn, &NoteSummary::from(note.clone()))?;
                }
                NoteNodeChange {
                    changed,
                    tree_changed: changed,
                    folder: None,
                    note: Some(NoteSummary::from(note)),
                }
            }
            _ => return Err(AppError::Config("Invalid note node kind".to_string())),
        };
        txn.commit().map_err(storage_error)?;
        Ok(change)
    }

    pub fn delete_note_node(
        &self,
        node_kind: &str,
        node_id: &str,
    ) -> AppResult<DeleteNoteNodeResult> {
        self.ensure_note_summary_index()?;
        let txn = self.db.begin_write().map_err(storage_error)?;
        let folders = read_note_folders_in_txn(&txn)?;
        let notes = read_note_summaries_in_txn(&txn)?;
        let mut folder_ids = HashSet::new();
        let mut note_ids = HashSet::new();

        match node_kind {
            "folder" => {
                if !folders.iter().any(|folder| folder.id == node_id) {
                    return Err(AppError::Config(format!(
                        "Folder '{node_id}' does not exist"
                    )));
                }
                collect_descendant_folder_ids(&folders, node_id, &mut folder_ids);
                folder_ids.insert(node_id.to_string());
                for note in &notes {
                    if note
                        .parent_id
                        .as_ref()
                        .is_some_and(|parent| folder_ids.contains(parent))
                    {
                        note_ids.insert(note.id.clone());
                    }
                }
            }
            "note" => {
                if !notes.iter().any(|note| note.id == node_id) {
                    return Err(AppError::Config(format!("Note '{node_id}' does not exist")));
                }
                note_ids.insert(node_id.to_string());
            }
            _ => return Err(AppError::Config("Invalid note node kind".to_string())),
        }

        {
            let mut folder_table = txn.open_table(NOTE_FOLDERS_TABLE).map_err(storage_error)?;
            for id in &folder_ids {
                folder_table
                    .remove(entity_key(NOTE_FOLDER_PREFIX, id).as_str())
                    .map_err(storage_error)?;
            }
        }
        {
            let mut note_table = txn.open_table(NOTES_TABLE).map_err(storage_error)?;
            for id in &note_ids {
                note_table
                    .remove(entity_key(NOTE_DOCUMENT_PREFIX, id).as_str())
                    .map_err(storage_error)?;
            }
        }
        {
            let mut summary_table = txn
                .open_table(NOTE_SUMMARIES_TABLE)
                .map_err(storage_error)?;
            for id in &note_ids {
                summary_table
                    .remove(entity_key(NOTE_SUMMARY_PREFIX, id).as_str())
                    .map_err(storage_error)?;
            }
        }
        txn.commit().map_err(storage_error)?;
        let folder_count = folder_ids.len();
        let note_count = note_ids.len();
        let mut ids = folder_ids.into_iter().chain(note_ids).collect::<Vec<_>>();
        ids.sort();
        Ok(DeleteNoteNodeResult {
            folder_count,
            note_count,
            ids,
        })
    }

    pub fn load_notes_snapshot(&self) -> AppResult<NotesSnapshot> {
        Ok(NotesSnapshot {
            folders: self.list_note_folders()?,
            notes: self.list_notes()?,
        })
    }

    pub fn replace_notes_snapshot(&self, snapshot: &NotesSnapshot) -> AppResult<()> {
        validate_notes_snapshot(snapshot)?;
        let txn = self.db.begin_write().map_err(storage_error)?;
        clear_prefix_in_txn(&txn, NOTE_FOLDERS_TABLE, NOTE_FOLDER_PREFIX)?;
        clear_prefix_in_txn(&txn, NOTES_TABLE, NOTE_DOCUMENT_PREFIX)?;
        clear_prefix_in_txn(&txn, NOTE_SUMMARIES_TABLE, NOTE_SUMMARY_PREFIX)?;
        for folder in &snapshot.folders {
            write_note_folder_in_txn(&txn, folder)?;
        }
        for note in &snapshot.notes {
            write_note_in_txn(&txn, note)?;
            write_note_summary_in_txn(&txn, &NoteSummary::from(note.clone()))?;
        }
        write_meta_u32(
            &txn,
            META_NOTE_SUMMARY_INDEX_VERSION,
            NOTE_SUMMARY_INDEX_VERSION,
        )?;
        txn.commit().map_err(storage_error)?;
        Ok(())
    }

    fn ensure_note_summary_index(&self) -> AppResult<()> {
        if self.note_summary_index_version()? >= NOTE_SUMMARY_INDEX_VERSION {
            return Ok(());
        }

        let txn = self.db.begin_write().map_err(storage_error)?;
        let current_version =
            read_meta_u32_in_txn(&txn, META_NOTE_SUMMARY_INDEX_VERSION)?.unwrap_or(0);
        open_note_summary_table_in_txn(&txn)?;
        if current_version >= NOTE_SUMMARY_INDEX_VERSION {
            txn.commit().map_err(storage_error)?;
            return Ok(());
        }

        clear_prefix_in_txn(&txn, NOTE_SUMMARIES_TABLE, NOTE_SUMMARY_PREFIX)?;
        for note in read_notes_in_txn(&txn)? {
            write_note_summary_in_txn(&txn, &NoteSummary::from(note))?;
        }
        write_meta_u32(
            &txn,
            META_NOTE_SUMMARY_INDEX_VERSION,
            NOTE_SUMMARY_INDEX_VERSION,
        )?;
        txn.commit().map_err(storage_error)?;
        Ok(())
    }

    fn note_summary_index_version(&self) -> AppResult<u32> {
        let txn = self.db.begin_read().map_err(storage_error)?;
        let table = match txn.open_table(META_TABLE) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(0),
            Err(error) => return Err(storage_error(error)),
        };
        let Some(raw) = table
            .get(META_NOTE_SUMMARY_INDEX_VERSION)
            .map_err(storage_error)?
        else {
            return Ok(0);
        };
        parse_meta_u32(raw.value(), META_NOTE_SUMMARY_INDEX_VERSION)
    }
}

fn read_note_folders_in_txn(txn: &redb::WriteTransaction) -> AppResult<Vec<NoteFolder>> {
    let table = txn.open_table(NOTE_FOLDERS_TABLE).map_err(storage_error)?;
    let mut folders = Vec::new();
    for entry in table.iter().map_err(storage_error)? {
        let (key, value) = entry.map_err(storage_error)?;
        if key.value().starts_with(NOTE_FOLDER_PREFIX) {
            folders.push(deserialize_json::<NoteFolder>(value.value())?);
        }
    }
    sort_note_folders(&mut folders);
    Ok(folders)
}

fn read_notes_in_txn(txn: &redb::WriteTransaction) -> AppResult<Vec<NoteDocument>> {
    let table = txn.open_table(NOTES_TABLE).map_err(storage_error)?;
    let mut notes = Vec::new();
    for entry in table.iter().map_err(storage_error)? {
        let (key, value) = entry.map_err(storage_error)?;
        if key.value().starts_with(NOTE_DOCUMENT_PREFIX) {
            notes.push(deserialize_json::<NoteDocument>(value.value())?);
        }
    }
    sort_notes(&mut notes);
    Ok(notes)
}

fn read_note_summaries_in_txn(txn: &redb::WriteTransaction) -> AppResult<Vec<NoteSummary>> {
    let table = txn
        .open_table(NOTE_SUMMARIES_TABLE)
        .map_err(storage_error)?;
    let mut notes = Vec::new();
    for entry in table.iter().map_err(storage_error)? {
        let (key, value) = entry.map_err(storage_error)?;
        if key.value().starts_with(NOTE_SUMMARY_PREFIX) {
            notes.push(deserialize_json::<NoteSummary>(value.value())?);
        }
    }
    sort_note_summaries(&mut notes);
    Ok(notes)
}

fn read_note_in_txn(
    txn: &redb::WriteTransaction,
    note_id: &str,
) -> AppResult<Option<NoteDocument>> {
    let table = txn.open_table(NOTES_TABLE).map_err(storage_error)?;
    let key = entity_key(NOTE_DOCUMENT_PREFIX, note_id);
    table
        .get(key.as_str())
        .map_err(storage_error)?
        .map(|value| deserialize_json::<NoteDocument>(value.value()))
        .transpose()
}

fn write_note_folder_in_txn(txn: &redb::WriteTransaction, folder: &NoteFolder) -> AppResult<()> {
    write_json_in_txn(
        txn,
        NOTE_FOLDERS_TABLE,
        &entity_key(NOTE_FOLDER_PREFIX, &folder.id),
        folder,
    )
}

fn write_note_in_txn(txn: &redb::WriteTransaction, note: &NoteDocument) -> AppResult<()> {
    write_json_in_txn(
        txn,
        NOTES_TABLE,
        &entity_key(NOTE_DOCUMENT_PREFIX, &note.id),
        note,
    )
}

fn write_note_summary_in_txn(txn: &redb::WriteTransaction, note: &NoteSummary) -> AppResult<()> {
    write_json_in_txn(
        txn,
        NOTE_SUMMARIES_TABLE,
        &entity_key(NOTE_SUMMARY_PREFIX, &note.id),
        note,
    )
}

fn open_note_summary_table_in_txn(txn: &redb::WriteTransaction) -> AppResult<()> {
    txn.open_table(NOTE_SUMMARIES_TABLE)
        .map_err(storage_error)?;
    Ok(())
}

fn normalize_note_name(raw: &str) -> AppResult<String> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(AppError::Config("Note name cannot be empty".to_string()));
    }
    if value.chars().count() > MAX_NOTE_NAME_CHARS {
        return Err(AppError::Config(format!(
            "Note name cannot exceed {MAX_NOTE_NAME_CHARS} characters"
        )));
    }
    if value.contains('/') || value.contains('\\') {
        return Err(AppError::Config(
            "Note name cannot contain '/' or '\\'".to_string(),
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(AppError::Config(
            "Note name cannot contain control characters".to_string(),
        ));
    }
    Ok(value.to_string())
}

fn normalize_or_unique_name(
    raw: Option<String>,
    fallback: &str,
    sibling_names: &HashSet<String>,
) -> AppResult<String> {
    if let Some(raw) = raw {
        let name = normalize_note_name(&raw)?;
        if sibling_names.contains(&name.to_lowercase()) {
            return Err(AppError::Config(format!(
                "A note item named '{name}' already exists in this folder"
            )));
        }
        return Ok(name);
    }

    let base = normalize_note_name(fallback)?;
    if !sibling_names.contains(&base.to_lowercase()) {
        return Ok(base);
    }
    for index in 2..10_000 {
        let candidate = format!("{base} {index}");
        if !sibling_names.contains(&candidate.to_lowercase()) {
            return Ok(candidate);
        }
    }
    Err(AppError::Config(
        "Could not generate a unique note name".to_string(),
    ))
}

fn sibling_names(
    folders: &[NoteFolder],
    notes: &[impl NoteListItem],
    parent_id: Option<&str>,
    exclude: Option<(&str, &str)>,
) -> HashSet<String> {
    let mut names = HashSet::new();
    for folder in folders {
        if folder.parent_id.as_deref() != parent_id
            || exclude == Some(("folder", folder.id.as_str()))
        {
            continue;
        }
        names.insert(folder.name.to_lowercase());
    }
    for note in notes {
        if note.parent_id() != parent_id || exclude == Some(("note", note.id())) {
            continue;
        }
        names.insert(note.title().to_lowercase());
    }
    names
}

fn validate_unique_sibling_name(
    folders: &[NoteFolder],
    notes: &[impl NoteListItem],
    parent_id: Option<&str>,
    name: &str,
    exclude: Option<(&str, &str)>,
) -> AppResult<()> {
    if sibling_names(folders, notes, parent_id, exclude).contains(&name.to_lowercase()) {
        return Err(AppError::Config(format!(
            "A note item named '{name}' already exists in this folder"
        )));
    }
    Ok(())
}

fn validate_parent_exists(folders: &[NoteFolder], parent_id: Option<&str>) -> AppResult<()> {
    if let Some(parent_id) = parent_id {
        if !folders.iter().any(|folder| folder.id == parent_id) {
            return Err(AppError::Config(format!(
                "Folder '{parent_id}' does not exist"
            )));
        }
    }
    Ok(())
}

fn validate_not_descendant_folder(
    folders: &[NoteFolder],
    source_id: &str,
    target_parent_id: &str,
) -> AppResult<()> {
    let by_id: HashMap<&str, &NoteFolder> = folders
        .iter()
        .map(|folder| (folder.id.as_str(), folder))
        .collect();
    let mut current = Some(target_parent_id);
    let mut visited = HashSet::new();
    while let Some(folder_id) = current {
        if folder_id == source_id {
            return Err(AppError::Config(
                "A folder cannot be moved into its descendant".to_string(),
            ));
        }
        if !visited.insert(folder_id) {
            return Err(AppError::Config(
                "Folder hierarchy contains a cycle".to_string(),
            ));
        }
        current = by_id
            .get(folder_id)
            .and_then(|folder| folder.parent_id.as_deref());
    }
    Ok(())
}

fn collect_descendant_folder_ids(
    folders: &[NoteFolder],
    parent_id: &str,
    collected: &mut HashSet<String>,
) {
    for folder in folders {
        if folder.parent_id.as_deref() == Some(parent_id) && collected.insert(folder.id.clone()) {
            collect_descendant_folder_ids(folders, &folder.id, collected);
        }
    }
}

fn next_sort_order_for_parent(
    folders: &[NoteFolder],
    notes: &[impl NoteListItem],
    parent_id: Option<&str>,
) -> i64 {
    folders
        .iter()
        .filter(|folder| folder.parent_id.as_deref() == parent_id)
        .map(|folder| folder.sort_order)
        .chain(
            notes
                .iter()
                .filter(|note| note.parent_id() == parent_id)
                .map(NoteListItem::sort_order),
        )
        .max()
        .unwrap_or(-1)
        .saturating_add(1)
}

fn validate_notes_snapshot(snapshot: &NotesSnapshot) -> AppResult<()> {
    let mut folder_ids = HashSet::new();
    let mut note_ids = HashSet::new();
    for folder in &snapshot.folders {
        normalize_note_name(&folder.name)?;
        if !folder_ids.insert(folder.id.as_str()) {
            return Err(AppError::Config(format!(
                "Duplicate note folder id '{}'",
                folder.id
            )));
        }
    }
    for note in &snapshot.notes {
        normalize_note_name(&note.title)?;
        if !note_ids.insert(note.id.as_str()) {
            return Err(AppError::Config(format!("Duplicate note id '{}'", note.id)));
        }
    }
    for folder in &snapshot.folders {
        if let Some(parent_id) = folder.parent_id.as_deref() {
            if !folder_ids.contains(parent_id) {
                return Err(AppError::Config(format!(
                    "Note folder '{}' has missing parent '{}'",
                    folder.id, parent_id
                )));
            }
            validate_not_descendant_folder(&snapshot.folders, &folder.id, parent_id)?;
        }
    }
    for note in &snapshot.notes {
        if let Some(parent_id) = note.parent_id.as_deref() {
            if !folder_ids.contains(parent_id) {
                return Err(AppError::Config(format!(
                    "Note '{}' has missing parent '{}'",
                    note.id, parent_id
                )));
            }
        }
    }
    for folder in &snapshot.folders {
        validate_unique_sibling_name(
            &snapshot.folders,
            &snapshot.notes,
            folder.parent_id.as_deref(),
            &folder.name,
            Some(("folder", &folder.id)),
        )?;
    }
    for note in &snapshot.notes {
        validate_unique_sibling_name(
            &snapshot.folders,
            &snapshot.notes,
            note.parent_id.as_deref(),
            &note.title,
            Some(("note", &note.id)),
        )?;
    }
    Ok(())
}

fn sort_note_folders(folders: &mut [NoteFolder]) {
    folders.sort_by(|left, right| {
        left.parent_id
            .cmp(&right.parent_id)
            .then(left.sort_order.cmp(&right.sort_order))
            .then(left.name.cmp(&right.name))
            .then(left.id.cmp(&right.id))
    });
}

fn sort_notes(notes: &mut [NoteDocument]) {
    notes.sort_by(|left, right| {
        left.parent_id
            .cmp(&right.parent_id)
            .then(left.sort_order.cmp(&right.sort_order))
            .then(left.title.cmp(&right.title))
            .then(left.id.cmp(&right.id))
    });
}

fn sort_note_summaries(notes: &mut [NoteSummary]) {
    notes.sort_by(|left, right| {
        left.parent_id
            .cmp(&right.parent_id)
            .then(left.sort_order.cmp(&right.sort_order))
            .then(left.title.cmp(&right.title))
            .then(left.id.cmp(&right.id))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_storage() -> Storage {
        let dir = std::env::temp_dir().join(format!(
            "nyaterm-notes-test-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        Storage::open(&dir).expect("open temp storage")
    }

    fn summary_titles(storage: &Storage) -> Vec<String> {
        storage
            .list_note_summaries()
            .expect("summaries")
            .into_iter()
            .map(|note| note.title)
            .collect()
    }

    #[test]
    fn creates_root_note_and_nested_folder() {
        let storage = temp_storage();
        let folder = storage
            .create_note_folder(None, Some("Projects".to_string()))
            .expect("create folder");
        let note = storage
            .create_note(
                Some(folder.id.clone()),
                Some("Deploy".to_string()),
                Some("# Runbook".to_string()),
            )
            .expect("create note");

        assert_eq!(note.parent_id.as_deref(), Some(folder.id.as_str()));
        assert_eq!(note.revision, 1);
        assert_eq!(storage.list_note_folders().expect("folders").len(), 1);
        assert_eq!(storage.list_notes().expect("notes").len(), 1);
        assert_eq!(summary_titles(&storage), vec!["Deploy".to_string()]);
    }

    #[test]
    fn rejects_duplicate_sibling_names_case_insensitive() {
        let storage = temp_storage();
        storage
            .create_note(None, Some("Readme".to_string()), None)
            .expect("create note");

        let error = storage
            .create_note_folder(None, Some("readme".to_string()))
            .expect_err("duplicate should fail");

        assert!(error.to_string().contains("already exists"));
    }

    #[test]
    fn rejects_folder_move_to_self_or_descendant() {
        let storage = temp_storage();
        let root = storage
            .create_note_folder(None, Some("Root".to_string()))
            .expect("root folder");
        let child = storage
            .create_note_folder(Some(root.id.clone()), Some("Child".to_string()))
            .expect("child folder");

        let self_error = storage
            .move_note_node("folder", &root.id, Some(root.id.clone()), 0)
            .expect_err("self move should fail");
        assert!(self_error.to_string().contains("itself"));

        let descendant_error = storage
            .move_note_node("folder", &root.id, Some(child.id.clone()), 0)
            .expect_err("descendant move should fail");
        assert!(descendant_error.to_string().contains("descendant"));
    }

    #[test]
    fn update_note_increments_revision_and_rejects_stale_revision() {
        let storage = temp_storage();
        let note = storage
            .create_note(None, Some("Draft".to_string()), Some("one".to_string()))
            .expect("create note");
        let updated = storage
            .update_note(
                &note.id,
                "Draft".to_string(),
                "two".to_string(),
                note.revision,
                false,
            )
            .expect("update note");
        assert_eq!(updated.note.revision, note.revision + 1);
        assert!(updated.changed);
        assert!(!updated.tree_changed);

        let error = storage
            .update_note(
                &note.id,
                "Draft".to_string(),
                "three".to_string(),
                note.revision,
                false,
            )
            .expect_err("stale update should fail");
        assert!(error.to_string().contains("Revision conflict"));
    }

    #[test]
    fn rebuilds_missing_note_summary_index_from_documents() {
        let storage = temp_storage();
        storage
            .create_note(
                None,
                Some("Indexed".to_string()),
                Some("large body".to_string()),
            )
            .expect("create note");

        let txn = storage.db.begin_write().expect("txn");
        clear_prefix_in_txn(&txn, NOTE_SUMMARIES_TABLE, NOTE_SUMMARY_PREFIX).expect("clear index");
        {
            let mut meta = txn.open_table(META_TABLE).expect("meta");
            meta.remove(META_NOTE_SUMMARY_INDEX_VERSION)
                .expect("remove meta");
        }
        txn.commit().expect("commit");

        let summaries = storage.list_note_summaries().expect("rebuild summaries");
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].title, "Indexed");
        assert_eq!(summaries[0].parent_id, None);
    }

    #[test]
    fn note_mutations_keep_summary_index_in_sync() {
        let storage = temp_storage();
        let folder = storage
            .create_note_folder(None, Some("Folder".to_string()))
            .expect("folder");
        let note = storage
            .create_note(None, Some("Draft".to_string()), Some("one".to_string()))
            .expect("note");

        let updated = storage
            .update_note(
                &note.id,
                "Published".to_string(),
                "two".to_string(),
                note.revision,
                false,
            )
            .expect("update");
        assert!(updated.tree_changed);
        assert_eq!(summary_titles(&storage), vec!["Published".to_string()]);

        let moved = storage
            .move_note_node("note", &note.id, Some(folder.id.clone()), 42)
            .expect("move");
        assert!(moved.changed);
        let summary = storage
            .list_note_summaries()
            .expect("summaries")
            .into_iter()
            .find(|item| item.id == note.id)
            .expect("moved summary");
        assert_eq!(summary.parent_id.as_deref(), Some(folder.id.as_str()));
        assert_eq!(summary.sort_order, 42);

        let renamed = storage
            .rename_note_node("note", &note.id, "Final".to_string())
            .expect("rename");
        assert!(renamed.changed);
        assert_eq!(summary_titles(&storage), vec!["Final".to_string()]);
    }

    #[test]
    fn recursive_delete_removes_folder_descendants_and_notes() {
        let storage = temp_storage();
        let root = storage
            .create_note_folder(None, Some("Root".to_string()))
            .expect("root folder");
        let child = storage
            .create_note_folder(Some(root.id.clone()), Some("Child".to_string()))
            .expect("child folder");
        storage
            .create_note(Some(child.id.clone()), Some("Leaf".to_string()), None)
            .expect("leaf note");

        let result = storage
            .delete_note_node("folder", &root.id)
            .expect("delete folder");

        assert_eq!(result.folder_count, 2);
        assert_eq!(result.note_count, 1);
        assert!(storage.list_note_folders().expect("folders").is_empty());
        assert!(storage.list_notes().expect("notes").is_empty());
        assert!(storage.list_note_summaries().expect("summaries").is_empty());
    }

    #[test]
    fn snapshot_export_and_replace_roundtrip() {
        let storage = temp_storage();
        let folder = storage
            .create_note_folder(None, Some("Folder".to_string()))
            .expect("folder");
        storage
            .create_note(
                Some(folder.id.clone()),
                Some("Note".to_string()),
                Some("body".to_string()),
            )
            .expect("note");
        let snapshot = storage.load_notes_snapshot().expect("snapshot");

        let replacement = temp_storage();
        replacement
            .replace_notes_snapshot(&snapshot)
            .expect("replace snapshot");
        let roundtrip = replacement.load_notes_snapshot().expect("roundtrip");

        assert_eq!(roundtrip, snapshot);
        assert_eq!(
            replacement.list_note_summaries().expect("summaries").len(),
            1
        );
    }
}
