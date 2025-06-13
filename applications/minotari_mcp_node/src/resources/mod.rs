//! Node-specific MCP resources

mod chain_metadata;
mod network_status;
mod sync_progress;
mod mempool_stats;
mod peer_list;
mod block_info;
mod transaction_info;
mod network_difficulty;

use minotari_mcp_common::ResourceRegistry;
use minotari_node_grpc_client::BaseNodeGrpcClient;
use std::sync::Arc;
use tonic::transport::Channel;

pub use chain_metadata::ChainMetadataResource;
pub use network_status::NetworkStatusResource;
pub use sync_progress::SyncProgressResource;
pub use mempool_stats::MempoolStatsResource;
pub use peer_list::PeerListResource;
pub use block_info::BlockInfoResource;
pub use transaction_info::TransactionInfoResource;
pub use network_difficulty::NetworkDifficultyResource;

/// Registry for node-specific MCP resources
pub struct NodeResourceRegistry;

impl NodeResourceRegistry {
    /// Create a new node resource registry with all available resources
    #[allow(clippy::new_ret_no_self)]  // Factory method for registry
    pub fn new(grpc_client: Arc<BaseNodeGrpcClient<Channel>>) -> ResourceRegistry {
        let mut registry = ResourceRegistry::new();

        // Static resources (always available)
        registry.register(Box::new(ChainMetadataResource::new(grpc_client.clone())));
        registry.register(Box::new(NetworkStatusResource::new(grpc_client.clone())));
        registry.register(Box::new(SyncProgressResource::new(grpc_client.clone())));
        registry.register(Box::new(MempoolStatsResource::new(grpc_client.clone())));
        registry.register(Box::new(PeerListResource::new(grpc_client.clone())));
        registry.register(Box::new(NetworkDifficultyResource::new(grpc_client.clone())));

        // Templated resources (support parameters)
        registry.register(Box::new(BlockInfoResource::new(grpc_client.clone())));
        registry.register(Box::new(TransactionInfoResource::new(grpc_client.clone())));

        registry
    }
}
