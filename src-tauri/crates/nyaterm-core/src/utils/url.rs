use reqwest::Url;

use crate::error::{AppError, AppResult};

pub fn normalize_api_base_url(value: &str) -> AppResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }

    let mut url = Url::parse(trimmed)
        .map_err(|err| AppError::Config(format!("Invalid API base URL '{trimmed}': {err}")))?;
    if !url.path().ends_with('/') {
        url.set_path(&format!("{}/", url.path()));
    }
    Ok(url.to_string())
}

pub fn join_api_base_url(base_url: &str, suffix: &str) -> AppResult<String> {
    let normalized = normalize_api_base_url(base_url)?;
    let url = Url::parse(&normalized).map_err(|err| {
        AppError::Config(format!(
            "Invalid API base URL '{}': {err}",
            normalized.as_str()
        ))
    })?;
    let original_query = url.query().map(str::to_string);
    let mut full_url = url.join(suffix.trim_start_matches('/')).map_err(|err| {
        AppError::Config(format!(
            "Failed to join API path '{}' to '{}': {err}",
            suffix, normalized
        ))
    })?;
    full_url.set_query(original_query.as_deref());
    Ok(full_url.to_string())
}

pub fn normalize_storage_endpoint(value: &str) -> String {
    value.trim().trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_base_url_is_directory_url() {
        assert_eq!(
            normalize_api_base_url("https://api.example.com/v1").unwrap(),
            "https://api.example.com/v1/"
        );
        assert_eq!(
            normalize_api_base_url("https://api.example.com/v1/").unwrap(),
            "https://api.example.com/v1/"
        );
    }

    #[test]
    fn api_base_url_preserves_query() {
        assert_eq!(
            normalize_api_base_url(" https://api.example.com/v1?api-version=1 ").unwrap(),
            "https://api.example.com/v1/?api-version=1"
        );
    }

    #[test]
    fn api_base_url_join_preserves_query() {
        assert_eq!(
            join_api_base_url("https://api.example.com/v1?api-version=1", "models").unwrap(),
            "https://api.example.com/v1/models?api-version=1"
        );
    }

    #[test]
    fn storage_endpoint_trims_whitespace_and_trailing_slashes() {
        assert_eq!(
            normalize_storage_endpoint(" https://dav.example.com/remote.php/webdav// "),
            "https://dav.example.com/remote.php/webdav"
        );
    }
}
