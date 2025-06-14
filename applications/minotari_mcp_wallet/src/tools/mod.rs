//! Wallet-specific MCP tools

mod address_tools;
mod atomic_swap_tools;
mod balance_tools;
mod simple_transfer;
mod transaction_tools;
mod wallet_state;

use std::sync::Arc;

// Address tools
pub use address_tools::{
    AddressConverterTool,
    AddressValidationTool,
    GetAddressTool,
    GetCompleteAddressTool,
    GetPaymentIdAddressTool,
};
// Atomic swap tools
pub use atomic_swap_tools::{AtomicSwapStatusTool, ClaimHtlcRefundTool, ClaimShaAtomicSwapTool, SendShaAtomicSwapTool};
// Balance tools
pub use balance_tools::{BalanceAnalysisTool, BalanceMonitorTool, GetBalanceTool, GetUnspentAmountsTool};
use minotari_mcp_common::ToolRegistry;
use minotari_wallet_grpc_client::WalletGrpcClient;
pub use simple_transfer::SimpleTransferTool;
use tonic::transport::Channel;
// Transaction tools
pub use transaction_tools::{
    CancelTransactionTool,
    CoinSplitTool,
    GetCompletedTransactionsTool,
    GetTransactionInfoTool,
    TransactionAnalysisTool,
    TransferTool,
};
pub use wallet_state::WalletStateTool;

/// Registry for wallet-specific MCP tools
pub struct WalletToolRegistry;

impl WalletToolRegistry {
    /// Create a new wallet tool registry with all available tools
    #[allow(clippy::new_ret_no_self)] // Factory method for registry
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>, control_enabled: bool) -> ToolRegistry {
        let mut registry = ToolRegistry::new();

        // Always available tools (read-only operations)

        // Wallet state and status
        registry.register(Box::new(WalletStateTool::new(grpc_client.clone())));

        // Balance tools
        registry.register(Box::new(GetBalanceTool::new(grpc_client.clone())));
        registry.register(Box::new(GetUnspentAmountsTool::new(grpc_client.clone())));
        registry.register(Box::new(BalanceAnalysisTool::new(grpc_client.clone())));
        registry.register(Box::new(BalanceMonitorTool::new(grpc_client.clone())));

        // Transaction tools (read-only)
        registry.register(Box::new(GetTransactionInfoTool::new(grpc_client.clone())));
        registry.register(Box::new(GetCompletedTransactionsTool::new(grpc_client.clone())));
        registry.register(Box::new(TransactionAnalysisTool::new(grpc_client.clone())));

        // Address tools
        registry.register(Box::new(GetAddressTool::new(grpc_client.clone())));
        registry.register(Box::new(GetCompleteAddressTool::new(grpc_client.clone())));
        registry.register(Box::new(GetPaymentIdAddressTool::new(grpc_client.clone())));
        registry.register(Box::new(AddressValidationTool::new(grpc_client.clone())));
        registry.register(Box::new(AddressConverterTool::new(grpc_client.clone())));

        // Atomic swap tools (status and information)
        registry.register(Box::new(AtomicSwapStatusTool::new(grpc_client.clone())));

        // Control tools (only if control operations are enabled)
        if control_enabled {
            // Transaction control tools
            registry.register(Box::new(SimpleTransferTool::new(grpc_client.clone())));
            registry.register(Box::new(TransferTool::new(grpc_client.clone())));
            registry.register(Box::new(CoinSplitTool::new(grpc_client.clone())));
            registry.register(Box::new(CancelTransactionTool::new(grpc_client.clone())));

            // Atomic swap control tools
            registry.register(Box::new(SendShaAtomicSwapTool::new(grpc_client.clone())));
            registry.register(Box::new(ClaimShaAtomicSwapTool::new(grpc_client.clone())));
            registry.register(Box::new(ClaimHtlcRefundTool::new(grpc_client.clone())));
        }

        registry
    }
}
