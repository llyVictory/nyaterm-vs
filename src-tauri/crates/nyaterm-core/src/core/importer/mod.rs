//! Import sessions from Xshell (.xts), MobaXterm (.mxtsessions), WindTerm (.sessions),
//! SecureCRT (.xml), FinalShell conn directories, NyaTerm JSON files, and Electerm bookmarks.

use crate::config::{
    self, AiExecutionProfile, ConnectionAuth, ConnectionType, Group, SavedConnection,
};
use crate::error::{AppError, AppResult};
#[cfg(not(test))]
use crate::utils::crypto;
use serde::Deserialize;
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

#[cfg(not(test))]
fn encrypt_import_secret(plaintext: &str) -> AppResult<String> {
    crypto::encrypt(plaintext)
}

#[cfg(test)]
fn encrypt_import_secret(plaintext: &str) -> AppResult<String> {
    Ok(format!("test-encrypted:{plaintext}"))
}

include!("types.rs");
include!("text.rs");
include!("common.rs");
include!("xshell.rs");
include!("mobaxterm.rs");
include!("windterm.rs");
include!("securecrt.rs");
include!("finalshell.rs");
include!("nyaterm_json.rs");
include!("electerm.rs");
#[cfg(feature = "desktop-importer")]
include!("termius.rs");
include!("merge.rs");
include!("tests.rs");

#[derive(serde::Serialize)]
pub struct BrowserImportResult {
    pub imported: usize,
    pub warnings: Vec<String>,
}

/// Content-only import: never resolves paths from uploaded files on the server.
pub fn import_browser_connections(source: &str, bytes: &[u8]) -> AppResult<BrowserImportResult> {
    if bytes.len() > 10 * 1024 * 1024 {
        return Err(AppError::Config("Import exceeds 10 MiB".into()));
    }
    let content = decode_bytes(bytes);
    let mut warnings = Vec::new();
    let imported = match source {
        "xshell" => {
            warnings.push("credentials_not_in_export".into());
            import_legacy_sessions(&(), parse_xshell_reader(std::io::Cursor::new(bytes))?)?
        }
        "mobaxterm" => {
            warnings.push("credentials_not_in_export".into());
            import_legacy_sessions(&(), parse_mobaxterm_bytes(bytes)?)?
        }
        "securecrt" => {
            warnings.push("credentials_not_in_export".into());
            import_legacy_sessions(&(), parse_securecrt_content(&content)?)?
        }
        "windterm" => {
            let entries: Vec<serde_json::Value> = serde_json::from_str(&content)?;
            // Encrypted auto-login and external key files require Desktop's profile resources.
            for entry in &entries {
                let login = entry.get("session.autoLogin");
                let has_encrypted = login.and_then(|v| v.as_str()).is_some_and(|v| {
                    !v.trim().is_empty() && serde_json::from_str::<serde_json::Value>(v).is_err()
                });
                let parsed_login = login
                    .and_then(|v| v.as_str())
                    .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());
                let login_value = parsed_login.as_ref().or(login);
                let has_key = entry.as_object().is_some_and(|fields| {
                    fields.iter().any(|(key, value)| {
                        key.starts_with("ssh.identityFilePath")
                            && value.as_str().is_some_and(|path| !path.trim().is_empty())
                    })
                }) || login_value
                    .and_then(|v| v.get("Public Key"))
                    .and_then(|v| v.as_object())
                    .is_some_and(|fields| {
                        fields.iter().any(|(key, value)| {
                            key.ends_with(".path")
                                && value.as_str().is_some_and(|path| !path.trim().is_empty())
                        })
                    });
                if has_encrypted || has_key {
                    return Err(AppError::Config("WindTerm profile resources required; import on Desktop and transfer a .nya backup".into()));
                }
            }
            import_prepared_nyaterm_json(
                &(),
                parse_windterm_content_impl(&content, None, None, false)?,
            )?
        }
        "electerm" => import_prepared_nyaterm_json(&(), parse_electerm_json_content(&content)?)?,
        "nyaterm_json" => import_prepared_nyaterm_json(&(), parse_nyaterm_json_content(&content)?)?,
        _ => {
            return Err(AppError::Config(
                "This import source requires Desktop".into(),
            ));
        }
    };
    Ok(BrowserImportResult { imported, warnings })
}
