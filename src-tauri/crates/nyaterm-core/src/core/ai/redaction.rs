use regex::Regex;
use std::sync::OnceLock;

use super::types::AiContext;

pub fn redact_context(context: &mut AiContext) {
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
