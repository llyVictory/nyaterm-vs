//! Web compatibility boundary. No untrusted plugin code executes on the server.
use serde_json::{Value, json};
pub fn capabilities() -> Value {
    json!({"runtime":"web","installation":false,"native":false,"server_process":false,"server_filesystem":false,"allowedPermissions":["ui"],"scope":"authenticated-browser"})
}
pub fn check(manifest: &Value) -> Value {
    let denied = manifest["permissions"]
        .as_array()
        .map(|permissions| {
            permissions
                .iter()
                .filter(|p| p.as_str() != Some("ui"))
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let browser_only =
        manifest["backend"].is_null() && denied.is_empty() && manifest["permissions"].is_array();
    json!({"runtime":"web","browserCompatible":browser_only,"deniedPermissions":denied,"installationAvailable":false})
}
