//! Node-specific MCP tools

mod submit_block;
mod submit_transaction;
mod ban_peer;
mod mining_tools;

use minotari_mcp_common::{ToolRegistry, McpTool};
use minotari_node_grpc_client::BaseNodeGrpcClient;
use std::sync::Arc;
use tonic::transport::Channel;

pub use submit_block::SubmitBlockTool;
pub use submit_transaction::SubmitTransactionTool;
pub use ban_peer::{BanPeerTool, UnbanPeerTool};
pub use mining_tools::GetNewBlockTemplateTool;

/// Registry for node-specific MCP tools
pub struct NodeToolRegistry;

impl NodeToolRegistry {
    /// Create a new node tool registry with all available tools
    pub fn new(
        grpc_client: Arc<BaseNodeGrpcClient<Channel>>, 
        control_enabled: bool
    ) -> ToolRegistry {
        let mut registry = ToolRegistry::new();

        // Always available tools (read-only or mining-related)
        registry.register(Box::new(GetNewBlockTemplateTool::new(grpc_client.clone())));

        // Control tools (only if control operations are enabled)
        if control_enabled {
            registry.register(Box::new(SubmitBlockTool::new(grpc_client.clone())));
            registry.register(Box::new(SubmitTransactionTool::new(grpc_client.clone())));
            registry.register(Box::new(BanPeerTool::new(grpc_client.clone())));
            registry.register(Box::new(UnbanPeerTool::new(grpc_client.clone())));
        }

        registry
    }
}
