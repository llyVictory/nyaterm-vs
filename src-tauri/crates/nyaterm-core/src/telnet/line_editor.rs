pub fn local_echo_text(data: &[u8]) -> String {
    let mut visible = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        match data[i] {
            0x1b => {
                i += 1;
                if i < data.len() && data[i] == b'[' {
                    i += 1;
                    while i < data.len() && !(0x40..=0x7e).contains(&data[i]) {
                        i += 1;
                    }
                } else if i < data.len() && data[i] == b']' {
                    i += 1;
                    while i < data.len() {
                        if data[i] == 0x07 {
                            break;
                        }
                        if data[i] == 0x1b && i + 1 < data.len() && data[i + 1] == b'\\' {
                            i += 1;
                            break;
                        }
                        i += 1;
                    }
                }
            }
            b'\r' => {
                if i + 1 < data.len() && data[i + 1] == b'\n' {
                    i += 1;
                }
                visible.extend_from_slice(b"\r\n");
            }
            b'\n' => visible.extend_from_slice(b"\r\n"),
            0x20..=0x7e | b'\t' => visible.push(data[i]),
            byte if byte >= 0x80 => visible.push(byte),
            _ => {}
        }
        i += 1;
    }
    String::from_utf8_lossy(&visible).to_string()
}

#[derive(Debug, Default)]
pub struct TelnetLineEditor {
    buffer: String,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct TelnetLineEditResult {
    pub display: String,
    pub writes: Vec<Vec<u8>>,
}

impl TelnetLineEditor {
    pub fn buffer(&self) -> &str {
        &self.buffer
    }

    pub fn process(&mut self, data: &[u8], enter_mode: TelnetEnterMode) -> TelnetLineEditResult {
        let input = String::from_utf8_lossy(data);
        let mut result = TelnetLineEditResult::default();
        let mut chars = input.char_indices().peekable();

        while let Some((idx, ch)) = chars.next() {
            match ch {
                '\r' | '\n' => {
                    if ch == '\r' {
                        if let Some((_, '\n')) = chars.peek().copied() {
                            chars.next();
                        }
                    }

                    let mut line = self.buffer.as_bytes().to_vec();
                    line.extend_from_slice(enter_bytes(enter_mode));
                    result.writes.push(line);
                    result.display.push_str("\r\n");
                    self.buffer.clear();
                }
                '\u{7f}' | '\u{8}' => {
                    self.backspace(&mut result.display);
                }
                '\u{1b}' => {
                    let end = consume_escape_sequence_end(idx, &mut chars);
                    let sequence = &input.as_bytes()[idx..end];
                    if sequence == b"\x1b[3~" {
                        self.backspace(&mut result.display);
                    } else {
                        result.writes.push(sequence.to_vec());
                    }
                }
                '\t' | ' '..='\u{7e}' if !ch.is_control() => {
                    self.buffer.push(ch);
                    result.display.push(ch);
                }
                ch if !ch.is_control() => {
                    self.buffer.push(ch);
                    result.display.push(ch);
                }
                _ => {
                    let mut bytes = [0u8; 4];
                    result
                        .writes
                        .push(ch.encode_utf8(&mut bytes).as_bytes().to_vec());
                }
            }
        }

        result
    }

    fn backspace(&mut self, display: &mut String) {
        if self.buffer.pop().is_some() {
            display.push_str("\x08 \x08");
        }
    }
}

pub fn consume_escape_sequence_end(
    start: usize,
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
) -> usize {
    let Some((_, next)) = chars.peek().copied() else {
        return start + 1;
    };

    match next {
        '[' => {
            let mut end = start + 1;
            while let Some((idx, ch)) = chars.next() {
                end = idx + ch.len_utf8();
                if (('\u{40}'..='\u{7e}').contains(&ch)) && ch != '[' {
                    break;
                }
            }
            end
        }
        ']' => {
            let mut end = start + 1;
            while let Some((idx, ch)) = chars.next() {
                end = idx + ch.len_utf8();
                if ch == '\u{7}' {
                    break;
                }
                if ch == '\u{1b}' {
                    if let Some((_, '\\')) = chars.peek().copied() {
                        let (esc_end_idx, esc_end_ch) = chars.next().expect("peeked char");
                        end = esc_end_idx + esc_end_ch.len_utf8();
                        break;
                    }
                }
            }
            end
        }
        _ => {
            let (idx, ch) = chars.next().expect("peeked char");
            idx + ch.len_utf8()
        }
    }
}
