use crate::config;

pub fn merge_model_discoveries(
    settings: &config::AiSettings,
    discoveries: Vec<super::types::AiModelDiscovery>,
) -> Vec<config::AiModelConfigItem> {
    let old_by_id = settings
        .models
        .iter()
        .map(|model| (model.id.as_str(), model))
        .collect::<std::collections::HashMap<_, _>>();
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| String::new());
    let mut seen = std::collections::HashSet::new();
    let mut merged = Vec::new();

    for item in discoveries {
        if !seen.insert(item.id.clone()) {
            continue;
        }
        if let Some(old) = old_by_id.get(item.id.as_str()) {
            let mut model = (*old).clone();
            model.last_seen_at = Some(now.clone());
            merged.push(model);
        } else {
            merged.push(config::AiModelConfigItem {
                id: item.id,
                name: item.name,
                backend: item.backend,
                provider_kind: item.provider_kind,
                credential_id: item.credential_id,
                enabled: false,
                source: item.source,
                last_seen_at: Some(now.clone()),
                supported_reasoning_efforts: None,
            });
        }
    }

    for old in &settings.models {
        if !seen.contains(&old.id) {
            merged.push(old.clone());
        }
    }

    merged.sort_by(|left, right| left.name.cmp(&right.name));
    merged
}

pub fn update_default_model_id(
    settings: &config::AiSettings,
    models: &[config::AiModelConfigItem],
) -> Option<String> {
    if let Some(default_model_id) = settings.default_model_id.as_deref() {
        if models
            .iter()
            .any(|model| model.enabled && model.id == default_model_id)
        {
            return Some(default_model_id.to_string());
        }
    }
    models
        .iter()
        .find(|model| model.enabled)
        .map(|model| model.id.clone())
}
