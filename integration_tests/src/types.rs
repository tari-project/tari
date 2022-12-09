// message RegisterValidatorNodeRequest {
//     bytes validator_node_public_key = 1;
//     Signature validator_node_signature = 2;
//     uint64 fee_per_gram = 3;
//     string message = 4;
// }
//
// message RegisterValidatorNodeResponse {
//     uint64 transaction_id = 1;
//     bool is_success = 2;
//     string failure_message = 3;
// }

use tari_common_types::types::FixedHash;

#[derive(Debug, Clone)]
pub struct BlockInfo {
    pub hash: FixedHash,
    pub height: u64,
    pub next_block_hash: Option<FixedHash>,
}

#[derive(Debug, Clone)]
pub struct BaseLayerMetadata {
    pub height_of_longest_chain: u64,
    pub tip_hash: FixedHash,
}

#[derive(Debug, Clone)]
pub struct RegisterValidatorNode {
    //
}
