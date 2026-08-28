use std::path::{Path, PathBuf};

use nyaterm_mcp_protocol::DiscoveryDocument;

use crate::error::{AppError, AppResult};

pub struct DiscoveryStore {
    directory: PathBuf,
    file: PathBuf,
}

impl DiscoveryStore {
    pub fn new(config_dir: &Path) -> Self {
        let directory = config_dir.join("mcp");
        let file = directory.join("discovery.json");
        Self { directory, file }
    }

    pub fn remove(&self) -> AppResult<()> {
        match std::fs::remove_file(&self.file) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        if let Ok(entries) = std::fs::read_dir(&self.directory) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with(".discovery-") && name.ends_with(".tmp") {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
        Ok(())
    }

    pub fn write(&self, document: &DiscoveryDocument) -> AppResult<()> {
        std::fs::create_dir_all(&self.directory)?;
        set_private_directory_permissions(&self.directory)?;
        let temporary = self
            .directory
            .join(format!(".discovery-{}.tmp", uuid::Uuid::new_v4()));
        let bytes = serde_json::to_vec_pretty(document)?;
        std::fs::write(&temporary, bytes)?;
        if let Err(error) = set_private_file_permissions(&temporary) {
            let _ = std::fs::remove_file(&temporary);
            return Err(error);
        }

        atomic_replace(&temporary, &self.file).map_err(|error| {
            let _ = std::fs::remove_file(&temporary);
            AppError::Io(error)
        })?;
        set_private_file_permissions(&self.file)
    }
}

#[cfg(not(windows))]
fn atomic_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::fs::rename(source, destination)
}

#[cfg(windows)]
fn atomic_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn set_private_directory_permissions(path: &Path) -> AppResult<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(unix)]
fn set_private_file_permissions(path: &Path) -> AppResult<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(windows)]
fn set_private_directory_permissions(path: &Path) -> AppResult<()> {
    set_windows_current_user_acl(path, true)
}

#[cfg(windows)]
fn set_private_file_permissions(path: &Path) -> AppResult<()> {
    set_windows_current_user_acl(path, false)
}

#[cfg(windows)]
fn set_windows_current_user_acl(path: &Path, directory: bool) -> AppResult<()> {
    use std::os::windows::process::CommandExt;

    // Build a protected DACL from the current token SID instead of a user name. Starting
    // from a fresh ACL also removes explicit grants that may have existed on a stale runtime
    // directory, so another local account cannot inherit or retain access to the credential.
    const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$target = $args[0]
$isDirectory = $args[1] -eq '1'
$sid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
$acl = if ($isDirectory) {
  [System.Security.AccessControl.DirectorySecurity]::new()
} else {
  [System.Security.AccessControl.FileSecurity]::new()
}
$acl.SetOwner($sid)
$acl.SetAccessRuleProtection($true, $false)
$inheritance = if ($isDirectory) {
  [System.Security.AccessControl.InheritanceFlags]::ContainerInherit -bor [System.Security.AccessControl.InheritanceFlags]::ObjectInherit
} else {
  [System.Security.AccessControl.InheritanceFlags]::None
}
$rule = [System.Security.AccessControl.FileSystemAccessRule]::new(
  $sid,
  [System.Security.AccessControl.FileSystemRights]::FullControl,
  $inheritance,
  [System.Security.AccessControl.PropagationFlags]::None,
  [System.Security.AccessControl.AccessControlType]::Allow
)
$acl.SetAccessRule($rule)
Set-Acl -LiteralPath $target -AclObject $acl
"#;
    let output = std::process::Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            SCRIPT,
        ])
        .arg(path)
        .arg(if directory { "1" } else { "0" })
        .creation_flags(0x0800_0000)
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(AppError::Config(
            "Failed to apply a current-user-only ACL to MCP discovery data.".into(),
        ))
    }
}

#[cfg(not(any(unix, windows)))]
fn set_private_directory_permissions(_path: &Path) -> AppResult<()> {
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn set_private_file_permissions(_path: &Path) -> AppResult<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_discovery_can_be_removed_repeatedly() {
        let root = std::env::temp_dir().join(format!("nyaterm-mcp-test-{}", uuid::Uuid::new_v4()));
        let store = DiscoveryStore::new(&root);
        store.remove().unwrap();
        store.remove().unwrap();
        let _ = std::fs::remove_dir_all(root);
    }
}
