use crate::config;
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[derive(Debug, Clone, Serialize)]
pub struct KeywordHighlightImportResult {
    pub imported_rules: usize,
    pub updated_rules: usize,
    pub total_rules: usize,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum KeywordHighlightImportFile {
    Config {
        keyword_highlights: Vec<config::KeywordHighlightRule>,
    },
    Rules(Vec<config::KeywordHighlightRule>),
}

pub fn parse_keyword_highlight_import(raw: &str) -> AppResult<Vec<config::KeywordHighlightRule>> {
    let import_file: KeywordHighlightImportFile = serde_json::from_str(raw)
        .map_err(|error| AppError::Config(format!("Invalid highlight rules JSON: {error}")))?;

    Ok(match import_file {
        KeywordHighlightImportFile::Config { keyword_highlights } => keyword_highlights,
        KeywordHighlightImportFile::Rules(rules) => rules,
    })
}

pub fn merge_keyword_highlight_rules(
    existing: &mut Vec<config::KeywordHighlightRule>,
    imported: Vec<config::KeywordHighlightRule>,
    mut next_id: impl FnMut() -> String,
) -> AppResult<KeywordHighlightImportResult> {
    let mut imported_rules = 0;
    let mut updated_rules = 0;
    let mut indexes = existing
        .iter()
        .enumerate()
        .filter_map(|(index, rule)| (!rule.id.trim().is_empty()).then(|| (rule.id.clone(), index)))
        .collect::<HashMap<_, _>>();

    for mut rule in imported {
        rule.name = rule.name.trim().to_string();
        rule.patterns = rule
            .patterns
            .into_iter()
            .map(|pattern| pattern.trim().to_string())
            .filter(|pattern| !pattern.is_empty())
            .collect();

        if rule.name.is_empty() || rule.patterns.is_empty() {
            continue;
        }

        rule.id = rule.id.trim().to_string();
        if rule.id.is_empty() {
            rule.id = next_id();
        }

        if let Some(index) = indexes.get(&rule.id).copied() {
            existing[index] = rule;
            updated_rules += 1;
        } else {
            let id = rule.id.clone();
            existing.push(rule);
            indexes.insert(id, existing.len() - 1);
            imported_rules += 1;
        }
    }

    if imported_rules == 0 && updated_rules == 0 {
        return Err(AppError::Config(
            "No valid highlight rules found in import file".to_string(),
        ));
    }

    Ok(KeywordHighlightImportResult {
        imported_rules,
        updated_rules,
        total_rules: existing.len(),
    })
}
