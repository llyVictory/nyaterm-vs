fn parse_ini_sections(content: &str) -> HashMap<String, HashMap<String, String>> {
    let mut sections: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut current_section = String::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            current_section = line[1..line.len() - 1].to_string();
            sections.entry(current_section.clone()).or_default();
        } else if let Some((key, value)) = line.split_once('=') {
            if let Some(section) = sections.get_mut(&current_section) {
                section.insert(key.trim().to_string(), value.trim().to_string());
            }
        }
    }

    sections
}

// ── MobaXterm (.mxtsessions) ────────────────────────────────────────────────
