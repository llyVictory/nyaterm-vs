use super::{
    history::load_history,
    model::{build_chat_options, build_client, resolve_request_model},
    parser::{extract_text_from_assistant, trim_string_to_option},
    prompt::{build_prompt, system_prompt},
    types::{AiChatRequest, AiMessage, AiMessageRole, AiStreamEventPayload},
};
use crate::config::AiSettings;
use crate::error::{AppError, AppResult};
use futures_util::StreamExt;
use genai::chat::{ChatMessage, ChatRequest, ChatStreamEvent};
use std::time::Duration;
use tokio::sync::oneshot;

pub trait AiEventSink: Send + Sync {
    fn emit(&self, stream_id: &str, payload: AiStreamEventPayload);
}
pub struct EventSink<F>(pub F);
impl<F: Fn(&str, AiStreamEventPayload) + Send + Sync> AiEventSink for EventSink<F> {
    fn emit(&self, id: &str, payload: AiStreamEventPayload) {
        (self.0)(id, payload)
    }
}
pub fn emit_stream_event(sink: &impl AiEventSink, id: &str, payload: AiStreamEventPayload) {
    sink.emit(id, payload)
}

#[derive(Debug, Clone)]
pub struct AiStreamResult {
    pub text: String,
    pub reasoning_content: Option<String>,
}

pub async fn run_model_stream(
    app: &impl AiEventSink,
    stream_id: &str,
    request: &AiChatRequest,
    settings: &AiSettings,
    cancel_rx: &mut oneshot::Receiver<()>,
) -> AppResult<AiStreamResult> {
    tracing::debug!(
        stream_id = %stream_id,
        action = ?request.action,
        session_id = ?request.session_id,
        "Preparing AI model stream"
    );

    let resolved_model = resolve_request_model(settings, request)?;
    let prompt = build_prompt(request, settings);

    let mut messages = vec![ChatMessage::system(system_prompt(
        &request.options.language,
    ))];

    if let Some(session_id) = &request.session_id {
        let max_turns = request.options.history_turns as usize;
        if max_turns > 0 {
            if let Ok(history) = load_history(app) {
                let history_msgs: Vec<&AiMessage> = history
                    .messages
                    .iter()
                    .filter(|m| m.session_id == *session_id)
                    .collect();
                let skip = history_msgs.len().saturating_sub(max_turns);
                for msg in history_msgs.into_iter().skip(skip) {
                    match msg.role {
                        AiMessageRole::User => {
                            messages.push(ChatMessage::user(&msg.content));
                        }
                        AiMessageRole::Assistant => {
                            let content = extract_text_from_assistant(&msg.content);
                            if !content.is_empty() {
                                messages.push(ChatMessage::assistant(&content));
                            }
                        }
                        AiMessageRole::System => {}
                    }
                }
            }
        }
    }

    messages.push(ChatMessage::user(prompt));

    if super::responses::uses_responses_api(&resolved_model) {
        return super::responses::run_responses_chat_messages_stream(
            app,
            stream_id,
            request,
            settings,
            &resolved_model,
            &messages,
            cancel_rx,
        )
        .await;
    }

    let client = build_client(&resolved_model, settings)?;

    tracing::debug!(
        stream_id = %stream_id,
        message_count = messages.len(),
        model_name = %resolved_model.model_name,
        provider_kind = ?resolved_model.provider_kind,
        "Dispatching AI model stream request"
    );

    let chat_req = ChatRequest::new(messages);
    let chat_options = build_chat_options(settings);

    let stream_result = tokio::time::timeout(
        Duration::from_millis(settings.timeout_ms),
        client.exec_chat_stream(&resolved_model.model_name, chat_req, Some(&chat_options)),
    )
    .await
    .map_err(|_| AppError::Config("AI request timed out".to_string()))?
    .map_err(|error| AppError::Config(format!("AI request failed: {error}")))?;

    let mut stream = stream_result.stream;
    let mut output = String::new();
    let mut reasoning_output = String::new();
    let idle_duration = Duration::from_millis(settings.timeout_ms);
    let idle_deadline = tokio::time::sleep(idle_duration);
    tokio::pin!(idle_deadline);

    loop {
        tokio::select! {
            _ = &mut idle_deadline => {
                return Err(AppError::Config("AI stream timed out (no data received)".to_string()));
            }
            _ = &mut *cancel_rx => {
                return Err(AppError::Cancelled("AI stream cancelled".to_string()));
            }
            item = stream.next() => {
                idle_deadline.as_mut().reset(tokio::time::Instant::now() + idle_duration);
                match item {
                    Some(Ok(ChatStreamEvent::Chunk(chunk))) => {
                        let text_delta = chunk.content;
                        if !text_delta.is_empty() {
                            output.push_str(&text_delta);
                            emit_stream_event(app, stream_id, AiStreamEventPayload {
                                event_type: "delta".to_string(),
                                stream_id: stream_id.to_string(),
                                session_id: request.session_id.clone(),
                                text_delta: Some(text_delta),
                                reasoning_delta: None,
                                message: None,
                                command_cards: vec![],
                                usage: None,
                                error: None,
                            });
                        }
                    }
                    Some(Ok(ChatStreamEvent::ReasoningChunk(chunk))) => {
                        let reasoning_delta = chunk.content;
                        if !reasoning_delta.is_empty() {
                            reasoning_output.push_str(&reasoning_delta);
                            emit_stream_event(app, stream_id, AiStreamEventPayload {
                                event_type: "reasoning_delta".to_string(),
                                stream_id: stream_id.to_string(),
                                session_id: request.session_id.clone(),
                                text_delta: None,
                                reasoning_delta: Some(reasoning_delta),
                                message: None,
                                command_cards: vec![],
                                usage: None,
                                error: None,
                            });
                        }
                    }
                    Some(Ok(ChatStreamEvent::End(end))) => {
                        if reasoning_output.is_empty() {
                            if let Some(captured_reasoning_content) = end.captured_reasoning_content {
                                reasoning_output = captured_reasoning_content;
                            }
                        }
                        break;
                    }
                    None => break,
                    Some(Ok(_)) => {}
                    Some(Err(error)) => {
                        return Err(AppError::Config(format!("AI stream failed: {error}")));
                    }
                }
            }
        }
    }

    tracing::info!(
        stream_id = %stream_id,
        text_len = output.len(),
        reasoning_len = reasoning_output.len(),
        "AI model stream completed"
    );

    Ok(AiStreamResult {
        text: output,
        reasoning_content: trim_string_to_option(reasoning_output),
    })
}
