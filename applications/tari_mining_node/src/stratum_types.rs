use serde::{Deserialize, Serialize};
use serde_json::Value;
use tari_core::blocks::Block;

#[derive(Serialize, Deserialize, Debug)]
pub struct LoginResponse {
    pub id: String,
    pub job: JobParams,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct JobParams {
    pub job_id: String,
    pub blob: String,
    pub target: String,
    pub height: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Job {
    pub job_id: u64,
    pub block: Option<Block>,
    pub target: u64,
    pub height: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RpcRequest {
    pub id: Option<String>,
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RpcResponse {
    pub id: String,
    pub result: Option<Value>,
    pub error: Option<RpcError>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LoginParams {
    pub login: String,
    pub pass: String,
    pub agent: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WorkerIdentifier {
    pub id: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SubmitParams {
    pub id: String,
    pub job_id: u64,
    pub nonce: u64,
    pub hash: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WorkerStatus {
    pub id: String,
    pub height: u64,
    pub difficulty: u64,
    pub accepted: u64,
    pub rejected: u64,
    pub stale: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum MinerMessage {
    // Height, Id, difficulty, Blob
    ReceivedJob(u64, u64, u64, String),
    StopJob,
    Shutdown,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ClientMessage {
    // job_id, hash, nonce
    FoundSolution(u64, String, u64),
    Shutdown,
}
