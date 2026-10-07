fn decode_bytes(raw: &[u8]) -> String {
    if let Some((enc, bom_len)) = encoding_rs::Encoding::for_bom(raw) {
        let (decoded, _, _) = enc.decode(&raw[bom_len..]);
        return decoded.into_owned();
    }
    match std::str::from_utf8(raw) {
        Ok(s) => s.to_string(),
        Err(_) => {
            let (decoded, _, _) = encoding_rs::GBK.decode(raw);
            decoded.into_owned()
        }
    }
}

// ── Xshell (.xts) ──────────────────────────────────────────────────────────
