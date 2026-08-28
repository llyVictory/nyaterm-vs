use std::path::PathBuf;
use std::sync::Arc;

use nyaterm_mcp_protocol::{
    AuthParams, CapabilityExecuteParams, ClientIdentifyParams, DiscoveryDocument, PROTOCOL_VERSION,
    RpcError, RpcRequest, RpcResponse,
};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct BridgeEndpoint {
    host: String,
    port: u16,
    token: String,
    generation: String,
}

impl BridgeEndpoint {
    #[cfg(test)]
    pub(crate) fn for_test(port: u16) -> Self {
        Self {
            host: "127.0.0.1".into(),
            port,
            token: "test-token".into(),
            generation: "test-generation".into(),
        }
    }
}

struct Connection {
    reader: BufReader<tokio::net::tcp::OwnedReadHalf>,
    writer: tokio::net::tcp::OwnedWriteHalf,
    next_id: u64,
}

#[derive(Clone)]
pub struct BridgeClient {
    endpoint: BridgeEndpoint,
    connection: Arc<Mutex<Option<Connection>>>,
}

impl BridgeClient {
    pub fn new(endpoint: BridgeEndpoint) -> Self {
        Self {
            endpoint,
            connection: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn identify(&self, name: String, version: Option<String>) {
        let params =
            serde_json::to_value(ClientIdentifyParams { name, version }).unwrap_or_default();
        let _ = self.rpc("client.identify", params).await;
    }

    pub async fn call(
        &self,
        name: &str,
        arguments: Value,
        cancellation: CancellationToken,
    ) -> Result<Value, RpcError> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let params = serde_json::to_value(CapabilityExecuteParams {
            request_id: Some(request_id.clone()),
            tool: name.to_string(),
            arguments,
        })
        .map_err(|error| bridge_error("invalid_argument", &error.to_string()))?;
        let mut guard = self.connection.lock().await;
        if guard.is_none() {
            *guard = Some(connect(&self.endpoint).await.map_err(io_error)?);
        }
        let connection = guard.as_mut().unwrap();
        let id = connection.next_id;
        connection.next_id += 1;
        write_request(
            connection,
            RpcRequest {
                id,
                method: "capability.execute".into(),
                params,
            },
        )
        .await
        .map_err(io_error)?;
        let response = tokio::select! {
            _ = cancellation.cancelled() => {
                let endpoint = self.endpoint.clone();
                tokio::spawn(async move { let _ = cancel_request(&endpoint, &request_id).await; });
                *guard = None;
                return Err(bridge_error("cancelled", "The MCP tool call was cancelled."));
            }
            response = read_response(connection) => response.map_err(io_error)?,
        };
        if response.id != id {
            *guard = None;
            return Err(bridge_error(
                "bridge_disconnected",
                "MCP bridge response ID mismatch.",
            ));
        }
        match (response.result, response.error) {
            (Some(value), None) => Ok(value),
            (_, Some(error)) => Err(error),
            _ => Err(bridge_error(
                "bridge_disconnected",
                "MCP bridge returned an empty response.",
            )),
        }
    }

    async fn rpc(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        let mut guard = self.connection.lock().await;
        if guard.is_none() {
            *guard = Some(connect(&self.endpoint).await.map_err(io_error)?);
        }
        let connection = guard.as_mut().unwrap();
        let id = connection.next_id;
        connection.next_id += 1;
        write_request(
            connection,
            RpcRequest {
                id,
                method: method.into(),
                params,
            },
        )
        .await
        .map_err(io_error)?;
        let response = read_response(connection).await.map_err(io_error)?;
        response
            .error
            .map_or_else(|| Ok(response.result.unwrap_or(Value::Null)), Err)
    }
}

async fn connect(endpoint: &BridgeEndpoint) -> std::io::Result<Connection> {
    let stream = TcpStream::connect((endpoint.host.as_str(), endpoint.port)).await?;
    let (read, write) = stream.into_split();
    let mut connection = Connection {
        reader: BufReader::new(read),
        writer: write,
        next_id: 2,
    };
    let params = serde_json::to_value(AuthParams {
        token: endpoint.token.clone(),
        generation: endpoint.generation.clone(),
    })
    .map_err(std::io::Error::other)?;
    write_request(
        &mut connection,
        RpcRequest {
            id: 1,
            method: "auth".into(),
            params,
        },
    )
    .await?;
    if read_response(&mut connection).await?.error.is_some() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "NyaTerm MCP authentication failed",
        ));
    }
    Ok(connection)
}

async fn cancel_request(endpoint: &BridgeEndpoint, request_id: &str) -> std::io::Result<()> {
    let mut connection = connect(endpoint).await?;
    write_request(
        &mut connection,
        RpcRequest {
            id: 2,
            method: "request.cancel".into(),
            params: json!({ "requestId": request_id }),
        },
    )
    .await?;
    let _ = read_response(&mut connection).await?;
    Ok(())
}

async fn write_request(connection: &mut Connection, request: RpcRequest) -> std::io::Result<()> {
    let mut bytes = serde_json::to_vec(&request).map_err(std::io::Error::other)?;
    bytes.push(b'\n');
    connection.writer.write_all(&bytes).await
}

async fn read_response(connection: &mut Connection) -> std::io::Result<RpcResponse> {
    let mut line = String::new();
    if connection.reader.read_line(&mut line).await? == 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "NyaTerm MCP bridge disconnected",
        ));
    }
    serde_json::from_str(line.trim_end()).map_err(std::io::Error::other)
}

pub fn endpoint_from_environment_or_discovery() -> Result<BridgeEndpoint, Box<dyn std::error::Error>>
{
    let ephemeral = std::env::var("NYATERM_MCP_EPHEMERAL").as_deref() == Ok("1");
    let host = std::env::var("NYATERM_MCP_HOST").ok();
    let port = std::env::var("NYATERM_MCP_PORT")
        .ok()
        .and_then(|value| value.parse().ok());
    let token = std::env::var("NYATERM_MCP_TOKEN").ok();
    let generation = std::env::var("NYATERM_MCP_GENERATION").ok();
    if let (Some(host), Some(port), Some(token), Some(generation)) = (host, port, token, generation)
    {
        if host != "127.0.0.1" {
            return Err("NyaTerm MCP bridge host must be 127.0.0.1".into());
        }
        return Ok(BridgeEndpoint {
            host,
            port,
            token,
            generation,
        });
    }
    if ephemeral {
        return Err("NyaTerm ephemeral MCP credential is incomplete or unavailable".into());
    }
    let document: DiscoveryDocument = serde_json::from_slice(&std::fs::read(discovery_path()?)?)?;
    if document.version != PROTOCOL_VERSION || document.host != "127.0.0.1" {
        return Err("Unsupported or unsafe NyaTerm MCP discovery document".into());
    }
    Ok(BridgeEndpoint {
        host: document.host,
        port: document.port,
        token: document.token,
        generation: document.generation,
    })
}

fn discovery_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(path) = std::env::var_os("NYATERM_MCP_DISCOVERY") {
        return Ok(path.into());
    }
    let executable = std::env::current_exe()?;
    let directory = executable
        .parent()
        .ok_or("Cannot resolve sidecar directory")?;
    if directory.join("portable.flag").is_file() {
        return Ok(directory
            .join("data")
            .join("config")
            .join("mcp")
            .join("discovery.json"));
    }
    Ok(dirs::home_dir()
        .ok_or("Cannot resolve home directory")?
        .join(".nyaterm")
        .join("mcp")
        .join("discovery.json"))
}

fn io_error(error: std::io::Error) -> RpcError {
    bridge_error("bridge_disconnected", &error.to_string())
}
fn bridge_error(code: &str, message: &str) -> RpcError {
    RpcError {
        code: code.into(),
        message: message.into(),
    }
}
