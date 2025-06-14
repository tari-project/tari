//! Wallet-specific MCP resources

mod simple_balance;

use std::sync::Arc;

use minotari_mcp_common::ResourceRegistry;
use minotari_wallet_grpc_client::WalletGrpcClient;
pub use simple_balance::SimpleBalanceResource;
use tonic::transport::Channel;

/// Registry for wallet-specific MCP resources
pub struct WalletResourceRegistry;

impl WalletResourceRegistry {
    /// Create a new wallet resource registry with all available resources
    #[allow(clippy::new_ret_no_self)] // Factory method for registry
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> ResourceRegistry {
        let mut registry = ResourceRegistry::new();

        // Static resources (always available)
        registry.register(Box::new(SimpleBalanceResource::new(grpc_client.clone())));

        registry
    }
}
