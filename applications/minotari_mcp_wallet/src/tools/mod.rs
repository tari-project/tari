//! Wallet-specific MCP tools

mod simple_transfer;

use minotari_mcp_common::{ToolRegistry, McpTool};
use minotari_wallet_grpc_client::WalletGrpcClient;
use std::sync::Arc;
use tonic::transport::Channel;

pub use simple_transfer::SimpleTransferTool;

/// Registry for wallet-specific MCP tools
pub struct WalletToolRegistry;

impl WalletToolRegistry {
    /// Create a new wallet tool registry with all available tools
    pub fn new(
        grpc_client: Arc<WalletGrpcClient<Channel>>, 
        control_enabled: bool
    ) -> ToolRegistry {
        let mut registry = ToolRegistry::new();

        // Control tools (only if control operations are enabled)
        if control_enabled {
            registry.register(Box::new(SimpleTransferTool::new(grpc_client.clone())));
        }

        registry
    }
}
