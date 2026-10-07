fn remap_del_to_bs(data: &mut [u8]) {
    for byte in data {
        if *byte == 0x7f {
            *byte = 0x08;
        }
    }
}
use encoding_rs::{CoderResult, Decoder, Encoding, UTF_8};

pub fn terminal_encoding(label: &str) -> &'static Encoding {
    let trimmed = label.trim();
    if trimmed.is_empty() {
        return UTF_8;
    }
    Encoding::for_label(trimmed.as_bytes()).unwrap_or(UTF_8)
}

pub struct TerminalOutputDecoder {
    decoder: Decoder,
}

impl TerminalOutputDecoder {
    pub fn new(encoding: &str) -> Self {
        Self {
            decoder: terminal_encoding(encoding).new_decoder_without_bom_handling(),
        }
    }

    pub fn decode(&mut self, data: &[u8]) -> String {
        let capacity = self
            .decoder
            .max_utf8_buffer_length(data.len())
            .unwrap_or_else(|| data.len().saturating_mul(4));
        let mut output = String::with_capacity(capacity);
        let mut total_read = 0;
        while total_read < data.len() {
            let (result, read, _) =
                self.decoder
                    .decode_to_string(&data[total_read..], &mut output, false);
            total_read += read;
            match result {
                CoderResult::InputEmpty => break,
                CoderResult::OutputFull => {
                    output.reserve(
                        self.decoder
                            .max_utf8_buffer_length(data.len() - total_read)
                            .unwrap_or_else(|| (data.len() - total_read).saturating_mul(4))
                            .max(4),
                    );
                }
            }
        }
        output
    }
}

pub fn encode_terminal_input(data: &[u8], encoding: &str) -> Vec<u8> {
    let target = terminal_encoding(encoding);
    if target == UTF_8 {
        return data.to_vec();
    }

    let mut result = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        if data[i] == 0x1b {
            let end = ansi_sequence_end(data, i);
            result.extend_from_slice(&data[i..end]);
            i = end;
            continue;
        }

        if data[i].is_ascii() {
            result.push(data[i]);
            i += 1;
            continue;
        }

        let start = i;
        while i < data.len() && !data[i].is_ascii() && data[i] != 0x1b {
            i += 1;
        }

        let text = String::from_utf8_lossy(&data[start..i]);
        let (encoded, _, _) = target.encode(&text);
        result.extend_from_slice(&encoded);
    }

    result
}

pub fn prepare_terminal_write_input(
    mut data: Vec<u8>,
    encoding: &str,
    raw: bool,
    backspace_as_bs: bool,
) -> Vec<u8> {
    if raw {
        return data;
    }
    if backspace_as_bs {
        remap_del_to_bs(&mut data);
    }
    encode_terminal_input(&data, encoding)
}

fn ansi_sequence_end(data: &[u8], start: usize) -> usize {
    let mut i = start + 1;
    if i >= data.len() {
        return i;
    }

    if matches!(data[i], b'[' | b']' | b'(' | b')' | b'#') {
        i += 1;
        while i < data.len() {
            let byte = data[i];
            i += 1;
            if (0x40..=0x7e).contains(&byte) {
                break;
            }
        }
        return i;
    }

    (i + 1).min(data.len())
}

#[cfg(test)]
mod tests {
    use super::{TerminalOutputDecoder, encode_terminal_input, prepare_terminal_write_input};

    #[test]
    fn write_input_encodes_text_but_preserves_raw_bytes_and_del() {
        assert_eq!(
            prepare_terminal_write_input("测\u{7f}".as_bytes().to_vec(), "GBK", false, true),
            vec![0xB2, 0xE2, 0x08]
        );
        let raw = vec![0x1b, b'[', b'M', b' ', 0x80, 0xff, 0x7f];
        assert_eq!(
            prepare_terminal_write_input(raw.clone(), "GBK", true, true),
            raw
        );
    }

    #[test]
    fn gbk_decoder_preserves_split_multibyte_character() {
        let mut decoder = TerminalOutputDecoder::new("GBK");
        assert_eq!(decoder.decode(&[0xB2]), "");
        assert_eq!(decoder.decode(&[0xE2]), "测");
    }

    #[test]
    fn gb18030_decoder_preserves_split_multibyte_character() {
        let mut decoder = TerminalOutputDecoder::new("GB18030");
        assert_eq!(decoder.decode(&[0xB2]), "");
        assert_eq!(decoder.decode(&[0xE2]), "测");
    }

    #[test]
    fn gbk_decoder_handles_utf8_output_expansion() {
        let mut decoder = TerminalOutputDecoder::new("GBK");
        let mut input = Vec::new();
        for _ in 0..1024 {
            input.extend_from_slice(&[0xB2, 0xE2]);
        }

        let output = decoder.decode(&input);

        assert_eq!(output.chars().count(), 1024);
        assert!(output.chars().all(|ch| ch == '测'));
    }

    #[test]
    fn input_encoder_keeps_controls_and_ansi_sequences_raw() {
        let input = b"\x1b[200~\x7f\xe6\xb5\x8b\x1b[201~";
        let encoded = encode_terminal_input(input, "GBK");
        assert_eq!(
            encoded,
            vec![
                0x1b, b'[', b'2', b'0', b'0', b'~', 0x7f, 0xB2, 0xE2, 0x1b, b'[', b'2', b'0', b'1',
                b'~'
            ]
        );
    }
}
