// Copyright 2025, The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.//! Common MCP (Model Context Protocol)
// infrastructure for Tari applications
//! Transport layer for MCP communication

use std::{collections::HashSet, fmt, net::SocketAddr};

use serde::{
    Deserialize,
    Deserializer,
    Serialize,
    de::{DeserializeSeed, Error as _, MapAccess, SeqAccess, Visitor},
};
use serde_json::Value;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
};

use crate::error::{McpError, McpResult};

/// Refuse a JSON text in which any object repeats a key. serde_json keeps only the last value, so someone reviewing the
/// first could approve a call that does something else. Malformed JSON is left for the parser to report.
pub fn check_duplicate_keys(text: &str) -> McpResult<()> {
    let mut deserializer = serde_json::Deserializer::from_str(text);
    match NoDuplicateKeys.deserialize(&mut deserializer) {
        Err(e) if e.is_data() => Err(McpError::invalid_request(e.to_string())),
        _ => Ok(()),
    }
}

/// Walks a JSON value once, keeping only the keys of the object being read
struct NoDuplicateKeys;

impl<'de> DeserializeSeed<'de> for NoDuplicateKeys {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for NoDuplicateKeys {
    type Value = ();

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a JSON value")
    }

    fn visit_bool<E>(self, _: bool) -> Result<(), E> {
        Ok(())
    }

    fn visit_i64<E>(self, _: i64) -> Result<(), E> {
        Ok(())
    }

    fn visit_u64<E>(self, _: u64) -> Result<(), E> {
        Ok(())
    }

    fn visit_f64<E>(self, _: f64) -> Result<(), E> {
        Ok(())
    }

    fn visit_str<E>(self, _: &str) -> Result<(), E> {
        Ok(())
    }

    fn visit_unit<E>(self) -> Result<(), E> {
        Ok(())
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        while seq.next_element_seed(NoDuplicateKeys)?.is_some() {}
        Ok(())
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        let mut keys = HashSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if keys.contains(&key) {
                return Err(A::Error::custom(format!("duplicate key {key:?}")));
            }
            map.next_value_seed(NoDuplicateKeys)?;
            keys.insert(key);
        }
        Ok(())
    }
}

/// MCP message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method")]
pub enum McpMessage {
    #[serde(rename = "tools/list")]
    ListTools { id: Value },

    #[serde(rename = "tools/call")]
    CallTool { id: Value, params: ToolCallParams },

    #[serde(rename = "resources/list")]
    ListResources { id: Value },

    #[serde(rename = "resources/read")]
    ReadResource { id: Value, params: ResourceReadParams },

    #[serde(rename = "prompts/list")]
    ListPrompts { id: Value },

    #[serde(rename = "prompts/get")]
    GetPrompt { id: Value, params: PromptGetParams },

    #[serde(rename = "ping")]
    Ping { id: Value },

    #[serde(rename = "initialize")]
    Initialize { id: Value, params: InitializeParams },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallParams {
    pub name: String,
    pub arguments: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceReadParams {
    pub uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptGetParams {
    pub name: String,
    pub arguments: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeParams {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub capabilities: ClientCapabilities,
    #[serde(rename = "clientInfo")]
    pub client_info: ClientInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientCapabilities {
    pub roots: Option<RootsCapability>,
    pub sampling: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootsCapability {
    #[serde(rename = "listChanged")]
    pub list_changed: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

/// MCP response message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResponse {
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpErrorResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpErrorResponse {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Transport handler trait
#[async_trait::async_trait]
pub trait Transport: Send + Sync {
    async fn listen(&self, addr: SocketAddr) -> McpResult<()>;
    async fn handle_connection(&self, stream: TcpStream) -> McpResult<()>;
}

use std::sync::Arc;

/// JSON-RPC over TCP transport
pub struct JsonRpcTransport {
    message_handler: Arc<dyn MessageHandler>,
}

/// Message handler trait
#[async_trait::async_trait]
pub trait MessageHandler: Send + Sync {
    async fn handle_message(&self, message: McpMessage) -> McpResult<McpResponse>;
}

impl JsonRpcTransport {
    pub fn new(message_handler: Arc<dyn MessageHandler>) -> Self {
        Self { message_handler }
    }
}

#[async_trait::async_trait]
impl Transport for JsonRpcTransport {
    async fn listen(&self, addr: SocketAddr) -> McpResult<()> {
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| McpError::transport_error(format!("Failed to bind to {addr}: {e}")))?;

        log::info!("MCP server listening on {}", addr);

        loop {
            match listener.accept().await {
                Ok((stream, peer_addr)) => {
                    log::debug!("New connection from {peer_addr}");

                    // Verify connection is from localhost
                    if !peer_addr.ip().is_loopback() {
                        log::warn!("Rejected non-localhost connection from {peer_addr}");
                        continue;
                    }

                    let handler_clone = Arc::clone(&self.message_handler);
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection_static(handler_clone.as_ref(), stream).await {
                            log::error!("Connection error: {e}");
                        }
                    });
                },
                Err(e) => {
                    log::error!("Failed to accept connection: {e}");
                },
            }
        }
    }

    async fn handle_connection(&self, stream: TcpStream) -> McpResult<()> {
        Self::handle_connection_static(self.message_handler.as_ref(), stream).await
    }
}

impl JsonRpcTransport {
    async fn handle_connection_static(handler: &dyn MessageHandler, mut stream: TcpStream) -> McpResult<()> {
        let (reader, mut writer) = stream.split();
        let mut reader = BufReader::new(reader);
        let mut line = String::new();

        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => {
                    // Connection closed
                    log::debug!("Connection closed by client");
                    break;
                },
                Ok(_) => {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }

                    log::debug!("Received message: {line}");

                    // Parse JSON-RPC message
                    let parsed = check_duplicate_keys(line)
                        .and_then(|()| serde_json::from_str::<McpMessage>(line).map_err(McpError::from));
                    let response = match parsed {
                        Ok(message) => {
                            // Handle the message
                            handler.handle_message(message).await
                        },
                        Err(e) => {
                            log::warn!("Failed to parse message: {e}");
                            Err(McpError::invalid_request(format!("Invalid JSON-RPC: {e}")))
                        },
                    };

                    // Send response
                    let response_json = match response {
                        Ok(resp) => serde_json::to_string(&resp).map_err(McpError::serialization_error)?,
                        Err(e) => {
                            let error_response = McpResponse {
                                id: Value::Null,
                                result: None,
                                error: Some(McpErrorResponse {
                                    code: match &e {
                                        McpError::PermissionDenied(_) => -32000,
                                        McpError::InvalidRequest(_) => -32600,
                                        McpError::ToolNotFound(_) | McpError::ResourceNotFound(_) => -32601,
                                        _ => -32603,
                                    },
                                    message: e.to_string(),
                                    data: None,
                                }),
                            };
                            serde_json::to_string(&error_response).map_err(McpError::serialization_error)?
                        },
                    };

                    writer
                        .write_all(response_json.as_bytes())
                        .await
                        .map_err(|e| McpError::transport_error(format!("Write failed: {e}")))?;
                    writer
                        .write_all(b"\n")
                        .await
                        .map_err(|e| McpError::transport_error(format!("Write failed: {e}")))?;
                    writer
                        .flush()
                        .await
                        .map_err(|e| McpError::transport_error(format!("Flush failed: {e}")))?;

                    log::debug!("Sent response: {response_json}");
                },
                Err(e) => {
                    log::error!("Read error: {e}");
                    return Err(McpError::transport_error(format!("Read failed: {e}")));
                },
            }
        }

        Ok(())
    }
}

impl McpError {
    pub fn transport_error(msg: impl Into<String>) -> Self {
        Self::TransportError(msg.into())
    }

    pub fn serialization_error(err: serde_json::Error) -> Self {
        Self::SerializationError(err)
    }
}
