use tokio::sync::{oneshot, watch};

use crate::stratum::{job::SubmittedJob, job_repository_service::JobRepositoryRequest};

mod block_template_repository;
mod job;
mod job_repository;
mod job_repository_service;
mod memory_job_repository;
pub mod multi_stratum_stream_adapter;
pub mod nicehash_stream_adapter;
pub mod stratum_server;
pub mod stratum_v1_adapter;
pub mod stream_adapter;
mod tari_sha3x_stratum_handler;

pub(crate) type SubmitJobQueueSender = tokio::sync::mpsc::Sender<(SubmittedJob, oneshot::Sender<Result<(), String>>)>;
pub(crate) type SubmitJobQueueReceiver =
    tokio::sync::mpsc::Receiver<(SubmittedJob, oneshot::Sender<Result<(), String>>)>;
pub type JobRepositorySender = tokio::sync::mpsc::Sender<JobRepositoryRequest>;
pub type JobRepositoryReceiver = tokio::sync::mpsc::Receiver<JobRepositoryRequest>;
pub type LatestBlockBroadcastReceiver = watch::Receiver<(Vec<u8>, u64, u64)>; // (blob, height, target)
pub type LatestBlockBroadcastSender = watch::Sender<(Vec<u8>, u64, u64)>; // (blob, height, target)

#[derive(Debug, Clone)]
pub enum StratumRequest {
    Login {
        id: String,
        login: String,
        address: String,
        worker: Option<String>,
        pass: String,
        agent: String,
        algo: Vec<String>,
    },
    Submit {
        id: String,
        job_id: String,
        nonce: String,
        result: String,
    },
    Subscribe {
        id: String,
        agent: String,
        // address: String,
        // worker: Option<String>,
    },
    Authorize {
        id: String,
        login: String,
        is_solo: bool,
        worker_name: Option<String>,
        pass: String,
    },
    ExtraNonceSubscribe {
        id: String,
    },
}

impl StratumRequest {
    pub fn id(&self) -> &str {
        match self {
            StratumRequest::Login { id, .. } => id.as_str(),
            StratumRequest::Submit { id, .. } => id.as_str(),
            StratumRequest::Subscribe { id, .. } => id.as_str(),
            StratumRequest::Authorize { id, .. } => id.as_str(),
            StratumRequest::ExtraNonceSubscribe { id } => id.as_str(),
        }
    }
}
