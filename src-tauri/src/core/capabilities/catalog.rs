use nyaterm_mcp_protocol::{capability, tool};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityAccess {
    Read,
    SensitiveRead,
    Write,
    DestructiveWrite,
}

#[derive(Debug, Clone, Copy)]
pub struct CapabilityDefinition {
    pub id: &'static str,
    pub mcp_tool: Option<&'static str>,
    pub access: CapabilityAccess,
    pub requires_session: bool,
}

pub const CATALOG: &[CapabilityDefinition] = &[
    CapabilityDefinition {
        id: capability::ENVIRONMENT,
        mcp_tool: Some(tool::GET_ENVIRONMENT),
        access: CapabilityAccess::Read,
        requires_session: false,
    },
    CapabilityDefinition {
        id: capability::SESSION_GET,
        mcp_tool: Some(tool::SESSION_GET),
        access: CapabilityAccess::Read,
        requires_session: true,
    },
    CapabilityDefinition {
        id: capability::TERMINAL_EXECUTE,
        mcp_tool: Some(tool::TERMINAL_EXECUTE),
        access: CapabilityAccess::Write,
        requires_session: true,
    },
    CapabilityDefinition {
        id: capability::TERMINAL_RECENT_OUTPUT,
        mcp_tool: Some(tool::TERMINAL_RECENT_OUTPUT),
        access: CapabilityAccess::SensitiveRead,
        requires_session: true,
    },
    CapabilityDefinition {
        id: capability::SFTP_HOME,
        mcp_tool: Some(tool::SFTP_HOME),
        access: CapabilityAccess::SensitiveRead,
        requires_session: true,
    },
    CapabilityDefinition {
        id: capability::SFTP_LIST,
        mcp_tool: Some(tool::SFTP_LIST),
        access: CapabilityAccess::SensitiveRead,
        requires_session: true,
    },
    CapabilityDefinition {
        id: capability::SFTP_STAT,
        mcp_tool: Some(tool::SFTP_STAT),
        access: CapabilityAccess::SensitiveRead,
        requires_session: true,
    },
    CapabilityDefinition {
        id: capability::SFTP_READ,
        mcp_tool: Some(tool::SFTP_READ_TEXT),
        access: CapabilityAccess::SensitiveRead,
        requires_session: true,
    },
    CapabilityDefinition {
        id: capability::SFTP_WRITE,
        mcp_tool: Some(tool::SFTP_WRITE_TEXT),
        access: CapabilityAccess::Write,
        requires_session: true,
    },
    CapabilityDefinition {
        id: capability::SFTP_MKDIR,
        mcp_tool: Some(tool::SFTP_MKDIR),
        access: CapabilityAccess::Write,
        requires_session: true,
    },
    CapabilityDefinition {
        id: capability::SFTP_RENAME,
        mcp_tool: Some(tool::SFTP_RENAME),
        access: CapabilityAccess::Write,
        requires_session: true,
    },
    CapabilityDefinition {
        id: capability::SFTP_DELETE,
        mcp_tool: Some(tool::SFTP_DELETE),
        access: CapabilityAccess::DestructiveWrite,
        requires_session: true,
    },
    CapabilityDefinition {
        id: capability::SFTP_CHMOD,
        mcp_tool: Some(tool::SFTP_CHMOD),
        access: CapabilityAccess::Write,
        requires_session: true,
    },
    CapabilityDefinition {
        id: capability::OUTPUT_READ,
        mcp_tool: Some(tool::OUTPUT_READ),
        access: CapabilityAccess::SensitiveRead,
        requires_session: false,
    },
];

#[cfg(test)]
fn capability_by_id(id: &str) -> Option<&'static CapabilityDefinition> {
    CATALOG.iter().find(|definition| definition.id == id)
}

pub fn capability_for_tool(name: &str) -> Option<&'static CapabilityDefinition> {
    CATALOG
        .iter()
        .find(|definition| definition.mcp_tool == Some(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_is_destructive_and_all_tools_are_unique() {
        assert_eq!(
            capability_by_id(capability::SFTP_DELETE).unwrap().access,
            CapabilityAccess::DestructiveWrite
        );
        let mut tools = CATALOG
            .iter()
            .filter_map(|item| item.mcp_tool)
            .collect::<Vec<_>>();
        let before = tools.len();
        tools.sort_unstable();
        tools.dedup();
        assert_eq!(tools.len(), before);
    }
}
