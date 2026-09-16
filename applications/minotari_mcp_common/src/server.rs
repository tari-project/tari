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
//! Core MCP server implementation

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::sync::RwLock;

use crate::{
    config::McpConfig,
    error::{McpError, McpResult},
    input_sanitizer::sanitize_tool_input,
    prompts::{PromptInfo, PromptRegistry},
    resources::{ResourceInfo, ResourceRegistry},
    security::SecurityContext,
    stdio_transport::StdioTransport,
    tools::{ToolInfo, ToolRegistry},
    transport::{
        InitializeParams,
        McpMessage,
        McpResponse,
        MessageHandler,
        PromptGetParams,
        ResourceReadParams,
        ToolCallParams,
    },
};

/// Core MCP server trait
#[async_trait]
pub trait McpServer: Send + Sync {
    async fn start(&self) -> McpResult<()>;
    async fn stop(&self) -> McpResult<()>;
    fn is_running(&self) -> bool;
    fn config(&self) -> &McpConfig;
}

/// MCP server implementation
pub struct McpServerImpl {
    config: McpConfig,
    security_context: Arc<RwLock<SecurityContext>>,
    tool_registry: Arc<ToolRegistry>,
    resource_registry: Arc<ResourceRegistry>,
    prompt_registry: Arc<PromptRegistry>,
    running: Arc<RwLock<bool>>,
}

/// Builder for MCP server
pub struct McpServerBuilder {
    config: McpConfig,
    tool_registry: ToolRegistry,
    resource_registry: ResourceRegistry,
    prompt_registry: PromptRegistry,
}

impl McpServerBuilder {
    pub fn new(config: McpConfig) -> Self {
        Self {
            config,
            tool_registry: ToolRegistry::new(),
            resource_registry: ResourceRegistry::new(),
            prompt_registry: PromptRegistry::new(),
        }
    }

    pub fn with_tool_registry(mut self, registry: ToolRegistry) -> Self {
        self.tool_registry = registry;
        self
    }

    pub fn with_resource_registry(mut self, registry: ResourceRegistry) -> Self {
        self.resource_registry = registry;
        self
    }

    pub fn with_prompt_registry(mut self, registry: PromptRegistry) -> Self {
        self.prompt_registry = registry;
        self
    }

    pub fn build(self) -> McpResult<McpServerImpl> {
        // Validate configuration
        self.config.validate().map_err(McpError::config_error)?;

        let security_context = Arc::new(RwLock::new(SecurityContext::new(
            self.config.are_control_operations_enabled(),
            self.config.rate_limit_per_minute,
            self.config
                .get_audit_log_path()
                .map(|p| p.to_string_lossy().to_string()),
        )));

        Ok(McpServerImpl {
            config: self.config,
            security_context,
            tool_registry: Arc::new(self.tool_registry),
            resource_registry: Arc::new(self.resource_registry),
            prompt_registry: Arc::new(self.prompt_registry),
            running: Arc::new(RwLock::new(false)),
        })
    }
}

#[async_trait]
impl McpServer for McpServerImpl {
    async fn start(&self) -> McpResult<()> {
        if !self.config.should_start_server() {
            log::info!("MCP server disabled in configuration");
            return Ok(());
        }

        let mut running = self.running.write().await;
        if *running {
            return Err(McpError::server_error("Server is already running"));
        }

        // Create message handler
        let handler = ServerMessageHandler {
            security_context: self.security_context.clone(),
            tool_registry: self.tool_registry.clone(),
            resource_registry: self.resource_registry.clone(),
            prompt_registry: self.prompt_registry.clone(),
        };

        // Create stdio transport
        let transport = StdioTransport::new(Arc::new(handler));

        log::info!("Starting MCP server with stdio transport");
        log::info!(
            "Control operations enabled: {}",
            self.config.are_control_operations_enabled()
        );

        *running = true;

        // Start the stdio transport (this will block until shutdown)
        transport.start().await?;

        // When we get here, the transport has shut down
        let mut running = self.running.write().await;
        *running = false;

        Ok(())
    }

    async fn stop(&self) -> McpResult<()> {
        let mut running = self.running.write().await;
        if !*running {
            return Ok(());
        }

        log::info!("Stopping MCP server");
        *running = false;
        Ok(())
    }

    fn is_running(&self) -> bool {
        // This is a simplified check - in a real implementation,
        // we'd need to track the actual transport state
        self.config.should_start_server()
    }

    fn config(&self) -> &McpConfig {
        &self.config
    }
}

/// Message handler implementation
struct ServerMessageHandler {
    security_context: Arc<RwLock<SecurityContext>>,
    tool_registry: Arc<ToolRegistry>,
    resource_registry: Arc<ResourceRegistry>,
    prompt_registry: Arc<PromptRegistry>,
}

#[async_trait]
impl MessageHandler for ServerMessageHandler {
    async fn handle_message(&self, message: McpMessage) -> McpResult<McpResponse> {
        match message {
            McpMessage::Initialize { id, params } => self.handle_initialize(id, params).await,
            McpMessage::Ping { id } => Ok(McpResponse {
                id,
                result: Some(json!({})),
                error: None,
            }),
            McpMessage::ListTools { id } => self.handle_list_tools(id).await,
            McpMessage::CallTool { id, params } => self.handle_call_tool(id, params).await,
            McpMessage::ListResources { id } => self.handle_list_resources(id).await,
            McpMessage::ReadResource { id, params } => self.handle_read_resource(id, params).await,
            McpMessage::ListPrompts { id } => self.handle_list_prompts(id).await,
            McpMessage::GetPrompt { id, params } => self.handle_get_prompt(id, params).await,
        }
    }
}

impl ServerMessageHandler {
    async fn handle_initialize(&self, id: Value, _params: InitializeParams) -> McpResult<McpResponse> {
        Ok(McpResponse {
            id,
            result: Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {},
                    "resources": {},
                    "prompts": {}
                },
                "serverInfo": {
                    "name": "Tari MCP Server",
                    "version": "1.0.0"
                }
            })),
            error: None,
        })
    }

    async fn handle_list_tools(&self, id: Value) -> McpResult<McpResponse> {
        let tools: Vec<ToolInfo> = self.tool_registry.list_tools();

        Ok(McpResponse {
            id,
            result: Some(json!({
                "tools": tools
            })),
            error: None,
        })
    }

    async fn handle_call_tool(&self, id: Value, params: ToolCallParams) -> McpResult<McpResponse> {
        // Get client IP - in a real implementation, this would come from the connection context
        let client_ip = "127.0.0.1".parse().unwrap();

        // Sanitize tool arguments before processing
        let sanitized_arguments = if let Some(args) = params.arguments {
            Some(sanitize_tool_input(&args)?)
        } else {
            None
        };

        // Check permissions
        let permission_level = self.tool_registry.get_permission_level(&params.name)?;
        let request_data = json!({
            "tool": params.name,
            "arguments": sanitized_arguments
        });

        let mut security_context = self.security_context.write().await;
        let _session_id = security_context.check_permission(
            client_ip,
            &format!("tool:{}", params.name),
            permission_level,
            request_data,
        )?;
        drop(security_context);

        // Execute the tool with sanitized arguments
        let result = self
            .tool_registry
            .execute_tool(&params.name, sanitized_arguments.unwrap_or(Value::Null))
            .await?;

        Ok(McpResponse {
            id,
            result: Some(json!({
                "content": [
                    {
                        "type": "text",
                        "text": result.to_string()
                    }
                ]
            })),
            error: None,
        })
    }

    async fn handle_list_resources(&self, id: Value) -> McpResult<McpResponse> {
        let resources: Vec<ResourceInfo> = self.resource_registry.list_resources();

        Ok(McpResponse {
            id,
            result: Some(json!({
                "resources": resources
            })),
            error: None,
        })
    }

    async fn handle_read_resource(&self, id: Value, params: ResourceReadParams) -> McpResult<McpResponse> {
        let content = self.resource_registry.read_resource(&params.uri).await?;

        Ok(McpResponse {
            id,
            result: Some(json!({
                "contents": [
                    {
                        "uri": params.uri,
                        "mimeType": "application/json",
                        "text": content.to_string()
                    }
                ]
            })),
            error: None,
        })
    }

    async fn handle_list_prompts(&self, id: Value) -> McpResult<McpResponse> {
        let prompts: Vec<PromptInfo> = self.prompt_registry.list_prompts();

        Ok(McpResponse {
            id,
            result: Some(json!({
                "prompts": prompts
            })),
            error: None,
        })
    }

    async fn handle_get_prompt(&self, id: Value, params: PromptGetParams) -> McpResult<McpResponse> {
        let content = self.prompt_registry.get_prompt(&params.name, params.arguments)?;

        Ok(McpResponse {
            id,
            result: Some(json!({
                "messages": content.messages
            })),
            error: None,
        })
    }
}
