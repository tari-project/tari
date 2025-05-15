use crate::proto;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct ChainMetadata {
    /// The current chain height, or the block number of the longest valid chain, or `None` if there is no chain
    pub best_block_height: u64,
    /// The block hash of the current tip of the longest valid chain, or `None` for an empty chain
    pub best_block_hash: Vec<u8>,
    /// The current geometric mean of the pow of the chain tip, or `None` if there is no chain
    pub accumulated_difficulty: Vec<u8>,
    /// The effective height of the pruning horizon. This indicates from what height
    /// a full block can be provided (exclusive).
    /// If `pruned_height` is equal to the `best_block_height` no blocks can be provided.
    /// Archival nodes wil always have an `pruned_height` of zero.
    pub pruned_height: u64,
    /// Timestamp of the last block in the chain, or `None` if there is no chain
    pub timestamp: u64,
}

#[derive(Serialize, Deserialize)]
pub struct TipInfoResponse {
    pub metadata: Option<ChainMetadata>,
    pub is_synced: bool,
}

impl From<proto::base_node::ChainMetadata> for ChainMetadata {
    fn from(proto_metadata: proto::base_node::ChainMetadata) -> Self {
        ChainMetadata {
            best_block_height: proto_metadata.best_block_height,
            best_block_hash: proto_metadata.best_block_hash,
            accumulated_difficulty: proto_metadata.accumulated_difficulty,
            pruned_height: proto_metadata.pruned_height,
            timestamp: proto_metadata.timestamp,
        }
    }
}

impl From<proto::base_node::TipInfoResponse> for TipInfoResponse {
    fn from(proto_resp: proto::base_node::TipInfoResponse) -> Self {
        TipInfoResponse {
            metadata: proto_resp.metadata.map(|metadata| metadata.into()),
            is_synced: proto_resp.is_synced,
        }
    }
}