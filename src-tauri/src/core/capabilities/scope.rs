use std::collections::HashSet;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct McpScope {
    pub session_ids: HashSet<String>,
    pub default_session_id: Option<String>,
}

impl McpScope {
    pub fn new(
        session_ids: impl IntoIterator<Item = String>,
        default_session_id: Option<String>,
    ) -> Self {
        let session_ids = session_ids.into_iter().collect::<HashSet<_>>();
        let default_session_id = default_session_id.filter(|id| session_ids.contains(id));
        Self {
            session_ids,
            default_session_id,
        }
    }

    pub fn require(&self, session_id: &str) -> AppResult<()> {
        if self.session_ids.contains(session_id) {
            Ok(())
        } else {
            Err(AppError::Config(
                "Session is not available in the current MCP scope.".to_string(),
            ))
        }
    }

    pub fn resolve_terminal_session(&self, requested: Option<&str>) -> AppResult<String> {
        if let Some(id) = requested.filter(|id| !id.trim().is_empty()) {
            self.require(id)?;
            return Ok(id.to_string());
        }
        self.default_session_id.clone().ok_or_else(|| {
            AppError::Config(
                "No default session is available; provide sessionId explicitly.".to_string(),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_only_scoped_default() {
        let scope = McpScope::new(["a".into(), "b".into()], Some("a".into()));
        assert_eq!(scope.resolve_terminal_session(None).unwrap(), "a");
        assert!(scope.resolve_terminal_session(Some("c")).is_err());
    }

    #[test]
    fn multiple_sessions_without_default_require_explicit_id() {
        let scope = McpScope::new(["a".into(), "b".into()], None);
        assert!(scope.resolve_terminal_session(None).is_err());
    }
}
