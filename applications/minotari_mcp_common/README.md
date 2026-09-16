# Minotari MCP Common

Common infrastructure for building Model Context Protocol (MCP) servers in the Tari ecosystem.

## Overview

This crate provides a secure, production-ready foundation for creating MCP servers that allow AI agents to interact with Tari blockchain functionality. It implements security-first principles with local-only binding, explicit permission controls, and comprehensive audit logging.

## Features

### Core Infrastructure
- **MCP Server Framework**: Complete server implementation with JSON-RPC transport
- **Security Framework**: Permission levels, rate limiting, audit logging, user confirmation workflows
- **Tool Registry**: Dynamic registration and execution of MCP tools
- **Resource Registry**: Structured data access for AI agents
- **Prompt Registry**: AI guidance and help system

### Security Features
- **Local-only binding**: Servers only bind to loopback addresses (127.0.0.1)
- **Permission levels**: Read-only vs Control operations with explicit user consent
- **Rate limiting**: Configurable per-client request rate limiting
- **Audit logging**: Comprehensive logging of all operations with timestamps and client info
- **Session management**: Secure session handling with automatic cleanup
- **Input validation**: Robust parameter validation for all operations

### Transport Layer
- **JSON-RPC 2.0**: Standards-compliant JSON-RPC over HTTP
- **Async/Await**: Full async support with tokio
- **Error Handling**: Comprehensive error types and handling
- **Timeout Management**: Configurable request timeouts

## Architecture

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   AI Agent      │───▶│   MCP Server     │───▶│  Tari Services  │
│   (Claude, etc) │    │   (Your App)     │    │   (gRPC, etc)   │
└─────────────────┘    └──────────────────┘    └─────────────────┘
                              │
                              ▼
                    ┌──────────────────┐
                    │ minotari_mcp_    │
                    │    common        │
                    │                  │
                    │ • Security       │
                    │ • Transport      │
                    │ • Tools/Resources│
                    │ • Prompts        │
                    └──────────────────┘
```

## Usage

### Basic Server

```rust
use minotari_mcp_common::{
    McpServer, McpServerBuilder, ToolRegistry, ResourceRegistry, PromptRegistry,
    SecurityContext, PermissionLevel
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create registries
    let mut tools = ToolRegistry::new();
    let mut resources = ResourceRegistry::new();
    let mut prompts = PromptRegistry::new();
    
    // Register your tools, resources, and prompts
    // tools.register(Box::new(MyTool::new()));
    
    // Create security context
    let security = SecurityContext::new()
        .with_local_only_binding(true)
        .with_rate_limit(30) // 30 requests per minute
        .with_audit_logging(true);
    
    // Build and start server
    let server = McpServerBuilder::new()
        .with_tools(tools)
        .with_resources(resources)
        .with_prompts(prompts)
        .with_security(security)
        .with_bind_address("127.0.0.1:8080".parse()?)
        .build()
        .await?;
    
    println!("MCP Server starting on 127.0.0.1:8080");
    server.run().await?;
    
    Ok(())
}
```

### Creating Tools

```rust
use minotari_mcp_common::{McpTool, McpResult, PermissionLevel, json_schema};
use async_trait::async_trait;
use serde_json::Value;

pub struct MyTool {
    // Tool state
}

#[async_trait]
impl McpTool for MyTool {
    fn name(&self) -> &str {
        "my_tool"
    }
    
    fn description(&self) -> &str {
        "Description of what this tool does"
    }
    
    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::ReadOnly // or PermissionLevel::Control
    }
    
    fn input_schema(&self) -> Value {
        json_schema! {
            "param1" => {
                "type": "string",
                "description": "First parameter"
            },
            "param2" => {
                "type": "number", 
                "description": "Second parameter"
            }
        }
    }
    
    async fn execute(&self, params: Value) -> McpResult<Value> {
        let param1 = get_required_string_param(&params, "param1")?;
        let param2 = get_required_number_param(&params, "param2")?;
        
        // Tool implementation
        Ok(serde_json::json!({
            "result": "success",
            "param1": param1,
            "param2": param2
        }))
    }
}
```

### Creating Resources

```rust
use minotari_mcp_common::{McpResource, McpResult};
use async_trait::async_trait;
use serde_json::Value;

pub struct MyResource {
    // Resource state
}

#[async_trait]
impl McpResource for MyResource {
    fn uri(&self) -> &str {
        "my-app://resource/data"
    }
    
    fn name(&self) -> &str {
        "Data Resource"
    }
    
    fn description(&self) -> &str {
        "Provides access to application data"
    }
    
    fn mime_type(&self) -> &str {
        "application/json"
    }
    
    async fn read(&self) -> McpResult<Value> {
        Ok(serde_json::json!({
            "data": "resource content",
            "timestamp": chrono::Utc::now()
        }))
    }
}
```

### Creating Prompts

```rust
use minotari_mcp_common::{
    PromptRegistry, simple_prompt, text_message,
    prompts::{MessageRole, PromptMessage}
};

pub fn create_prompts() -> PromptRegistry {
    let mut registry = PromptRegistry::new();
    
    registry.register(simple_prompt!(
        "help_prompt",
        "Provides help and guidance for using the application",
        vec![
            text_message(MessageRole::User, "I need help with this application"),
            text_message(MessageRole::Assistant, "I can help you with various operations. What would you like to do?"),
            text_message(MessageRole::User, "What operations are available?"),
            text_message(MessageRole::Assistant, "Available operations include...")
        ]
    ));
    
    registry
}
```

## Helper Functions

The crate provides convenient helper functions for parameter validation:

```rust
use minotari_mcp_common::{
    get_required_string_param, get_required_number_param, 
    get_required_bool_param, get_required_u64_param
};

async fn my_tool_execute(&self, params: Value) -> McpResult<Value> {
    let name = get_required_string_param(&params, "name")?;
    let amount = get_required_u64_param(&params, "amount")?;
    let enabled = get_required_bool_param(&params, "enabled")?;
    
    // Use validated parameters...
}
```

## Macros

### `json_schema!`

Create JSON schemas for tool input parameters:

```rust
let schema = json_schema! {
    "address" => {
        "type": "string",
        "description": "Wallet address"
    },
    "amount" => {
        "type": "number",
        "description": "Amount to send"
    }
};
```

### `simple_prompt!`

Create simple conversational prompts:

```rust
let prompt = simple_prompt!(
    "example",
    "Example prompt for demonstration",
    vec![
        text_message(MessageRole::User, "User message"),
        text_message(MessageRole::Assistant, "Assistant response")
    ]
);
```

## Security Model

### Permission Levels

- **ReadOnly**: Safe operations that don't modify state or spend funds
- **Control**: Operations that can modify state or spend funds (requires explicit user consent)

### Rate Limiting

- Configurable per-client request limits
- Automatic client identification and tracking
- Graceful degradation under load

### Audit Logging

All operations are logged with:
- Timestamp
- Client identifier
- Operation type
- Parameters (sanitized)
- Result status
- Permission level

### User Confirmation

For control operations, the framework can require user confirmation:
- Interactive prompts
- Timeout handling
- Operation cancellation

## Error Handling

The framework provides comprehensive error types:

```rust
use minotari_mcp_common::{McpError, McpResult};

// Common error patterns
fn example() -> McpResult<Value> {
    // Parameter validation error
    if param.is_empty() {
        return Err(McpError::invalid_request("Parameter cannot be empty"));
    }
    
    // Tool not found
    if !tool_exists {
        return Err(McpError::tool_not_found("unknown_tool"));
    }
    
    // Internal error
    operation.execute()
        .map_err(|e| McpError::internal_error(format!("Operation failed: {e}")))?;
    
    Ok(result)
}
```

## Examples

See the following applications for complete usage examples:

- [`minotari_mcp_wallet`](../minotari_mcp_wallet/): Wallet MCP server
- [`minotari_mcp_node`](../minotari_mcp_node/): Node MCP server

## Development

### Testing

```bash
cargo test -p minotari_mcp_common
```

### Contributing

1. Follow Rust best practices
2. Add comprehensive tests for new features
3. Update documentation for API changes
4. Ensure security implications are considered

## License

BSD-3-Clause - see LICENSE file for details.
