use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use tari_core::proof_of_work::PowAlgorithm;
use uuid::Uuid;

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
    pub miner_address: String,
    pub original_mining_hash: Vec<u8>,
    pub xn: u16,
}

pub struct SubmittedJob {
    pub job_id: String,
    pub nonce: u64,
    pub target: u64,
    pub chain_target: u64,
    pub blob: Vec<u8>,
    pub algo: String,
    pub pow_algo: PowAlgorithm,
    pub miner_address: String,
    pub original_mining_hash: Vec<u8>,
    pub result: Vec<u8>,
}
