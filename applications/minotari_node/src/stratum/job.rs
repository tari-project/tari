use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use tari_common_types::tari_address::TariAddress;
use tari_transaction_components::tari_proof_of_work::PowAlgorithm;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub job_id: String,
    pub created_at: NaiveDateTime,
    pub algo: String,
    pub pow_algo: PowAlgorithm,
    pub target: String,
    pub blob: Vec<u8>,
    pub height: u64,
    pub chain_target: String,
    pub miner_address: TariAddress,
    pub original_mining_hash: Vec<u8>,
    pub prev_block_hash: Vec<u8>,
    pub xn: u16,
}

pub struct SubmittedJob {
    pub job_id: String,
    pub nonce: u64,
    pub pow_algo: PowAlgorithm,
    pub original_mining_hash: Vec<u8>,
    pub cuckaroo_nonces: Vec<u64>,
}
