use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct TipInfoResponse {
    pub metadata: Option<tari_common_types::chain_metadata::ChainMetadata>,
    pub is_synced: bool,
}
