use std::time::Duration;

use super::stream::AiEventSink;
use genai::chat::{ChatMessage, ChatRole};
use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::oneshot;

use crate::config::{
    AiApiFormat, AiProviderApiProtocol, AiProviderKind, AiReasoningEffort, AiSettings,
};
use crate::error::{AppError, AppResult};
use crate::utils::url::{join_api_base_url, normalize_api_base_url};

use super::http::build_http_client;
use super::model::ResolvedAiModel;
use super::parser::{trim_string_to_option, truncate_preview};
use super::stream::{AiStreamResult, emit_stream_event};
use super::types::{AiChatRequest, AiStreamEventPayload};

const OPENAI_DEFAULT_BASE_URL: &str = "https://api.openai.com/v1/";

#[derive(Debug, Clone, Serialize)]
struct ResponsesInputMessage {
    role: &'static str,
    content: String,
}

#[derive(Debug)]
enum ResponsesSseEvent {
    TextDelta(String),
    ReasoningDelta(String),
    Completed {
        text: Option<String>,
        reasoning: Option<String>,
    },
    Error(String),
    Ignored,
}

pub fn uses_responses_api(model: &ResolvedAiModel) -> bool {
    if model.api_format != AiApiFormat::Responses {
        return false;
    }

    match model
        .credential
        .as_ref()
        .and_then(|credential| credential.api_protocol.as_ref())
    {
        Some(AiProviderApiProtocol::OpenaiCompatible) => true,
        Some(_) => false,
        None => matches!(
            model.provider_kind,
            AiProviderKind::Openai | AiProviderKind::OpenaiCompatible
        ),
    }
}

pub async fn test_responses_model(
    settings: &AiSettings,
    model: &ResolvedAiModel,
    system_prompt: &str,
    user_prompt: &str,
    max_output_tokens: u32,
) -> AppResult<()> {
    let mut body = json!({
        "model": model.model_name,
        "input": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": user_prompt },
        ],
        "max_output_tokens": max_output_tokens,
        "stream": false,
        "store": false,
    });
    if let Some(effort) = responses_reasoning_effort(&settings.default_reasoning_effort) {
        body["reasoning"] = json!({ "effort": effort });
    }

    let response = responses_request(settings, model, &body)?
        .send()
        .await
        .map_err(|error| AppError::Config(format!("AI model test failed: {error}")))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::Config(format!(
            "AI model test failed: {status} {}",
            truncate_preview(&body, 500)
        )));
    }
    let body: Value = response
        .json()
        .await
        .map_err(|error| AppError::Config(format!("Invalid AI model test response: {error}")))?;
    if let Some(error) = body.get("error").filter(|error| !error.is_null()) {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| error.to_string());
        return Err(AppError::Config(format!("AI model test failed: {message}")));
    }
    if matches!(body["status"].as_str(), Some("failed" | "cancelled")) {
        return Err(AppError::Config("AI model test failed".to_string()));
    }
    if body["id"].as_str().is_none() && body["output"].as_array().is_none() {
        return Err(AppError::Config(
            "AI model test returned an invalid response".to_string(),
        ));
    }
    Ok(())
}

pub async fn run_responses_chat_messages_stream(
    app: &impl AiEventSink,
    stream_id: &str,
    request: &AiChatRequest,
    settings: &AiSettings,
    resolved_model: &ResolvedAiModel,
    messages: &[ChatMessage],
    cancel_rx: &mut oneshot::Receiver<()>,
) -> AppResult<AiStreamResult> {
    let input = responses_input_messages(messages)?;
    run_responses_input_stream(
        app,
        stream_id,
        request,
        settings,
        resolved_model,
        input,
        true,
        cancel_rx,
    )
    .await
}

pub async fn run_responses_chat_messages_stream_without_text_deltas(
    app: &impl AiEventSink,
    stream_id: &str,
    request: &AiChatRequest,
    settings: &AiSettings,
    resolved_model: &ResolvedAiModel,
    messages: &[ChatMessage],
    cancel_rx: &mut oneshot::Receiver<()>,
) -> AppResult<AiStreamResult> {
    let input = responses_input_messages(messages)?;
    run_responses_input_stream(
        app,
        stream_id,
        request,
        settings,
        resolved_model,
        input,
        false,
        cancel_rx,
    )
    .await
}

fn responses_input_messages(messages: &[ChatMessage]) -> AppResult<Vec<ResponsesInputMessage>> {
    let mut input = Vec::new();

    for message in messages {
        let role = match message.role {
            ChatRole::System => "system",
            ChatRole::User => "user",
            ChatRole::Assistant => "assistant",
            ChatRole::Tool => continue,
        };
        let content = message.content.joined_texts().unwrap_or_default();
        if content.trim().is_empty() {
            continue;
        }
        input.push(ResponsesInputMessage { role, content });
    }

    if input.is_empty() {
        return Err(AppError::Config(
            "Responses API request has no text input".to_string(),
        ));
    }

    Ok(input)
}

async fn run_responses_input_stream(
    app: &impl AiEventSink,
    stream_id: &str,
    request: &AiChatRequest,
    settings: &AiSettings,
    resolved_model: &ResolvedAiModel,
    input: Vec<ResponsesInputMessage>,
    emit_text_deltas: bool,
    cancel_rx: &mut oneshot::Receiver<()>,
) -> AppResult<AiStreamResult> {
    let url = responses_url(resolved_model)?;

    let mut body = json!({
        "model": resolved_model.model_name,
        "input": input,
        "stream": true,
        "store": false,
    });
    if let Some(effort) = responses_reasoning_effort(&settings.default_reasoning_effort) {
        body["reasoning"] = json!({ "effort": effort });
    }

    let req = responses_request(settings, resolved_model, &body)?;

    tracing::debug!(
        stream_id = %stream_id,
        model_name = %resolved_model.model_name,
        provider_kind = ?resolved_model.provider_kind,
        url = %url,
        "Dispatching Responses API stream request"
    );

    let send_future = req
        .timeout(Duration::from_millis(settings.timeout_ms))
        .send();
    let mut resp = tokio::select! {
        _ = &mut *cancel_rx => {
            return Err(AppError::Cancelled("AI stream cancelled".to_string()));
        }
        result = send_future => {
            result.map_err(|error| AppError::Config(format!("AI request failed: {error}")))?
        }
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::Config(format!(
            "Responses API request failed: {status} {body}"
        )));
    }

    let mut output = String::new();
    let mut reasoning_output = String::new();
    let mut sse_buffer = String::new();
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
            chunk = resp.chunk() => {
                idle_deadline.as_mut().reset(tokio::time::Instant::now() + idle_duration);
                let Some(chunk) = chunk
                    .map_err(|error| AppError::Config(format!("AI stream failed: {error}")))?
                else {
                    break;
                };
                sse_buffer.push_str(&String::from_utf8_lossy(&chunk));
                process_sse_buffer(
                    app,
                    stream_id,
                    request,
                    &mut sse_buffer,
                    &mut output,
                    &mut reasoning_output,
                    emit_text_deltas,
                )?;
            }
        }
    }

    if !sse_buffer.trim().is_empty() {
        process_sse_block_into_output(
            app,
            stream_id,
            request,
            sse_buffer.trim(),
            &mut output,
            &mut reasoning_output,
            emit_text_deltas,
        )?;
    }

    tracing::info!(
        stream_id = %stream_id,
        text_len = output.len(),
        reasoning_len = reasoning_output.len(),
        text_preview = %truncate_preview(&output, 200),
        reasoning_preview = %truncate_preview(&reasoning_output, 200),
        "Responses API stream completed"
    );

    Ok(AiStreamResult {
        text: output,
        reasoning_content: trim_string_to_option(reasoning_output),
    })
}

fn process_sse_buffer(
    app: &impl AiEventSink,
    stream_id: &str,
    request: &AiChatRequest,
    buffer: &mut String,
    output: &mut String,
    reasoning_output: &mut String,
    emit_text_deltas: bool,
) -> AppResult<()> {
    while let Some((index, separator_len)) = find_sse_separator(buffer) {
        let block = buffer[..index].trim().to_string();
        buffer.drain(..index + separator_len);
        if !block.is_empty() {
            process_sse_block_into_output(
                app,
                stream_id,
                request,
                &block,
                output,
                reasoning_output,
                emit_text_deltas,
            )?;
        }
    }
    Ok(())
}

fn find_sse_separator(buffer: &str) -> Option<(usize, usize)> {
    let lf = buffer.find("\n\n").map(|index| (index, 2));
    let crlf = buffer.find("\r\n\r\n").map(|index| (index, 4));

    match (lf, crlf) {
        (Some(left), Some(right)) => Some(if left.0 <= right.0 { left } else { right }),
        (Some(found), None) | (None, Some(found)) => Some(found),
        (None, None) => None,
    }
}

fn process_sse_block_into_output(
    app: &impl AiEventSink,
    stream_id: &str,
    request: &AiChatRequest,
    block: &str,
    output: &mut String,
    reasoning_output: &mut String,
    emit_text_deltas: bool,
) -> AppResult<()> {
    match parse_responses_sse_block(block)? {
        ResponsesSseEvent::TextDelta(text_delta) => {
            if !text_delta.is_empty() {
                output.push_str(&text_delta);
                if emit_text_deltas {
                    emit_stream_event(
                        app,
                        stream_id,
                        AiStreamEventPayload {
                            event_type: "delta".to_string(),
                            stream_id: stream_id.to_string(),
                            session_id: request.session_id.clone(),
                            text_delta: Some(text_delta),
                            reasoning_delta: None,
                            message: None,
                            command_cards: vec![],
                            usage: None,
                            error: None,
                        },
                    );
                }
            }
        }
        ResponsesSseEvent::ReasoningDelta(reasoning_delta) => {
            if !reasoning_delta.is_empty() {
                reasoning_output.push_str(&reasoning_delta);
                emit_stream_event(
                    app,
                    stream_id,
                    AiStreamEventPayload {
                        event_type: "reasoning_delta".to_string(),
                        stream_id: stream_id.to_string(),
                        session_id: request.session_id.clone(),
                        text_delta: None,
                        reasoning_delta: Some(reasoning_delta),
                        message: None,
                        command_cards: vec![],
                        usage: None,
                        error: None,
                    },
                );
            }
        }
        ResponsesSseEvent::Completed { text, reasoning } => {
            if output.is_empty()
                && let Some(text) = text
            {
                output.push_str(&text);
            }
            if reasoning_output.is_empty()
                && let Some(reasoning) = reasoning
            {
                reasoning_output.push_str(&reasoning);
            }
        }
        ResponsesSseEvent::Error(message) => {
            return Err(AppError::Config(format!(
                "Responses API stream failed: {message}"
            )));
        }
        ResponsesSseEvent::Ignored => {}
    }

    Ok(())
}

fn parse_responses_sse_block(block: &str) -> AppResult<ResponsesSseEvent> {
    let mut event_type: Option<&str> = None;
    let mut data_lines = Vec::new();

    for line in block.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(value) = line.strip_prefix("event:") {
            event_type = Some(value.trim());
        } else if let Some(value) = line.strip_prefix("data:") {
            data_lines.push(value.trim_start());
        }
    }

    let data = data_lines.join("\n");
    if data.is_empty() || data == "[DONE]" {
        return Ok(ResponsesSseEvent::Ignored);
    }

    let value: Value = serde_json::from_str(&data)
        .map_err(|error| AppError::Config(format!("Invalid Responses API stream JSON: {error}")))?;
    let kind = value
        .get("type")
        .and_then(Value::as_str)
        .or(event_type)
        .unwrap_or_default();

    if kind == "error" {
        return Ok(ResponsesSseEvent::Error(error_message(&value)));
    }

    if kind == "response.output_text.delta" {
        return Ok(ResponsesSseEvent::TextDelta(
            value
                .get("delta")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        ));
    }

    if kind.ends_with(".delta") && kind.contains("reasoning") {
        return Ok(ResponsesSseEvent::ReasoningDelta(
            value
                .get("delta")
                .or_else(|| value.get("text"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        ));
    }

    if kind == "response.completed" {
        let response = value.get("response").unwrap_or(&value);
        return Ok(ResponsesSseEvent::Completed {
            text: completed_output_text(response),
            reasoning: completed_reasoning_text(response),
        });
    }

    Ok(ResponsesSseEvent::Ignored)
}

fn error_message(value: &Value) -> String {
    value
        .get("error")
        .and_then(|error| {
            error
                .get("message")
                .or_else(|| error.get("type"))
                .and_then(Value::as_str)
        })
        .or_else(|| value.get("message").and_then(Value::as_str))
        .unwrap_or("Unknown Responses API error")
        .to_string()
}

fn completed_output_text(response: &Value) -> Option<String> {
    let mut text = String::new();
    for item in response.get("output")?.as_array()? {
        if item.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }
        let Some(content) = item.get("content").and_then(Value::as_array) else {
            continue;
        };
        for part in content {
            if part.get("type").and_then(Value::as_str) == Some("output_text")
                && let Some(part_text) = part.get("text").and_then(Value::as_str)
            {
                text.push_str(part_text);
            }
        }
    }
    trim_string_to_option(text)
}

fn completed_reasoning_text(response: &Value) -> Option<String> {
    let mut text = String::new();
    for item in response.get("output")?.as_array()? {
        if item.get("type").and_then(Value::as_str) != Some("reasoning") {
            continue;
        }
        for key in ["summary", "content"] {
            if let Some(parts) = item.get(key).and_then(Value::as_array) {
                for part in parts {
                    if let Some(part_text) = part
                        .get("text")
                        .or_else(|| part.get("summary"))
                        .and_then(Value::as_str)
                    {
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        text.push_str(part_text);
                    }
                }
            }
        }
    }
    trim_string_to_option(text)
}

fn responses_reasoning_effort(value: &AiReasoningEffort) -> Option<&'static str> {
    match value {
        AiReasoningEffort::Auto => None,
        AiReasoningEffort::None => Some("none"),
        AiReasoningEffort::Minimal => Some("minimal"),
        AiReasoningEffort::Low => Some("low"),
        AiReasoningEffort::Medium => Some("medium"),
        AiReasoningEffort::High => Some("high"),
        AiReasoningEffort::XHigh => Some("xhigh"),
        AiReasoningEffort::Max => Some("max"),
        AiReasoningEffort::Ultra => Some("ultra"),
    }
}

pub fn responses_request(
    settings: &AiSettings,
    model: &ResolvedAiModel,
    body: &Value,
) -> AppResult<reqwest::RequestBuilder> {
    let mut request = build_http_client(settings)?
        .post(responses_url(model)?)
        .json(body);
    if let Some(key) = model
        .credential
        .as_ref()
        .and_then(|credential| credential.api_key.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        request = request.bearer_auth(key);
    }
    Ok(request)
}

fn responses_url(model: &ResolvedAiModel) -> AppResult<String> {
    let base_url = responses_base_url(model)?;
    join_api_base_url(&base_url, "responses")
}

fn responses_base_url(model: &ResolvedAiModel) -> AppResult<String> {
    let configured = model
        .credential
        .as_ref()
        .and_then(|credential| credential.base_url.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty());

    match (configured, &model.provider_kind) {
        (Some(base_url), _) => normalize_api_base_url(base_url),
        (None, AiProviderKind::Openai) => Ok(OPENAI_DEFAULT_BASE_URL.to_string()),
        (None, AiProviderKind::OpenaiCompatible) => Err(AppError::Config(
            "Responses API requires an API base URL for OpenAI-compatible credentials".to_string(),
        )),
        (None, provider_kind) => Err(AppError::Config(format!(
            "Responses API requires an API base URL for {provider_kind:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AiProviderCredential, AiProviderKind};

    fn resolved_model(base_url: Option<&str>) -> ResolvedAiModel {
        ResolvedAiModel {
            model_name: "gpt-test".to_string(),
            provider_kind: AiProviderKind::OpenaiCompatible,
            api_format: AiApiFormat::Responses,
            credential: Some(AiProviderCredential {
                id: "credential-test".to_string(),
                name: "Test".to_string(),
                provider_kind: AiProviderKind::OpenaiCompatible,
                icon_data_url: None,
                api_protocol: None,
                api_format: AiApiFormat::Responses,
                base_url: base_url.map(str::to_string),
                api_key: Some("key".to_string()),
                enabled: true,
            }),
        }
    }

    #[test]
    fn responses_url_accepts_missing_trailing_slash() {
        assert_eq!(
            responses_url(&resolved_model(Some("https://api.example.com/v1"))).unwrap(),
            "https://api.example.com/v1/responses"
        );
    }

    #[test]
    fn responses_url_accepts_trailing_slash() {
        assert_eq!(
            responses_url(&resolved_model(Some("https://api.example.com/v1/"))).unwrap(),
            "https://api.example.com/v1/responses"
        );
    }

    #[test]
    fn responses_url_preserves_query() {
        assert_eq!(
            responses_url(&resolved_model(Some(
                "https://api.example.com/v1?api-version=1"
            )))
            .unwrap(),
            "https://api.example.com/v1/responses?api-version=1"
        );
    }

    #[test]
    fn parses_text_delta_event() {
        let event = parse_responses_sse_block(
            r#"event: response.output_text.delta
data: {"type":"response.output_text.delta","delta":"hello"}"#,
        )
        .unwrap();

        assert!(matches!(event, ResponsesSseEvent::TextDelta(ref text) if text == "hello"));
    }

    #[test]
    fn parses_reasoning_delta_event() {
        let event = parse_responses_sse_block(
            r#"data: {"type":"response.reasoning_summary_text.delta","delta":"thinking"}"#,
        )
        .unwrap();

        assert!(matches!(event, ResponsesSseEvent::ReasoningDelta(ref text) if text == "thinking"));
    }

    #[test]
    fn parses_completed_event_text() {
        let event = parse_responses_sse_block(
            r#"data: {"type":"response.completed","response":{"output":[{"type":"message","content":[{"type":"output_text","text":"done"}]}]}}"#,
        )
        .unwrap();

        assert!(matches!(
            event,
            ResponsesSseEvent::Completed { text: Some(ref text), .. } if text == "done"
        ));
    }

    #[test]
    fn parses_error_event() {
        let event = parse_responses_sse_block(
            r#"event: error
data: {"type":"error","error":{"message":"bad request"}}"#,
        )
        .unwrap();

        assert!(matches!(event, ResponsesSseEvent::Error(ref message) if message == "bad request"));
    }

    #[test]
    fn rejects_malformed_json() {
        let error = parse_responses_sse_block("data: {not-json").unwrap_err();

        assert!(
            error
                .to_string()
                .contains("Invalid Responses API stream JSON")
        );
    }

    #[test]
    fn finds_crlf_sse_separator() {
        assert_eq!(find_sse_separator("data: {}\r\n\r\nnext"), Some((8, 4)));
    }

    #[test]
    fn routes_only_openai_protocol_responses_credentials() {
        let mut model = resolved_model(Some("https://api.example.com/v1/"));
        assert!(uses_responses_api(&model));

        model.provider_kind = AiProviderKind::Anthropic;
        assert!(!uses_responses_api(&model));

        model.provider_kind = AiProviderKind::Openai;
        model.api_format = AiApiFormat::ChatCompletions;
        assert!(!uses_responses_api(&model));
    }
}
