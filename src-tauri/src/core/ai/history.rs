pub use nyaterm_core::core::ai::history::*;
pub fn clear_ai_history(app: &tauri::AppHandle) -> crate::error::AppResult<()> {
    super::stream::cancel_all_chat_streams();
    nyaterm_core::core::ai::history::clear_ai_history_storage(app)
}
