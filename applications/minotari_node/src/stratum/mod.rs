use tokio::sync::{oneshot, watch};

use crate::stratum::{job::SubmittedJob, job_repository_service::JobRepositoryRequest};

mod block_template_repository;
mod job;
mod job_repository;
mod job_repository_service;
mod memory_job_repository;
pub mod stratum_server;
mod tari_sha3x_stratum_handler;

pub(crate) type SubmitJobQueueSender = tokio::sync::mpsc::Sender<(SubmittedJob, oneshot::Sender<Result<(), String>>)>;
pub(crate) type SubmitJobQueueReceiver =
    tokio::sync::mpsc::Receiver<(SubmittedJob, oneshot::Sender<Result<(), String>>)>;
pub type JobRepositorySender = tokio::sync::mpsc::Sender<JobRepositoryRequest>;
pub type JobRepositoryReceiver = tokio::sync::mpsc::Receiver<JobRepositoryRequest>;
pub type LatestBlockBroadcastReceiver = watch::Receiver<(Vec<u8>, u64, u64)>; // (blob, height, target)
pub type LatestBlockBroadcastSender = watch::Sender<(Vec<u8>, u64, u64)>; // (blob, height, target)
