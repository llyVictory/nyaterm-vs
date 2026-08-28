use crate::config::{AiPermissionMode, RiskLevel};

use super::CapabilityAccess;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyDecision {
    Allow,
    RequireApproval,
    Deny,
}

#[derive(Debug, Clone)]
pub struct CommandRisk {
    pub level: RiskLevel,
    pub reason: String,
}

pub fn decide_policy(
    mode: &AiPermissionMode,
    access: CapabilityAccess,
    command_risk: Option<&RiskLevel>,
) -> PolicyDecision {
    if matches!(
        access,
        CapabilityAccess::Write | CapabilityAccess::DestructiveWrite
    ) && *mode == AiPermissionMode::Observer
    {
        return PolicyDecision::Deny;
    }
    if access == CapabilityAccess::DestructiveWrite {
        return PolicyDecision::RequireApproval;
    }
    if command_risk.is_some_and(|risk| *risk >= RiskLevel::High) {
        return PolicyDecision::RequireApproval;
    }
    match (mode, access) {
        (_, CapabilityAccess::Read) => PolicyDecision::Allow,
        (AiPermissionMode::Auto, CapabilityAccess::SensitiveRead | CapabilityAccess::Write) => {
            PolicyDecision::Allow
        }
        (_, CapabilityAccess::SensitiveRead | CapabilityAccess::Write) => {
            PolicyDecision::RequireApproval
        }
        (_, CapabilityAccess::DestructiveWrite) => PolicyDecision::RequireApproval,
    }
}

pub fn assess_command_risk(command: &str) -> CommandRisk {
    let normalized = command
        .trim()
        .replace("\r\n", "\n")
        .replace('\n', " ")
        .to_ascii_lowercase();
    let compact = normalized.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.is_empty() {
        return risk(RiskLevel::Medium, "empty command");
    }
    if is_root_rm_command(&compact)
        || (compact.starts_with("dd ") && compact.contains("of=/dev/"))
        || contains_any(
            &compact,
            &[
                "mkfs",
                "wipefs",
                ":(){",
                "shutdown",
                "poweroff",
                "reboot",
                "halt",
                "systemctl stop ssh",
                "systemctl stop sshd",
                "service ssh stop",
                "service sshd stop",
            ],
        )
    {
        return risk(
            RiskLevel::Critical,
            "matches irreversible or system-disruptive command pattern",
        );
    }
    if compact.starts_with("sudo ")
        || contains_any(
            &compact,
            &[
                "rm -r",
                "rm -f",
                " rmdir ",
                " chmod -r",
                " chown -r",
                "systemctl restart",
                "systemctl stop",
                "service ",
                "apt install",
                "apt remove",
                "apt purge",
                "yum install",
                "yum remove",
                "dnf install",
                "dnf remove",
                "pacman -s",
                "pacman -r",
                "brew install",
                "brew uninstall",
                "npm install -g",
                "pip install",
                "docker rm",
                "docker rmi",
                "docker system prune",
                "kubectl delete",
                "kubectl drain",
                "kubectl apply",
                "kubectl replace",
                "git reset --hard",
                "git clean -fd",
            ],
        )
    {
        return risk(
            RiskLevel::High,
            "matches privileged, destructive, restart, package, container, or cluster mutation pattern",
        );
    }
    if contains_any(
        &format!(" {compact} "),
        &[
            " > ",
            ">>",
            " tee ",
            " touch ",
            " mkdir ",
            " cp ",
            " mv ",
            " chmod ",
            " chown ",
            " setfacl ",
            " export ",
            "git checkout",
            "git switch",
            "git pull",
            "git merge",
            "npm run",
            "make install",
        ],
    ) {
        return risk(
            RiskLevel::Medium,
            "matches local write or state-changing command pattern",
        );
    }
    let readonly = [
        "ls",
        "pwd",
        "whoami",
        "id",
        "uname",
        "cat",
        "less",
        "head",
        "tail",
        "grep",
        "rg",
        "find",
        "df",
        "du",
        "free",
        "top",
        "ps",
        "ss",
        "netstat",
        "ip ",
        "journalctl",
        "systemctl status",
        "docker ps",
        "docker logs",
        "kubectl get",
        "kubectl describe",
        "git status",
        "git log",
        "git diff",
    ];
    if readonly
        .iter()
        .any(|prefix| compact == prefix.trim() || compact.starts_with(&format!("{prefix} ")))
    {
        return risk(RiskLevel::Low, "matches read-only diagnostic pattern");
    }
    risk(
        RiskLevel::Medium,
        "no explicit read-only pattern matched; defaulting to medium",
    )
}

fn risk(level: RiskLevel, reason: &str) -> CommandRisk {
    CommandRisk {
        level,
        reason: reason.to_string(),
    }
}

fn contains_any(command: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|pattern| command.contains(pattern))
}

fn is_root_rm_command(command: &str) -> bool {
    let tokens = command.split_whitespace().collect::<Vec<_>>();
    tokens.first() == Some(&"rm")
        && tokens
            .iter()
            .any(|token| token.starts_with('-') && token.contains('r') && token.contains('f'))
        && tokens
            .iter()
            .skip(1)
            .any(|token| matches!(*token, "/" | "/*" | "--no-preserve-root"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_matrix_is_conservative() {
        assert_eq!(
            decide_policy(&AiPermissionMode::Observer, CapabilityAccess::Write, None),
            PolicyDecision::Deny
        );
        assert_eq!(
            decide_policy(
                &AiPermissionMode::Confirm,
                CapabilityAccess::SensitiveRead,
                None
            ),
            PolicyDecision::RequireApproval
        );
        assert_eq!(
            decide_policy(
                &AiPermissionMode::Auto,
                CapabilityAccess::Write,
                Some(&RiskLevel::Low)
            ),
            PolicyDecision::Allow
        );
        assert_eq!(
            decide_policy(
                &AiPermissionMode::Auto,
                CapabilityAccess::Write,
                Some(&RiskLevel::High)
            ),
            PolicyDecision::RequireApproval
        );
        assert_eq!(
            decide_policy(
                &AiPermissionMode::Auto,
                CapabilityAccess::DestructiveWrite,
                None
            ),
            PolicyDecision::RequireApproval
        );
    }

    #[test]
    fn risk_matches_existing_protections() {
        assert_eq!(assess_command_risk("ls -la").level, RiskLevel::Low);
        assert_eq!(
            assess_command_risk("systemctl restart nginx").level,
            RiskLevel::High
        );
        assert_eq!(assess_command_risk("rm -rf /").level, RiskLevel::Critical);
    }
}
