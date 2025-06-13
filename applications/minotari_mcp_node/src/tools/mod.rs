//! Node-specific MCP tools

mod submit_block;
mod submit_transaction;
// mod ban_peer; // Commented out - BanPeerRequest/UnbanPeerRequest not available in gRPC client
mod mining_tools;
mod blockchain_tools;
mod mempool_tools;
mod network_tools;

use minotari_mcp_common::ToolRegistry;
use minotari_node_grpc_client::BaseNodeGrpcClient;
use std::sync::Arc;
use tonic::transport::Channel;

pub use submit_block::SubmitBlockTool;
pub use submit_transaction::SubmitTransactionTool;
// pub use ban_peer::{BanPeerTool, UnbanPeerTool}; // Commented out - not available

// Mining tools
pub use mining_tools::{
    GetNewBlockTemplateTool, GetNewBlockTool, GetNewBlockTemplateWithCoinbasesTool,
    GetNewBlockWithCoinbasesTool, MiningAnalysisTool
};

// Blockchain tools  
pub use blockchain_tools::{
    ListHeadersTool, GetHeaderByHashTool, GetBlocksTool, GetTipInfoTool,
    GetSyncInfoTool, GetNetworkDifficultyTool, GetTokensInCirculationTool,
    GetNetworkStateTool
};

// Mempool tools
pub use mempool_tools::{
    GetMempoolStatsTool, GetMempoolTransactionsTool, GetTransactionStateTool,
    AnalyzeMempoolTool
};

// Network tools
pub use network_tools::{
    GetNetworkStatusTool, ListConnectedPeersTool, GetAllPeersTool,
    GetNodeIdentityTool, NetworkDiagnosticsTool
};

/// Registry for node-specific MCP tools
pub struct NodeToolRegistry;

impl NodeToolRegistry {
    /// Create a new node tool registry with all available tools
    #[allow(clippy::new_ret_no_self)]  // Factory method for registry
    pub fn new(
        grpc_client: Arc<BaseNodeGrpcClient<Channel>>,
        control_enabled: bool
    ) -> ToolRegistry {
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
