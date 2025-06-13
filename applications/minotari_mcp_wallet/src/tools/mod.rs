//! Wallet-specific MCP tools

mod simple_transfer;
mod wallet_state;

use minotari_mcp_common::ToolRegistry;
use minotari_wallet_grpc_client::WalletGrpcClient;
use std::sync::Arc;
use tonic::transport::Channel;

pub use simple_transfer::SimpleTransferTool;
pub use wallet_state::WalletStateTool;

/// Registry for wallet-specific MCP tools
pub struct WalletToolRegistry;

impl WalletToolRegistry {
    /// Create a new wallet tool registry with all available tools
    pub fn new(
        grpc_client: Arc<WalletGrpcClient<Channel>>, 
        control_enabled: bool
    ) -> ToolRegistry {
        let mut registry = ToolRegistry::new();

        // Always available tools (read-only)
        registry.register(Box::new(WalletStateTool::new(grpc_client.clone())));

        // Control tools (only if control operations are enabled)
        if control_enabled {
            registry.register(Box::new(SimpleTransferTool::new(grpc_client.clone())));
        }

        registry
    }
}
