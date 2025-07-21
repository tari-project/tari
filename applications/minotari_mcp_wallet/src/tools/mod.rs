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
