//! Persistent command suggestions. Web has no shell confirmation protocol, so
//! only explicit client submissions enter history, never terminal output.
use crate::{
    commands::{argument, text},
    error::Result,
    state::State,
};
use nyaterm_core::{config, core::history::CommandHistoryStore, utils::fuzzy};
use serde_json::{Value, json};

pub fn supports(command: &str) -> bool {
    if command == "import_quick_commands" {
        return true;
    }
    matches!(
        command,
        "get_quick_commands"
            | "save_quick_commands"
            | "upsert_quick_command"
            | "increment_quick_command_use_count"
            | "fuzzy_search_commands"
            | "fuzzy_search_candidates"
            | "get_command_history"
            | "fuzzy_search_history"
            | "delete_command_history"
            | "register_command_submission"
            | "register_command_confirmation_candidate"
    )
}
fn limit(args: &Value) -> usize {
    args["limit"].as_u64().unwrap_or(8).min(200) as usize
}
pub async fn command(state: &State, owner: &str, command: &str, args: &Value) -> Result<Value> {
    if matches!(
        command,
        "register_command_submission" | "register_command_confirmation_candidate"
    ) {
        state.session(owner, text(args, "sessionId")?).await?;
        if command == "register_command_confirmation_candidate" {
            return Ok(json!(false));
        }
    }
    let _guard = state.mutation.lock().await;
    Ok(match command {
        "import_quick_commands" => {
            let store = nyaterm_core::core::quick_commands::QuickCommandsStore::new();
            store.load_from_disk(&())?;
            let content = text(args, "content")?;
            if content.len() > 1024 * 1024 {
                return Err(crate::error::WebError::bad("Import exceeds 1 MiB"));
            }
            let result = store.import_from_text(&(), content, argument(args, "source")?)?;
            state.broadcast("quick-commands-changed", Value::Null).await;
            json!(result)
        }
        "get_quick_commands" => json!(config::load_quick_commands(&())?),
        "fuzzy_search_commands" => {
            let cfg = config::load_quick_commands(&())?;
            let items = cfg
                .commands
                .iter()
                .map(|c| (c.label.as_str(), c.command.as_str()))
                .collect::<Vec<_>>();
            json!(fuzzy::fuzzy_search_items(
                &items,
                text(args, "pattern")?,
                "quickCommand",
                limit(args),
                None,
                None
            ))
        }
        "fuzzy_search_candidates" => {
            let items: Vec<fuzzy::FuzzySearchCandidate> = argument(args, "items")?;
            json!(fuzzy::fuzzy_search_candidates(
                &items,
                text(args, "pattern")?,
                limit(args)
            ))
        }
        "save_quick_commands" | "upsert_quick_command" | "increment_quick_command_use_count" => {
            let store = nyaterm_core::core::quick_commands::QuickCommandsStore::new();
            store.load_from_disk(&())?;
            match command {
                "save_quick_commands" => store.save_all(&(), argument(args, "config")?)?,
                "upsert_quick_command" => {
                    store.upsert(
                        &(),
                        argument(args, "command")?,
                        argument(args, "newCategory")?,
                    )?;
                }
                _ => store.increment_use_count(&(), text(args, "id")?)?,
            }
            state.broadcast("quick-commands-changed", Value::Null).await;
            Value::Null
        }
        _ => {
            let mut history = CommandHistoryStore::new();
            history.load()?;
            match command {
                "get_command_history" => json!(history.list()),
                "fuzzy_search_history" => json!(history.search(
                    text(args, "pattern")?,
                    limit(args),
                    args["minCommandLength"].as_u64().map(|v| v as usize),
                    args["maxCommandLength"].as_u64().map(|v| v as usize)
                )),
                _ => {
                    let changed = if command == "delete_command_history" {
                        history.delete_command(text(args, "command")?)
                    } else {
                        history.add(text(args, "command")?.into())
                    };
                    if changed {
                        history.save()?;
                        state
                            .broadcast("command-history-changed", Value::Null)
                            .await;
                    }
                    Value::Null
                }
            }
        }
    })
}
