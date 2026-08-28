mod bridge;

use std::sync::Arc;

use bridge::{BridgeClient, BridgeEndpoint, endpoint_from_environment_or_discovery};
use nyaterm_mcp_protocol::{
    EmptyArgs, OutputReadArgs, PathArgs, SessionArgs, SftpChmodArgs, SftpMkdirArgs,
    SftpReadTextArgs, SftpRenameArgs, SftpWriteTextArgs, TerminalExecuteArgs,
    TerminalRecentOutputArgs, tool,
};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, Implementation, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerInfo, Tool, ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler, ServiceExt};
use schemars::JsonSchema;
use serde_json::{Map, Value, json};

#[derive(Clone)]
struct NyaTermMcp {
    bridge: BridgeClient,
    tools: Arc<Vec<Tool>>,
}

impl NyaTermMcp {
    fn new(endpoint: BridgeEndpoint) -> Self {
        Self {
            bridge: BridgeClient::new(endpoint),
            tools: Arc::new(build_tools()),
        }
    }
}

impl ServerHandler for NyaTermMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "nyaterm-mcp",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions("Operate sessions already opened in NyaTerm. NyaTerm enforces session scope and approvals.")
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        if !self.tools.iter().any(|item| item.name == request.name) {
            return Err(McpError::invalid_params("Unknown NyaTerm tool", None));
        }
        let arguments = Value::Object(request.arguments.unwrap_or_default());
        let result = match self
            .bridge
            .call(&request.name, arguments, context.ct.clone())
            .await
        {
            Ok(value) => CallToolResult::structured(value),
            Err(error) => CallToolResult::structured_error(
                json!({ "code": error.code, "message": error.message }),
            ),
        };
        Ok(result.into())
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(self.tools.as_ref().clone()))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tools.iter().find(|item| item.name == name).cloned()
    }

    async fn on_initialized(&self, context: rmcp::service::NotificationContext<RoleServer>) {
        if let Some(info) = context.peer.peer_info() {
            self.bridge
                .identify(
                    info.client_info.name.to_string(),
                    Some(info.client_info.version.to_string()),
                )
                .await;
        }
    }
}

fn build_tools() -> Vec<Tool> {
    vec![
        tool_def::<EmptyArgs>(
            tool::GET_ENVIRONMENT,
            "Return scoped NyaTerm sessions and the optional default session.",
            true,
            false,
        ),
        tool_def::<SessionArgs>(
            tool::SESSION_GET,
            "Return safe metadata and capability availability for a scoped session.",
            true,
            false,
        ),
        tool_def::<TerminalExecuteArgs>(
            tool::TERMINAL_EXECUTE,
            "Execute a command in an existing scoped NyaTerm terminal session.",
            false,
            false,
        ),
        tool_def::<TerminalRecentOutputArgs>(
            tool::TERMINAL_RECENT_OUTPUT,
            "Read recent ANSI-free terminal output for a scoped session.",
            true,
            false,
        ),
        tool_def::<SessionArgs>(
            tool::SFTP_HOME,
            "Return the remote home directory.",
            true,
            false,
        ),
        tool_def::<PathArgs>(tool::SFTP_LIST, "List a remote directory.", true, false),
        tool_def::<PathArgs>(tool::SFTP_STAT, "Read remote path metadata.", true, false),
        tool_def::<SftpReadTextArgs>(
            tool::SFTP_READ_TEXT,
            "Read up to 64 KiB of a remote UTF-8 text file.",
            true,
            false,
        ),
        tool_def::<SftpWriteTextArgs>(
            tool::SFTP_WRITE_TEXT,
            "Write a remote UTF-8 text file with optional conflict protection.",
            false,
            false,
        ),
        tool_def::<SftpMkdirArgs>(tool::SFTP_MKDIR, "Create a remote directory.", false, false),
        tool_def::<SftpRenameArgs>(
            tool::SFTP_RENAME,
            "Rename or move a remote path.",
            false,
            false,
        ),
        tool_def::<PathArgs>(
            tool::SFTP_DELETE,
            "Delete a remote path using NyaTerm's existing delete semantics.",
            false,
            true,
        ),
        tool_def::<SftpChmodArgs>(
            tool::SFTP_CHMOD,
            "Change remote path permissions.",
            false,
            false,
        ),
        tool_def::<OutputReadArgs>(
            tool::OUTPUT_READ,
            "Read another chunk of a large result produced on this MCP connection.",
            true,
            false,
        ),
    ]
}

fn tool_def<T: JsonSchema>(
    name: &'static str,
    description: &'static str,
    read_only: bool,
    destructive: bool,
) -> Tool {
    let schema = serde_json::to_value(schemars::schema_for!(T))
        .unwrap_or_else(|_| json!({ "type": "object" }));
    let object = schema.as_object().cloned().unwrap_or_else(Map::new);
    let mut item = Tool::new(name, description, object);
    item.annotations = Some(
        ToolAnnotations::new()
            .read_only(read_only)
            .destructive(destructive)
            .open_world(false),
    );
    item
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = endpoint_from_environment_or_discovery()?;
    let server = NyaTermMcp::new(endpoint)
        .serve(rmcp::transport::stdio())
        .await?;
    server.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use nyaterm_mcp_protocol::{RpcRequest, RpcResponse};
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines, ReadHalf, WriteHalf},
        net::TcpListener,
    };

    use super::*;

    async fn send_client_message(writer: &mut WriteHalf<tokio::io::DuplexStream>, raw: &str) {
        let value = serde_json::from_str::<Value>(raw).expect("valid MCP client message");
        writer
            .write_all(&serde_json::to_vec(&value).unwrap())
            .await
            .unwrap();
        writer.write_all(b"\n").await.unwrap();
    }

    async fn receive_response(
        lines: &mut Lines<BufReader<ReadHalf<tokio::io::DuplexStream>>>,
        id: u64,
    ) -> Value {
        loop {
            let line = tokio::time::timeout(std::time::Duration::from_secs(5), lines.next_line())
                .await
                .unwrap_or_else(|_| panic!("timed out waiting for MCP response {id}"))
                .expect("server response")
                .expect("server transport remains open");
            let value: Value = serde_json::from_str(&line).expect("valid server response");
            if value.get("id").and_then(Value::as_u64) == Some(id) {
                return value;
            }
        }
    }

    #[tokio::test]
    async fn mcp_initialize_lists_tools_and_calls_mock_bridge() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let bridge_task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (reader, mut writer) = stream.into_split();
            let mut lines = BufReader::new(reader).lines();
            while let Some(line) = lines.next_line().await.unwrap() {
                let request: RpcRequest = serde_json::from_str(&line).unwrap();
                let result = match request.method.as_str() {
                    "auth" => {
                        assert_eq!(request.params["token"], "test-token");
                        assert_eq!(request.params["generation"], "test-generation");
                        json!({ "authenticated": true })
                    }
                    "client.identify" => {
                        assert_eq!(request.params["name"], "integration-test");
                        json!({ "identified": true })
                    }
                    "capability.execute" => {
                        assert_eq!(request.params["tool"], tool::GET_ENVIRONMENT);
                        json!({ "defaultSessionId": "session-1", "sessions": [] })
                    }
                    other => panic!("unexpected bridge method: {other}"),
                };
                let response = RpcResponse {
                    id: request.id,
                    result: Some(result),
                    error: None,
                };
                let mut bytes = serde_json::to_vec(&response).unwrap();
                bytes.push(b'\n');
                writer.write_all(&bytes).await.unwrap();
            }
        });

        let (server_transport, client_transport) = tokio::io::duplex(64 * 1024);
        let server_task = tokio::spawn(async move {
            let service = NyaTermMcp::new(BridgeEndpoint::for_test(port))
                .serve(server_transport)
                .await
                .unwrap();
            service.waiting().await.unwrap();
        });
        let (client_reader, mut client_writer) = tokio::io::split(client_transport);
        let mut client_lines = BufReader::new(client_reader).lines();

        send_client_message(
            &mut client_writer,
            r#"{
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "initialize",
                    "params": {
                        "protocolVersion": "2025-11-25",
                        "capabilities": {},
                        "clientInfo": { "name": "integration-test", "version": "1.0.0" }
                    }
                }"#,
        )
        .await;
        let initialized = receive_response(&mut client_lines, 1).await;
        assert_eq!(initialized["result"]["serverInfo"]["name"], "nyaterm-mcp");

        send_client_message(
            &mut client_writer,
            r#"{ "jsonrpc": "2.0", "method": "notifications/initialized" }"#,
        )
        .await;
        send_client_message(
            &mut client_writer,
            r#"{ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }"#,
        )
        .await;
        let listed = receive_response(&mut client_lines, 2).await;
        assert_eq!(listed["result"]["tools"].as_array().unwrap().len(), 14);

        send_client_message(
            &mut client_writer,
            r#"{
                    "jsonrpc": "2.0",
                    "id": 3,
                    "method": "tools/call",
                    "params": { "name": "get_environment", "arguments": {} }
                }"#,
        )
        .await;
        let called = receive_response(&mut client_lines, 3).await;
        assert_eq!(
            called["result"]["structuredContent"]["defaultSessionId"],
            "session-1"
        );

        drop(client_writer);
        drop(client_lines);
        server_task.abort();
        bridge_task.abort();
    }
}
