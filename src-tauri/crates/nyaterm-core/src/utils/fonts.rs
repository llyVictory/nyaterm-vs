use font_kit::font::Font;
use font_kit::source::SystemSource;
use serde::Serialize;

pub const DEFAULT_TERMINAL_FONT_FAMILY: &str = "\"JetBrainsMono Nerd Font Mono\", \"JetBrains Mono\", \"Cascadia Mono\", \"SF Mono\", Menlo, Monaco, Consolas, \"Liberation Mono\", monospace";
const LEGACY_DEFAULT_TERMINAL_FONT_FAMILY: &str = "\"JetBrains Mono\", \"Cascadia Mono\", \"SF Mono\", Menlo, Monaco, Consolas, \"Liberation Mono\", monospace";

#[cfg(target_os = "macos")]
pub const DEFAULT_UI_FONT_FAMILY: &str = "system-ui, -apple-system, BlinkMacSystemFont, \"PingFang SC\", \"Helvetica Neue\", Arial, sans-serif";

#[cfg(target_os = "windows")]
pub const DEFAULT_UI_FONT_FAMILY: &str =
    "system-ui, \"Segoe UI\", \"Microsoft YaHei\", \"Noto Sans SC\", Arial, sans-serif";

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub const DEFAULT_UI_FONT_FAMILY: &str =
    "system-ui, \"Noto Sans SC\", \"Noto Sans CJK SC\", \"Helvetica Neue\", Arial, sans-serif";

const BUILT_IN_MONOSPACE_FONTS: &[&str] = &["JetBrainsMono Nerd Font Mono", "JetBrains Mono"];
const GENERIC_MONOSPACE_FAMILY: &str = "monospace";

#[derive(Debug, Clone, Serialize)]
pub struct FontInfo {
    pub family: String,
    pub monospace: bool,
}

pub fn list_system_font_infos() -> Vec<FontInfo> {
    let source = SystemSource::new();
    let Ok(mut families) = source.all_families() else {
        return Vec::new();
    };

    families.sort();
    families.dedup();
    families
        .into_iter()
        .map(|family| FontInfo {
            monospace: is_system_font_family_monospace_with_source(&source, &family),
            family,
        })
        .collect()
}

pub fn list_system_font_families() -> Vec<String> {
    let Ok(mut families) = SystemSource::new().all_families() else {
        return Vec::new();
    };

    families.sort();
    families.dedup();
    families
}

pub fn normalize_terminal_font_family(font_family: &str) -> String {
    if is_default_terminal_font_family(font_family) {
        return DEFAULT_TERMINAL_FONT_FAMILY.to_string();
    }

    if is_legacy_default_terminal_font_family(font_family) {
        return DEFAULT_TERMINAL_FONT_FAMILY.to_string();
    }

    let source = SystemSource::new();
    let mut kept = Vec::new();

    for raw_family in split_font_family_stack(font_family) {
        let family = unquote_font_family(&raw_family);
        if is_generic_monospace_family(&family) {
            push_unique_font_family(&mut kept, GENERIC_MONOSPACE_FAMILY.to_string());
            continue;
        }

        if is_built_in_monospace_family(&family)
            || is_system_font_family_monospace_with_source(&source, &family)
        {
            push_unique_font_family(&mut kept, raw_family.trim().to_string());
        }
    }

    if kept.is_empty() {
        DEFAULT_TERMINAL_FONT_FAMILY.to_string()
    } else {
        kept.join(", ")
    }
}

fn is_built_in_monospace_family(family: &str) -> bool {
    BUILT_IN_MONOSPACE_FONTS
        .iter()
        .any(|known| known.eq_ignore_ascii_case(family))
}

fn is_default_terminal_font_family(font_family: &str) -> bool {
    font_family
        .trim()
        .eq_ignore_ascii_case(DEFAULT_TERMINAL_FONT_FAMILY)
}

fn is_legacy_default_terminal_font_family(font_family: &str) -> bool {
    font_family
        .trim()
        .eq_ignore_ascii_case(LEGACY_DEFAULT_TERMINAL_FONT_FAMILY)
}

fn is_generic_monospace_family(family: &str) -> bool {
    family.eq_ignore_ascii_case(GENERIC_MONOSPACE_FAMILY)
}

fn is_system_font_family_monospace_with_source(source: &SystemSource, family: &str) -> bool {
    let Ok(family_handle) = source.select_family_by_name(family) else {
        return false;
    };

    family_handle
        .fonts()
        .iter()
        .filter_map(|handle| Font::from_handle(handle).ok())
        .any(|font| is_font_monospace(&font))
}

fn is_font_monospace(font: &Font) -> bool {
    font.is_monospace() || has_equal_sample_advances(&font)
}

fn has_equal_sample_advances(font: &Font) -> bool {
    let mut advances = Vec::new();

    for ch in ['i', 'W', '0', 'm', ' '] {
        let Some(glyph_id) = font.glyph_for_char(ch) else {
            return false;
        };
        let Ok(advance) = font.advance(glyph_id) else {
            return false;
        };
        advances.push(advance.x());
    }

    let Some(first) = advances.first().copied() else {
        return false;
    };
    advances
        .iter()
        .all(|advance| (advance - first).abs() <= f32::EPSILON)
}

fn split_font_family_stack(stack: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaping = false;

    for ch in stack.chars() {
        if escaping {
            current.push(ch);
            escaping = false;
            continue;
        }

        if ch == '\\' {
            current.push(ch);
            escaping = true;
            continue;
        }

        match quote {
            Some(active_quote) if ch == active_quote => {
                quote = None;
                current.push(ch);
            }
            Some(_) => current.push(ch),
            None if ch == '\'' || ch == '"' => {
                quote = Some(ch);
                current.push(ch);
            }
            None if ch == ',' => {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    result.push(trimmed.to_string());
                }
                current.clear();
            }
            None => current.push(ch),
        }
    }

    let trimmed = current.trim();
    if !trimmed.is_empty() {
        result.push(trimmed.to_string());
    }

    result
}

fn unquote_font_family(family: &str) -> String {
    let trimmed = family.trim();
    if trimmed.len() >= 2 {
        let first = trimmed.chars().next();
        let last = trimmed.chars().next_back();
        if matches!(
            (first, last),
            (Some('\''), Some('\'')) | (Some('"'), Some('"'))
        ) {
            return trimmed[1..trimmed.len() - 1].to_string();
        }
    }
    trimmed.to_string()
}

fn push_unique_font_family(families: &mut Vec<String>, family: String) {
    let normalized = unquote_font_family(&family);
    if families
        .iter()
        .map(|existing| unquote_font_family(existing))
        .any(|existing| existing.eq_ignore_ascii_case(&normalized))
    {
        return;
    }
    families.push(family);
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_TERMINAL_FONT_FAMILY, normalize_terminal_font_family, split_font_family_stack,
        unquote_font_family,
    };

    #[test]
    fn splits_font_stack_without_splitting_inside_quotes() {
        assert_eq!(
            split_font_family_stack("JetBrains Mono, \"Noto, Sans\", monospace"),
            vec!["JetBrains Mono", "\"Noto, Sans\"", "monospace"]
        );
    }

    #[test]
    fn removes_matching_wrapping_quotes() {
        assert_eq!(unquote_font_family("\"Cascadia Mono\""), "Cascadia Mono");
        assert_eq!(unquote_font_family("'JetBrains Mono'"), "JetBrains Mono");
    }

    #[test]
    fn normalizes_legacy_default_to_nerd_font_default() {
        assert_eq!(
            normalize_terminal_font_family(
                "\"JetBrains Mono\", \"Cascadia Mono\", \"SF Mono\", Menlo, Monaco, Consolas, \"Liberation Mono\", monospace"
            ),
            DEFAULT_TERMINAL_FONT_FAMILY
        );
    }

    #[test]
    fn keeps_current_default_terminal_font_stack() {
        assert_eq!(
            normalize_terminal_font_family(DEFAULT_TERMINAL_FONT_FAMILY),
            DEFAULT_TERMINAL_FONT_FAMILY
        );
    }

    #[test]
    fn keeps_custom_jetbrains_mono_stack_without_forcing_nerd_font() {
        assert_eq!(
            normalize_terminal_font_family("\"JetBrains Mono\", monospace"),
            "\"JetBrains Mono\", monospace"
        );
    }
}
