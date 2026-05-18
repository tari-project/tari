# Minotari Node MCP Server

A Model Context Protocol (MCP) server that provides AI agents with secure access to Tari blockchain node functionality.

## Features

### Security-First Design
- **Local-only binding**: Server only binds to loopback addresses (127.0.0.1) for security
- **Read-only operations**: All node operations are read-only for safety
- **Rate limiting**: Configurable request rate limits per client
- **Audit logging**: Optional comprehensive logging of all operations
- **Input validation**: Robust parameter validation for all operations

### Node Operations

#### Tools (Direct Actions)
- **network_info**: Get network status and connectivity information
- **blockchain_info**: Retrieve blockchain metadata and chain state
- **peer_info**: Get connected peer information and statistics
- **transaction_lookup**: Search for transactions by hash or criteria
- **block_lookup**: Retrieve block information by height or hash
- **mempool_stats**: Get current mempool status and statistics
- **node_identity**: Get node identity and address information
- **sync_status**: Check blockchain synchronization status

#### Resources (Data Access)
- **node_status**: Current node operational status
- **blockchain_state**: Real-time blockchain state information
- **network_peers**: Connected peer list and details
- **mempool_status**: Current mempool contents and statistics

#### Prompts (AI Guidance)
- **node_troubleshooting**: Help with node connectivity and sync issues
- **network_analysis**: Guidance for analyzing network health
- **blockchain_exploration**: Help with exploring blockchain data
- **sync_status**: Assistance with synchronization problems

## Installation

Build from source:

```bash
cargo build --release -p minotari_mcp_node
```

## Configuration

### Command Line Options

```bash
minotari_mcp_node --help
```

### Environment Variables

- `MINOTARI_NODE_MCP_ENABLED`: Enable MCP server
- `MINOTARI_NODE_MCP_BIND_ADDRESS`: Server bind address (default: 127.0.0.1)
- `MINOTARI_NODE_MCP_PORT`: Server port (default: 8082)
- `MINOTARI_NODE_MCP_MAX_CONNECTIONS`: Max concurrent connections (default: 5)
- `MINOTARI_NODE_MCP_TIMEOUT`: Request timeout in seconds (default: 60)
- `MINOTARI_NODE_MCP_RATE_LIMIT`: Max requests per minute per client (default: 30)
- `MINOTARI_NODE_MCP_AUDIT_LOGGING`: Enable audit logging
- `MINOTARI_NODE_MCP_AUDIT_LOG_PATH`: Audit log file path
- `MINOTARI_NODE_GRPC_ADDRESS`: Node gRPC endpoint (default: 127.0.0.1:18142)

## Usage

### Basic Setup

1. **Start your Tari base node with gRPC enabled**:
   ```bash
   minotari_node --enable-grpc
   ```

2. **Start the MCP server**:
   ```bash
   minotari_mcp_node --mcp-enabled
   ```

3. **With custom node gRPC address**:
   ```bash
   minotari_mcp_node --mcp-enabled --node-grpc-address 127.0.0.1:18142
   ```

### AI Integration

Connect your AI agent to the MCP server at `http://127.0.0.1:8082` using the Model Context Protocol.

Example MCP client configuration:
```json
{
  "servers": {
    "tari-node": {
      "command": "minotari_mcp_node",
      "args": ["--mcp-enabled"],
      "env": {
        "MINOTARI_NODE_MCP_ENABLED": "true"
      }
    }
  }
}
```

## Security Considerations

### ⚠️ IMPORTANT SECURITY NOTES

1. **Read-Only Operations**: All node operations are read-only, making this server safe for AI agent access.

2. **Network Binding**: The server only binds to localhost (127.0.0.1) for security. Do not modify this unless you understand the risks.

3. **Audit Logging**: Enable audit logging in production environments to track all operations.

### Recommended Settings

**For Development/Testing**:
```bash
minotari_mcp_node --mcp-enabled --mcp-audit-logging
```

**For AI Integration**:
```bash
minotari_mcp_node --mcp-enabled --mcp-audit-logging --mcp-rate-limit 20
```

**For Production Monitoring**:
```bash
minotari_mcp_node --mcp-enabled --mcp-audit-logging --mcp-rate-limit 50 --mcp-timeout 120
```

## Development

### Architecture

The server is built on the `minotari_mcp_common` framework and integrates with the Tari base node via gRPC.

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   AI Agent      │───▶│   MCP Server     │───▶│  Tari Node      │
│   (Claude, etc) │    │   (This App)     │    │   (gRPC API)    │
└─────────────────┘    └──────────────────┘    └─────────────────┘
```

### Adding New Tools

1. Create a new tool in `src/tools/`
2. Implement the `McpTool` trait
3. Register it in `src/tools/mod.rs`
4. Add appropriate tests

### Testing

```bash
cargo test -p minotari_mcp_node
```

## Troubleshooting

### Common Issues

1. **Connection refused**: Ensure the Tari base node is running with gRPC enabled
2. **Rate limited**: Adjust `--mcp-rate-limit` if hitting limits
3. **Timeout errors**: Increase `--mcp-timeout` for slow operations
4. **Node not synced**: Some operations require a synchronized node

### Example Queries

**Check network status**:
```json
{
  "tool": "network_info",
  "parameters": {}
}
```

**Get blockchain info**:
```json
{
  "tool": "blockchain_info", 
  "parameters": {}
}
```

**Look up a block**:
```json
{
  "tool": "block_lookup",
  "parameters": {
    "height": 12345
  }
}
```

### Logs

Check logs in `log/application.log` for detailed operation information.

## Use Cases

### Network Monitoring
- Monitor node connectivity and peer health
- Track blockchain synchronization status
- Analyze network performance metrics

### Blockchain Exploration
- Query transaction and block data
- Explore mempool contents
- Analyze chain statistics

### Node Diagnostics
- Troubleshoot connectivity issues
- Monitor node performance
- Debug synchronization problems

## License

BSD-3-Clause - see LICENSE file for details.
