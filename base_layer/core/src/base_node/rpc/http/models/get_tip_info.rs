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