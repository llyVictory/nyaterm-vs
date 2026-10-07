use crate::{
    commands::{argument, text},
    error::{Result, WebError},
    state::State,
};
use nyaterm_core::{config, storage};
use serde_json::{Value, json};
pub fn supports(command: &str) -> bool {
    matches!(
        command,
        "list_note_tree"
            | "get_note"
            | "create_note"
            | "create_note_folder"
            | "update_note"
            | "rename_note_node"
            | "move_note_node"
            | "delete_note_node"
            | "get_notes_export"
    )
}
pub async fn command(state: &State, command: &str, args: &Value) -> Result<Value> {
    if command == "get_notes_export" {
        return Ok(json!(storage::load_notes_snapshot()?));
    }
    if command == "list_note_tree" {
        return Ok(json!(config::NoteTreePayload {
            folders: storage::list_note_folders()?,
            notes: storage::list_note_summaries()?
        }));
    }
    if command == "get_note" {
        return Ok(json!(
            storage::get_note(text(args, "noteId")?)?.ok_or(WebError::bad("Note not found"))?
        ));
    }
    let _guard = state.mutation.lock().await;
    let (result, event) = match command {
        "create_note" => {
            let note = storage::create_note(
                argument(args, "parentId")?,
                argument(args, "title")?,
                argument(args, "markdown")?,
            )?;
            let event = json!({"kind":"created","nodeKind":"note","ids":[note.id],"folders":[],"notes":[config::NoteSummary::from(note.clone())],"treeChanged":true});
            (json!(note), event)
        }
        "create_note_folder" => {
            let folder =
                storage::create_note_folder(argument(args, "parentId")?, argument(args, "name")?)?;
            let event = json!({"kind":"created","nodeKind":"folder","ids":[folder.id],"folders":[folder],"notes":[],"treeChanged":true});
            (json!(folder), event)
        }
        "update_note" => {
            let update = storage::update_note(
                text(args, "noteId")?,
                argument(args, "title")?,
                argument(args, "markdown")?,
                argument(args, "expectedRevision")?,
                args["force"].as_bool().unwrap_or(false),
            )
            .map_err(|error| match error {
                nyaterm_core::error::AppError::Config(message)
                    if message.starts_with("Revision conflict:") =>
                {
                    WebError(axum::http::StatusCode::CONFLICT, "Revision conflict".into())
                }
                other => other.into(),
            })?;
            let event = json!({"kind":"updated","nodeKind":"note","ids":[update.note.id],"folders":[],"notes":[config::NoteSummary::from(update.note.clone())],"treeChanged":update.tree_changed});
            (json!(update.note), event)
        }
        "delete_note_node" => {
            let deleted =
                storage::delete_note_node(text(args, "nodeKind")?, text(args, "nodeId")?)?;
            let event = json!({"kind":"deleted","nodeKind":args["nodeKind"],"ids":deleted.ids,"folders":[],"notes":[],"treeChanged":true});
            (json!(deleted), event)
        }
        _ => {
            let change = if command == "rename_note_node" {
                storage::rename_note_node(
                    text(args, "nodeKind")?,
                    text(args, "nodeId")?,
                    argument(args, "name")?,
                )?
            } else {
                storage::move_note_node(
                    text(args, "nodeKind")?,
                    text(args, "nodeId")?,
                    argument(args, "parentId")?,
                    argument(args, "sortOrder")?,
                )?
            };
            (
                Value::Null,
                json!({"kind":if command=="rename_note_node" {"renamed"} else {"moved"},"nodeKind":args["nodeKind"],"ids":[args["nodeId"]],"folders":change.folder.into_iter().collect::<Vec<_>>(),"notes":change.note.into_iter().collect::<Vec<_>>(),"treeChanged":change.tree_changed}),
            )
        }
    };
    state.broadcast("notes-changed", event).await;
    Ok(result)
}
