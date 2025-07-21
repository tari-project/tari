# Minotari Wallet MCP Server

A Model Context Protocol (MCP) server that provides AI agents with secure access to Tari wallet functionality.

## Features

### Security-First Design
- **Local-only binding**: Server only binds to loopback addresses (127.0.0.1) for security
- **Permission levels**: Separate read-only and control operations with explicit user consent
- **Rate limiting**: Configurable request rate limits per client
- **Audit logging**: Optional comprehensive logging of all operations
- **User confirmation**: Optional confirmation prompts for value-transferring operations

### Wallet Operations

#### Tools (Direct Actions)
- **balance_check**: Get current wallet balance and status
- **transaction_history**: Retrieve transaction history with filtering
- **transfer**: Send Tari to another address (requires control permission)
- **burn_transaction**: Create burn transactions (requires control permission)
- **coin_split**: Split coins for better UTXO management (requires control permission)
- **address_info**: Get detailed address information

#### Resources (Data Access)
- **simple_balance**: Current balance information
- **transaction_list**: Recent transactions
- **address_book**: Known addresses and contacts

#### Prompts (AI Guidance)
- **balance_check**: Complete wallet overview guidance
- **send_transaction**: Step-by-step transaction guidance
- **transaction_troubleshooting**: Help with transaction issues
- **wallet_recovery**: Wallet recovery procedures

## Installation

Build from source:

```bash
cargo build --release -p minotari_mcp_wallet
```

## Configuration

### Command Line Options

```bash
minotari_mcp_wallet --help
```

### Environment Variables

- `MINOTARI_WALLET_MCP_ENABLED`: Enable MCP server
- `MINOTARI_WALLET_MCP_CONTROL_ENABLED`: Enable control operations (dangerous)
- `MINOTARI_WALLET_MCP_BIND_ADDRESS`: Server bind address (default: 127.0.0.1)
- `MINOTARI_WALLET_MCP_PORT`: Server port (default: 8081)
- `MINOTARI_WALLET_MCP_MAX_CONNECTIONS`: Max concurrent connections (default: 5)
- `MINOTARI_WALLET_MCP_TIMEOUT`: Request timeout in seconds (default: 60)
- `MINOTARI_WALLET_MCP_RATE_LIMIT`: Max requests per minute per client (default: 30)
- `MINOTARI_WALLET_MCP_AUDIT_LOGGING`: Enable audit logging
- `MINOTARI_WALLET_MCP_AUDIT_LOG_PATH`: Audit log file path
- `MINOTARI_WALLET_GRPC_ADDRESS`: Wallet gRPC endpoint (default: 127.0.0.1:18143)
- `MINOTARI_WALLET_MCP_REQUIRE_CONFIRMATION`: Require user confirmation for transfers

## Usage

### Basic Setup

1. **Start your Tari wallet with gRPC enabled**:
   ```bash
   minotari_console_wallet --enable-grpc
   ```

2. **Start the MCP server in read-only mode** (safe):
   ```bash
   minotari_mcp_wallet --mcp-enabled
   ```

3. **For control operations** (allows spending - use with caution):
   ```bash
   minotari_mcp_wallet --mcp-enabled --mcp-control-enabled --require-confirmation
   ```

### AI Integration

Connect your AI agent to the MCP server at `http://127.0.0.1:8081` using the Model Context Protocol.

Example MCP client configuration:
```json
{
  "servers": {
    "tari-wallet": {
      "command": "minotari_mcp_wallet",
      "args": ["--mcp-enabled"],
      "env": {
        "MINOTARI_WALLET_MCP_ENABLED": "true"
      }
    }
  }
}
```

## Security Considerations

### ⚠️ IMPORTANT SECURITY NOTES

1. **Control Operations**: When `--mcp-control-enabled` is set, AI agents can spend your funds. Only enable this in fully trusted environments.

2. **Network Binding**: The server only binds to localhost (127.0.0.1) for security. Do not modify this unless you understand the risks.

3. **User Confirmation**: Use `--require-confirmation` for additional safety when control operations are enabled.

4. **Audit Logging**: Enable audit logging in production environments to track all operations.

### Recommended Settings

**For Development/Testing**:
```bash
minotari_mcp_wallet --mcp-enabled --mcp-audit-logging
```

**For AI Integration (Read-Only)**:
```bash
minotari_mcp_wallet --mcp-enabled --mcp-audit-logging --mcp-rate-limit 10
```

**For Trusted AI Control** (use with extreme caution):
```bash
minotari_mcp_wallet --mcp-enabled --mcp-control-enabled --require-confirmation --mcp-audit-logging
```

## Development

### Architecture

The server is built on the `minotari_mcp_common` framework and integrates with the Tari wallet via gRPC.

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   AI Agent      │───▶│   MCP Server     │───▶│  Tari Wallet    │
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
cargo test -p minotari_mcp_wallet
```

## Troubleshooting

### Common Issues

1. **Connection refused**: Ensure the Tari wallet is running with gRPC enabled
2. **Permission denied**: Check that control operations are enabled if needed
3. **Rate limited**: Adjust `--mcp-rate-limit` if hitting limits
4. **Timeout errors**: Increase `--mcp-timeout` for slow operations

### Logs

Check logs in `log/application.log` for detailed operation information.

## License

BSD-3-Clause - see LICENSE file for details.
