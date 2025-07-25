use std::cmp;

use async_trait::async_trait;
use minotari_app_grpc::conversions::chain_metadata;
use tokio::sync::watch;

use crate::stratum::{
    job::{Job, SubmittedJob},
    job_repository::JobRepository,
    stratum_server::{LoginResponse, StratumJob, StratumJobHandler, SubmitResponse},
    LatestBlockBroadcastReceiver,
    SubmitJobQueueSender,
};

#[derive(Clone)]
pub(crate) struct TariSha3xStratumHandler<T: JobRepository + Clone + Send + Sync + 'static> {
    latest_block_blob_template: LatestBlockBroadcastReceiver,
    job_repository: T,
    submit_job_queue_sender: SubmitJobQueueSender,
}

impl<T: JobRepository + Clone + Send + Sync + 'static> TariSha3xStratumHandler<T> {
    pub(crate) fn new(
        latest_block_blob_template: LatestBlockBroadcastReceiver,
        job_repository: T,
        submit_job_queue_sender: SubmitJobQueueSender,
    ) -> Self {
        Self {
            latest_block_blob_template,
            job_repository,
            submit_job_queue_sender,
        }
    }
}

#[async_trait]
impl<T: JobRepository + Clone + Send + Sync + 'static> StratumJobHandler for TariSha3xStratumHandler<T> {
    async fn login(
        &self,
        id: String,
        login: String,
        pass: String,
        agent: String,
        endpoint_difficulty: u64,
    ) -> anyhow::Result<LoginResponse> {
        // Handle login request
        let (blob, height, target) = self.latest_block_blob_template.borrow().clone();
        let job_id = uuid::Uuid::new_v4();
        let id = uuid::Uuid::new_v4();
        let random_bytes = rand::random::<u16>();
        let xn = hex::encode(&random_bytes.to_le_bytes());
        let algo = "sha3x".to_string();
        let job_target = hex::encode((u64::MAX / cmp::min(target, endpoint_difficulty)).to_le_bytes());
        let chain_target = hex::encode(target.to_le_bytes());

        let job_record = Job {
            id,
            algo: algo.clone(),
            blob: blob.clone(),
            height,
            target: job_target.clone(),
            job_id: job_id.clone(),
            created_at: chrono::Utc::now().naive_utc(),
            miner_address: login.clone(),
            chain_target: chain_target.clone(),
            xn: random_bytes as u32,
        };
        let job = self.job_repository.insert_job(job_record).await?;

        Ok(LoginResponse {
            id: id.to_string(),
            job: StratumJob {
                job_id: job_id.to_string(),
                algo: algo.clone(),
                blob: hex::encode(blob),
                height,
                target: job_target,
                xn,
            },
        })
    }

    async fn submit(&self, job_id: String, nonce: u64, result: String, id: String) -> anyhow::Result<SubmitResponse> {
        let job_id = uuid::Uuid::parse_str(&job_id)?;
        let job = self.job_repository.get_job(job_id).await?;
        if job.is_none() {
            return Err(anyhow::anyhow!("Job not found"));
        }
        let job = job.unwrap();

        // Quick check the nonce against extra nonce.
        let extra_nonce = job.xn;
        dbg!(extra_nonce);
        if nonce.to_le_bytes()[..2] != extra_nonce.to_le_bytes()[..2] {
            return Err(anyhow::anyhow!("Nonce does not match extra nonce"));
        }

        // Quick check for the target.
        let result = hex::decode(result)?;
        let target = &hex::decode(job.target)?;
        dbg!(&result);
        dbg!(&target);
        let result_u64 = u64::from_be_bytes([
            result[0], result[1], result[2], result[3], result[4], result[5], result[6], result[7],
        ]);
        let target_u64 = u64::from_le_bytes([
            target[0], target[1], target[2], target[3], target[4], target[5], target[6], target[7],
        ]);
        dbg!(result_u64);
        dbg!(target_u64);
        if result_u64 > target_u64 {
            return Err(anyhow::anyhow!("Result is greater than target"));
        }
        let chain_target = hex::decode(job.chain_target)?;
        let chain_target_u64 = u64::from_le_bytes([
            chain_target[0],
            chain_target[1],
            chain_target[2],
            chain_target[3],
            chain_target[4],
            chain_target[5],
            chain_target[6],
            chain_target[7],
        ]);

        let (tx, rx) = tokio::sync::oneshot::channel();
        let _res = self
            .submit_job_queue_sender
            .send((
                SubmittedJob {
                    job_id: job.job_id.to_string(),
                    nonce,
                    algo: job.algo.clone(),
                    target: target_u64,
                    chain_target: chain_target_u64,
                    blob: job.blob.clone(),
                    miner_address: job.miner_address.clone(),
                    result,
                },
                tx,
            ))
            .await?;
        let res = rx.await?;
        match res {
            Ok(_) => {
                // Handle successful submission
                Ok(SubmitResponse {
                    id,
                    result: true, // Placeholder for actual result
                })
            },
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to submit job: {}", e));
            },
        }
        // Handle submit request
    }
}
