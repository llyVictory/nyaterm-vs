enum HistoryMatcher {
    Literal {
        needle: String,
        case_sensitive: bool,
        whole_word: bool,
    },
    Regex(regex::Regex),
}

impl HistoryMatcher {
    fn new(query: &str, case_sensitive: bool, regex: bool, whole_word: bool) -> AppResult<Self> {
        if regex {
            let pattern = if whole_word {
                format!(r"\b(?:{query})\b")
            } else {
                query.to_string()
            };
            let compiled = RegexBuilder::new(&pattern)
                .case_insensitive(!case_sensitive)
                .build()
                .map_err(|error| {
                    AppError::Config(format!("Invalid regular expression: {error}"))
                })?;
            return Ok(Self::Regex(compiled));
        }

        let needle = if case_sensitive {
            query.to_string()
        } else {
            query.to_lowercase()
        };

        Ok(Self::Literal {
            needle,
            case_sensitive,
            whole_word,
        })
    }

    fn find(&self, haystack: &str) -> Option<(usize, usize)> {
        match self {
            Self::Literal {
                needle,
                case_sensitive,
                whole_word,
            } => {
                let searchable = if *case_sensitive {
                    haystack.to_string()
                } else {
                    haystack.to_lowercase()
                };
                find_literal_match(&searchable, needle, *whole_word)
            }
            Self::Regex(regex) => regex
                .find(haystack)
                .map(|found| (found.start(), found.end())),
        }
    }
}

fn find_literal_match(haystack: &str, needle: &str, whole_word: bool) -> Option<(usize, usize)> {
    if needle.is_empty() {
        return None;
    }

    let mut offset = 0;
    while offset <= haystack.len() {
        let relative = haystack[offset..].find(needle)?;
        let start = offset + relative;
        let end = start + needle.len();

        if !whole_word || is_word_boundary_match(haystack, start, end) {
            return Some((start, end));
        }

        offset = end;
    }

    None
}

fn is_word_boundary_match(text: &str, start: usize, end: usize) -> bool {
    let before = text[..start].chars().next_back();
    let after = text[end..].chars().next();

    before.is_none_or(|ch| !is_word_char(ch)) && after.is_none_or(|ch| !is_word_char(ch))
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn context_records(
    records: &[TranscriptRecord],
    index: usize,
    count: usize,
    before: bool,
) -> Vec<String> {
    if count == 0 {
        return Vec::new();
    }

    if before {
        let start = index.saturating_sub(count);
        return records[start..index]
            .iter()
            .map(|record| record.data.clone())
            .collect();
    }

    let start = index.saturating_add(1);
    let end = start.saturating_add(count).min(records.len());
    records[start..end]
        .iter()
        .map(|record| record.data.clone())
        .collect()
}

#[cfg(test)]
fn strip_terminal_control_sequences(text: &str) -> String {
    let mut pending = Vec::new();
    let mut cursor = 0;
    let replayed = replay_terminal_output(text, &mut pending, &mut cursor);
    let mut out = String::with_capacity(text.len());
    for line in replayed.lines {
        out.push_str(&line);
        out.push('\n');
    }
    out.extend(pending);
    out
}

struct TerminalReplayResult {
    lines: Vec<String>,
    // Echo matching must stop at the first artificial line boundary.
    first_forced_split: Option<usize>,
}

fn replay_terminal_output(
    text: &str,
    line: &mut Vec<char>,
    cursor: &mut usize,
) -> TerminalReplayResult {
    let bytes = text.as_bytes();
    let mut replayed = TerminalReplayResult {
        lines: Vec::new(),
        first_forced_split: None,
    };
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'\x1b' => {
                i += 1;
                if i >= bytes.len() {
                    break;
                }
                match bytes[i] {
                    b'[' => {
                        i += 1;
                        let params_start = i;
                        while i < bytes.len() {
                            let b = bytes[i];
                            i += 1;
                            if (0x40..=0x7e).contains(&b) {
                                apply_csi_sequence(
                                    &text[params_start..i - 1],
                                    b,
                                    line,
                                    cursor,
                                    &mut replayed.lines,
                                );
                                break;
                            }
                        }
                    }
                    b']' => {
                        i += 1;
                        while i < bytes.len() {
                            if bytes[i] == b'\x07' {
                                i += 1;
                                break;
                            }
                            if bytes[i] == b'\x1b' && i + 1 < bytes.len() && bytes[i + 1] == b'\\' {
                                i += 2;
                                break;
                            }
                            i += 1;
                        }
                    }
                    b'P' | b'X' | b'^' | b'_' => {
                        i += 1;
                        while i < bytes.len() {
                            if bytes[i] == b'\x1b' && i + 1 < bytes.len() && bytes[i + 1] == b'\\' {
                                i += 2;
                                break;
                            }
                            i += 1;
                        }
                    }
                    _ => {
                        advance_one_char(text, &mut i);
                    }
                }
            }
            b'\r' => {
                *cursor = 0;
                i += 1;
            }
            b'\n' => {
                replayed.lines.push(take_replay_line(line, cursor));
                i += 1;
            }
            b'\t' => {
                write_terminal_char(line, cursor, '\t', &mut replayed);
                i += 1;
            }
            b'\x08' => {
                *cursor = cursor.saturating_sub(1);
                i += 1;
            }
            b if b.is_ascii_control() => {
                i += 1;
            }
            b if b.is_ascii() => {
                write_terminal_char(line, cursor, b as char, &mut replayed);
                i += 1;
            }
            _ => {
                if !text.is_char_boundary(i) {
                    i += 1;
                    continue;
                }
                let Some(ch) = text[i..].chars().next() else {
                    break;
                };
                write_terminal_char(line, cursor, ch, &mut replayed);
                i += ch.len_utf8();
            }
        }
    }

    replayed
}

const MAX_REPLAY_CURSOR_COLUMNS: usize = 4096;
// Transcript segments are bounded even when a TUI never sends a newline.
const MAX_REPLAY_LINE_CHARS: usize = 16 * 1024;

fn take_replay_line(line: &mut Vec<char>, cursor: &mut usize) -> String {
    *cursor = 0;
    line.drain(..).collect()
}

fn write_terminal_char(
    line: &mut Vec<char>,
    cursor: &mut usize,
    ch: char,
    replayed: &mut TerminalReplayResult,
) {
    if *cursor >= MAX_REPLAY_LINE_CHARS {
        replayed.first_forced_split.get_or_insert(replayed.lines.len());
        replayed.lines.push(take_replay_line(line, cursor));
    }
    if *cursor > line.len() {
        line.resize(*cursor, ' ');
    }

    if *cursor == line.len() {
        line.push(ch);
    } else {
        line[*cursor] = ch;
    }
    *cursor += 1;
}

fn apply_csi_sequence(
    params: &str,
    final_byte: u8,
    line: &mut Vec<char>,
    cursor: &mut usize,
    lines: &mut Vec<String>,
) {
    match final_byte {
        b'B' | b'E' => {
            // Empty transcript records are discarded; do not expand cursor
            // movement into a parameter-sized collection of blank rows.
            lines.push(take_replay_line(line, cursor));
        }
        b'C' => {
            let count = csi_param(params, 0, 1);
            *cursor = cursor
                .saturating_add(count)
                .min(MAX_REPLAY_CURSOR_COLUMNS);
        }
        b'D' => {
            let count = csi_param(params, 0, 1);
            *cursor = cursor.saturating_sub(count);
        }
        b'G' => {
            let column = csi_param(params, 0, 1);
            *cursor = column
                .saturating_sub(1)
                .min(MAX_REPLAY_CURSOR_COLUMNS);
        }
        b'K' => match csi_param(params, 0, 0) {
            0 => {
                line.truncate(*cursor);
            }
            1 => {
                let end = (*cursor).saturating_add(1).min(line.len());
                line.drain(..end);
                *cursor = 0;
            }
            2 | 3 => {
                line.clear();
                *cursor = 0;
            }
            _ => {}
        },
        b'P' => {
            let count = csi_param(params, 0, 1);
            let end = (*cursor).saturating_add(count).min(line.len());
            if *cursor < end {
                line.drain(*cursor..end);
            }
        }
        b'@' => {
            let count = csi_param(params, 0, 1)
                .min(MAX_REPLAY_LINE_CHARS.saturating_sub(line.len()));
            let start = (*cursor).min(line.len());
            line.splice(start..start, std::iter::repeat_n(' ', count));
        }
        _ => {}
    }
}

fn csi_param(params: &str, index: usize, default: usize) -> usize {
    params
        .split(';')
        .nth(index)
        .map(|part| {
            part.trim_start_matches(|ch: char| {
                matches!(ch, '?' | '>' | '<' | '=')
            })
        })
        .filter(|part| !part.is_empty())
        .and_then(|part| part.parse::<usize>().ok())
        .unwrap_or(default)
}

fn advance_one_char(text: &str, index: &mut usize) {
    if *index >= text.len() {
        return;
    }

    if !text.is_char_boundary(*index) {
        *index += 1;
        return;
    }

    if let Some(ch) = text[*index..].chars().next() {
        *index += ch.len_utf8();
    } else {
        *index = text.len();
    }
}
