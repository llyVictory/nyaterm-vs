use regex::Regex;
use std::sync::OnceLock;

use super::types::{AiChatRequest, AiContext, AiTerminalTarget};

/// Redact every copy of the selected terminal context before prompt construction
/// or history storage, including per-target snapshots used by external agents.
pub fn redact_request(request: &mut AiChatRequest) {
    redact_context(&mut request.context);
    request.user_input = redact_sensitive_text(&request.user_input);
    for target in &mut request.targets {
        redact_target(target);
    }
    for snapshot in &mut request.target_contexts {
        redact_context(&mut snapshot.context);
        if let Some(target) = &mut snapshot.target {
            redact_target(target);
        }
    }
}

fn redact_target(target: &mut AiTerminalTarget) {
    target.label = redact_sensitive_text(&target.label);
    for value in [&mut target.host, &mut target.username]
        .into_iter()
        .flatten()
    {
        *value = redact_sensitive_text(value);
    }
}

pub fn redact_context(context: &mut AiContext) {
    for value in [
        &mut context.connection_name,
        &mut context.host,
        &mut context.username,
        &mut context.description,
        &mut context.shell_path,
        &mut context.serial_port,
        &mut context.cwd,
        &mut context.os,
        &mut context.arch,
        &mut context.session_type,
    ]
    .into_iter()
    .flatten()
    {
        *value = redact_sensitive_text(value);
    }
    for value in context.tags.iter_mut().chain(context.group_path.iter_mut()) {
        *value = redact_sensitive_text(value);
    }
    context.recent_output = redact_sensitive_text(&context.recent_output);
    context.selected_text = redact_sensitive_text(&context.selected_text);
    context.input_buffer = redact_sensitive_text(&context.input_buffer);
}

pub fn redact_sensitive_text(input: &str) -> String {
    let mut output = input.to_string();
    for (pattern, replacement) in redaction_patterns() {
        output = pattern.replace_all(&output, *replacement).to_string();
    }
    output
}

pub fn redact_marker_values(input: &str, markers: &[&str]) -> String {
    let mut output = input.to_string();
    for marker in markers.iter().copied().filter(|marker| !marker.is_empty()) {
        let mut search_start = 0;
        while let Some(relative_index) = output[search_start..].find(marker) {
            let marker_index = search_start + relative_index;
            let value_start = marker_index + marker.len();
            let value_end = output[value_start..]
                .find(['&', ' ', '"'])
                .map(|offset| value_start + offset)
                .unwrap_or(output.len());
            output.replace_range(value_start..value_end, "[redacted]");
            search_start = value_start + "[redacted]".len();
        }
    }
    output
}

fn redaction_patterns() -> &'static [(Regex, &'static str)] {
    static PATTERNS: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        vec![
            (
                Regex::new(
                    r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----",
                )
                .unwrap(),
                "[REDACTED_PRIVATE_KEY]",
            ),
            (
                Regex::new(r"(?i)Authorization:\s*Bearer\s+[A-Za-z0-9._\-]+").unwrap(),
                "Authorization: Bearer [REDACTED]",
            ),
            (
                Regex::new(r"(?i)(password|passwd|pwd)\s*[:=]\s*[^\s;&|]+").unwrap(),
                "$1=[REDACTED]",
            ),
            (
                Regex::new(
                    r"(?i)(token|api[_-]?key|secret[_-]?key|access[_-]?key)\s*[:=]\s*[^\s;&|]+",
                )
                .unwrap(),
                "$1=[REDACTED]",
            ),
            (
                Regex::new(r"AKIA[0-9A-Z]{16}").unwrap(),
                "[REDACTED_AWS_ACCESS_KEY]",
            ),
            (
                Regex::new(r"(?i)(postgres|mysql|mongodb)://[^@\s]+@").unwrap(),
                "$1://[REDACTED]@",
            ),
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_sensitive_values() {
        let raw = "password=secret token:abc Authorization: Bearer abc.def AKIA1234567890ABCDEF";
        let redacted = redact_sensitive_text(raw);
        assert!(!redacted.contains("secret"));
        assert!(!redacted.contains("abc.def"));
        assert!(!redacted.contains("AKIA1234567890ABCDEF"));
    }

    #[test]
    fn redacts_metadata_and_all_target_snapshots_before_prompt_construction() {
        let mut request: AiChatRequest = serde_json::from_value(serde_json::json!({
            "action": "generate_command", "userInput": "password=user-secret",
            "context": {"description": "password=description-secret", "tags": ["token=tag-secret"], "groupPath": ["api_key=group-secret"]},
            "targets": [{"terminalSessionId": "term-1", "label": "token=label-secret", "sessionType": "SSH"}],
            "targetContexts": [{
                "target": {"terminalSessionId": "term-1", "label": "password=target-secret", "sessionType": "SSH"},
                "context": {"description": "token=snapshot-secret", "recentOutput": "password=output-secret", "selectedText": "api_key=selection-secret", "inputBuffer": "token=input-secret"}
            }]
        })).unwrap();
        redact_request(&mut request);
        let payload = serde_json::to_string(&request).unwrap();
        for secret in [
            "user-secret",
            "description-secret",
            "tag-secret",
            "group-secret",
            "label-secret",
            "target-secret",
            "snapshot-secret",
            "output-secret",
            "selection-secret",
            "input-secret",
        ] {
            assert!(!payload.contains(secret), "leaked {secret}");
        }
        assert!(payload.contains("[REDACTED]"));
        assert!(payload.contains("term-1"));
    }

    #[test]
    fn redacts_marker_values_without_revisiting_redacted_markers() {
        let raw = "access_token=abc access_token=[redacted] code=xyz&state=ok refresh_token=end";
        let redacted = redact_marker_values(raw, &["access_token=", "code=", "refresh_token="]);

        assert_eq!(
            redacted,
            "access_token=[redacted] access_token=[redacted] code=[redacted]&state=ok refresh_token=[redacted]"
        );
        assert!(!redacted.contains("abc"));
        assert!(!redacted.contains("code=xyz"));
        assert!(!redacted.contains("refresh_token=end"));
    }

    #[test]
    fn redacts_marker_values_until_space_quote_ampersand_or_line_end() {
        let raw = r#"api_key=one next="api_key=two" code=three&state=ok id_token=four"#;
        let redacted = redact_marker_values(raw, &["api_key=", "code=", "id_token="]);

        assert_eq!(
            redacted,
            r#"api_key=[redacted] next="api_key=[redacted]" code=[redacted]&state=ok id_token=[redacted]"#
        );
    }
}
