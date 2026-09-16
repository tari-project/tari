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
//! Node-specific MCP tools

mod submit_block;
mod submit_transaction;
// mod ban_peer; // Commented out - BanPeerRequest/UnbanPeerRequest not available in gRPC client
mod blockchain_tools;
mod mempool_tools;
mod mining_tools;
mod network_tools;

// Blockchain tools
pub use blockchain_tools::{
    GetBlocksTool,
    GetHeaderByHashTool,
    GetNetworkDifficultyTool,
    GetNetworkStateTool,
    GetSyncInfoTool,
    GetTipInfoTool,
    GetTokensInCirculationTool,
    ListHeadersTool,
};
// Mempool tools
pub use mempool_tools::{AnalyzeMempoolTool, GetMempoolStatsTool, GetMempoolTransactionsTool, GetTransactionStateTool};
// pub use ban_peer::{BanPeerTool, UnbanPeerTool}; // Commented out - not available

// Mining tools
pub use mining_tools::{
    GetNewBlockTemplateTool,
    GetNewBlockTemplateWithCoinbasesTool,
    GetNewBlockTool,
    GetNewBlockWithCoinbasesTool,
    MiningAnalysisTool,
};
use minotari_mcp_common::ToolRegistry;
use minotari_node_grpc_client::BaseNodeGrpcClient;
// Network tools
pub use network_tools::{
    GetAllPeersTool,
    GetNetworkStatusTool,
    GetNodeIdentityTool,
    ListConnectedPeersTool,
    NetworkDiagnosticsTool,
};
pub use submit_block::SubmitBlockTool;
pub use submit_transaction::SubmitTransactionTool;
use tonic::transport::Channel;

/// Registry for node-specific MCP tools
pub struct NodeToolRegistry;

impl NodeToolRegistry {
    /// Create a new node tool registry with all available tools
    #[allow(clippy::new_ret_no_self)] // Factory method for registry
    pub fn new(grpc_client: BaseNodeGrpcClient<Channel>, control_enabled: bool) -> ToolRegistry {
        let mut registry = ToolRegistry::new();

        // Always available tools (read-only operations)

        // Blockchain tools
        registry.register(Box::new(ListHeadersTool::new(grpc_client.clone())));
        registry.register(Box::new(GetHeaderByHashTool::new(grpc_client.clone())));
        registry.register(Box::new(GetBlocksTool::new(grpc_client.clone())));
        registry.register(Box::new(GetTipInfoTool::new(grpc_client.clone())));
        registry.register(Box::new(GetSyncInfoTool::new(grpc_client.clone())));
        registry.register(Box::new(GetNetworkDifficultyTool::new(grpc_client.clone())));
        registry.register(Box::new(GetTokensInCirculationTool::new(grpc_client.clone())));
        registry.register(Box::new(GetNetworkStateTool::new(grpc_client.clone())));

        // Mempool tools
        registry.register(Box::new(GetMempoolStatsTool::new(grpc_client.clone())));
        registry.register(Box::new(GetMempoolTransactionsTool::new(grpc_client.clone())));
        registry.register(Box::new(GetTransactionStateTool::new(grpc_client.clone())));
        registry.register(Box::new(AnalyzeMempoolTool::new(grpc_client.clone())));

        // Network tools
        registry.register(Box::new(GetNetworkStatusTool::new(grpc_client.clone())));
        registry.register(Box::new(ListConnectedPeersTool::new(grpc_client.clone())));
        registry.register(Box::new(GetAllPeersTool::new(grpc_client.clone())));
        registry.register(Box::new(GetNodeIdentityTool::new(grpc_client.clone())));
        registry.register(Box::new(NetworkDiagnosticsTool::new(grpc_client.clone())));

        // Mining tools (read-only)
        registry.register(Box::new(GetNewBlockTemplateTool::new(grpc_client.clone())));
        registry.register(Box::new(GetNewBlockTool::new(grpc_client.clone())));
        registry.register(Box::new(GetNewBlockTemplateWithCoinbasesTool::new(grpc_client.clone())));
        registry.register(Box::new(GetNewBlockWithCoinbasesTool::new(grpc_client.clone())));
        registry.register(Box::new(MiningAnalysisTool::new(grpc_client.clone())));

        // Control tools (only if control operations are enabled)
        if control_enabled {
            registry.register(Box::new(SubmitBlockTool::new(grpc_client.clone())));
            registry.register(Box::new(SubmitTransactionTool::new(grpc_client.clone())));
            // registry.register(Box::new(BanPeerTool::new(grpc_client.clone()))); // Commented out - not available
            // registry.register(Box::new(UnbanPeerTool::new(grpc_client.clone()))); // Commented out - not available
        }

        registry
    }
}
