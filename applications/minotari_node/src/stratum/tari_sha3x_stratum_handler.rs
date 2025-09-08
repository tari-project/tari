use std::{str::FromStr, vec};

use async_trait::async_trait;
use rand::{rngs::OsRng, RngCore};
use tari_common_types::tari_address::TariAddress;
use tari_transaction_components::tari_proof_of_work::PowAlgorithm;

use crate::stratum::{
    block_template_repository::{BlockTemplate, BlockTemplateRepository},
    job::{Job, SubmittedJob},
    job_repository::JobRepository,
    stratum_server::{
        AuthorizeResponse,
        LoginResponse,
        NotifyResponse,
        StratumJob,
        StratumJobHandler,
        SubmitResponse,
        SubscribeResponse,
    },
    SubmitJobQueueSender,
};

#[derive(Clone)]
pub(crate) struct TariSha3xStratumHandler<
    TJobRepo: JobRepository + Clone + Send + Sync + 'static,
    TBlockRepo: BlockTemplateRepository + Clone + Send + Sync + 'static,
> {
    block_template_repository: TBlockRepo,
    job_repository: TJobRepo,
    submit_job_queue_sender: SubmitJobQueueSender,
}

impl<
        TJobRepo: JobRepository + Clone + Send + Sync + 'static,
        TBlockRepo: BlockTemplateRepository + Clone + Send + Sync + 'static,
    > TariSha3xStratumHandler<TJobRepo, TBlockRepo>
{
    pub(crate) fn new(
        block_template_repository: TBlockRepo,
        job_repository: TJobRepo,
        submit_job_queue_sender: SubmitJobQueueSender,
    ) -> Self {
        Self {
            block_template_repository,
            job_repository,
            submit_job_queue_sender,
        }
    }
}

#[async_trait]
impl<
        TJobRepo: JobRepository + Clone + Send + Sync + 'static,
        TBlockRepo: BlockTemplateRepository + Clone + Send + Sync + 'static,
    > StratumJobHandler for TariSha3xStratumHandler<TJobRepo, TBlockRepo>
{
    async fn login(
        &self,
        _id: String,
        _login: String,
        address: String,
        algo: &[String],
    ) -> anyhow::Result<LoginResponse> {
        // Handle login request
        let address = TariAddress::from_str(&address)?;

        let main_algo;
        if algo.is_empty() {
            main_algo = "cuckaroo".to_string();
        } else if algo.len() == 1 {
            main_algo = algo[0].clone();
        } else if algo.iter().any(|a| a.as_str() == "rx/0") {
            main_algo = "RandomXT".to_string();
        } else {
            return Err(anyhow::anyhow!("Unsupported algorithm: {}", algo.join(", ")));
        }

        let algo = main_algo.parse()?;
        let BlockTemplate {
            blob,
            height,
            target,
            seed_hash,
            prev_block_hash,
            ..
        } = self
            .block_template_repository
            .get_block_template(algo, Some(address.clone()))
            .await?;
        let mut r = OsRng;
        let job_id = hex::encode(r.next_u64().to_le_bytes());

        let id = hex::encode(r.next_u64().to_le_bytes());
        let random_bytes = rand::random::<u16>();
        let job_target = hex::encode((u64::MAX / target).to_le_bytes());
        let chain_target = hex::encode(target.to_le_bytes());

        if blob.is_empty() {
            return Err(anyhow::anyhow!("No blob available for the latest block"));
        }
        let original_mining_hash = blob.clone();
        let blob = if main_algo == "RandomXT" {
            // The format is:
            // | 1 byte | 1 byte | 1 bytes | 32 bytes | 8 bytes | 1 byte | 32 bytes|
            // | major version | minor version | timestamp | mining_hash | nonce (big endian) | pow_algo | pow_data, excluding algo, padded to 32 bytes |
            //
            // Major version: 0
            // Minor version: 0
            // Timestamp: 0
            let mut final_blob = vec![0u8; 76];
            final_blob[0] = 0; // Major version
            final_blob[1] = 0; // Minor version
            final_blob[2] = 0; // Timestamp
            final_blob[3..35].copy_from_slice(&blob); // Mining hash
                                                      //   final_blob[35..43] nonce (0)
            final_blob[43] = 2; // Pow algorithm (2 for RandomXT)
            final_blob
        } else {
            blob
        };
        let job_record = Job {
            id: id.clone(),
            algo: main_algo.clone(),
            pow_algo: algo,
            blob: blob.clone(),
            height,
            target: job_target.clone(),
            job_id: job_id.clone(),
            created_at: chrono::Utc::now().naive_utc(),
            miner_address: address.clone(),
            chain_target: chain_target.clone(),
            xn: random_bytes,
            original_mining_hash,
            prev_block_hash: prev_block_hash.clone(),
        };
        self.job_repository.insert_job(job_record).await?;

        Ok(LoginResponse {
            id: id.to_string(),
            job: StratumJob {
                job_id: job_id.to_string(),
                algo: if main_algo == "RandomXT" {
                    "rx/0".to_string()
                } else {
                    main_algo
                },
                blob: hex::encode(blob),
                height,
                target: job_target,
                seed_hash: seed_hash.map(hex::encode),
                // xn,
            },
            status: "OK".to_string(),
        })
    }

    async fn submit(
        &self,
        job_id: String,
        nonce: u64,
        result: String,
        id: String,
        cuckaroo_nonces: Option<Vec<u64>>,
    ) -> anyhow::Result<SubmitResponse> {
        let job = self.job_repository.get_job(job_id.clone()).await?;
        if job.is_none() {
            return Err(anyhow::anyhow!("Job with id {} not found", job_id));
        }
        let job = job.unwrap();

        // Quick check for the target.
        let result = hex::decode(result)?;
        let target = &hex::decode(job.target)?;
        let result_u64 = if job.pow_algo == PowAlgorithm::RandomXT {
            let result = &result[result.len() - 8..];
            dbg!(&result);
            u64::from_le_bytes([
                result[0], result[1], result[2], result[3], result[4], result[5], result[6], result[7],
            ])
        } else {
            u64::from_be_bytes([
                result[0], result[1], result[2], result[3], result[4], result[5], result[6], result[7],
            ])
        };
        let target_u64 = u64::from_le_bytes([
            target[0], target[1], target[2], target[3], target[4], target[5], target[6], target[7],
        ]);
        if result_u64 > target_u64 {
            return Err(anyhow::anyhow!("Result is greater than target"));
        }

        let (tx, rx) = tokio::sync::oneshot::channel();
        let _res = self
            .submit_job_queue_sender
            .send((
                SubmittedJob {
                    job_id: job.job_id.to_string(),
                    nonce,
                    pow_algo: job.pow_algo.clone(),
                    original_mining_hash: job.original_mining_hash.clone(),
                    cuckaroo_nonces: cuckaroo_nonces.unwrap_or_default(),
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

    async fn subscribe(&self, _id: String, _agent: String) -> anyhow::Result<SubscribeResponse> {
        let mut r = OsRng;
        let subscription_id = hex::encode(r.next_u64().to_le_bytes());

        let nonce = rand::random::<u16>();

        let xn = hex::encode(&nonce.to_le_bytes());

        Ok(SubscribeResponse {
            subscription_id: subscription_id.clone(),
            nonce_hex: xn.clone(),
            nonce,
        })
    }

    async fn authorize(
        &self,
        _id: String,
        main_algo: String,
        login: String,
        _worker_name: Option<String>,
        _pass: String,
        nonce: Option<u16>,
    ) -> anyhow::Result<AuthorizeResponse> {
        let algo = main_algo.parse()?;
        let address = TariAddress::from_str(&login)?;

        let job_record = create_job_from_blob(&self.block_template_repository, algo, address, nonce).await?;
        self.job_repository.insert_job(job_record.clone()).await?;

        Ok(AuthorizeResponse {
            blob: hex::encode(job_record.blob.clone()),
            extra_nonce_hex: job_record.xn.to_string(),
            height: job_record.height,
        })
    }

    async fn check_notify_needed(&self, last_job_id: String) -> Result<Option<NotifyResponse>, anyhow::Error> {
        let last_job = self.job_repository.get_job(last_job_id.clone()).await?;

        if last_job.is_none() {
            return Err(anyhow::anyhow!("Job with id {} not found", last_job_id));
        }

        let last_job = last_job.unwrap();

        let (best_height, best_block_header) = self.block_template_repository.get_tip().await?;

        if last_job.height == best_height.saturating_add(1) && last_job.prev_block_hash == best_block_header {
            // No need to notify, the job is still valid
            return Ok(None);
        }

        // Else, let's make a new job.
        let job_record = create_job_from_blob(
            &self.block_template_repository,
            last_job.pow_algo,
            last_job.miner_address.clone(),
            Some(last_job.xn.clone()),
        )
        .await?;
        self.job_repository.insert_job(job_record.clone()).await?;
        Ok(Some(NotifyResponse {
            job_id: job_record.job_id,
            height: job_record.height,
            blob: hex::encode(job_record.blob),
            extra_nonce_hex: hex::encode(job_record.xn.to_le_bytes()),
        }))
    }
}

async fn create_job_from_blob<TBlockRepo: BlockTemplateRepository>(
    block_template_repository: &TBlockRepo,
    algo: PowAlgorithm,
    address: TariAddress,
    _nonce: Option<u16>,
) -> anyhow::Result<Job> {
    let BlockTemplate {
        blob,
        height,
        target,
        prev_block_hash,
        ..
    } = block_template_repository
        .get_block_template(algo, Some(address.clone()))
        .await?;
    let mut r = OsRng;
    let job_id = hex::encode(r.next_u64().to_le_bytes());

    let id = hex::encode(r.next_u64().to_le_bytes());
    let random_bytes = rand::random::<u16>();
    let job_target = hex::encode((u64::MAX / target).to_le_bytes());
    let chain_target = hex::encode(target.to_le_bytes());

    if blob.is_empty() {
        return Err(anyhow::anyhow!("No blob available for the latest block"));
    }
    let original_mining_hash = blob.clone();
    let blob = if algo == PowAlgorithm::RandomXT {
        // The format is:
        // | 1 byte | 1 byte | 1 bytes | 32 bytes | 8 bytes | 1 byte | 32 bytes|
        // | major version | minor version | timestamp | mining_hash | nonce (big endian) | pow_algo | pow_data, excluding algo, padded to 32 bytes |
        //
        // Major version: 0
        // Minor version: 0
        // Timestamp: 0
        let mut final_blob = vec![0u8; 76];
        final_blob[0] = 0; // Major version
        final_blob[1] = 0; // Minor version
        final_blob[2] = 0; // Timestamp
        final_blob[3..35].copy_from_slice(&blob); // Mining hash
                                                  //   final_blob[35..43] nonce (0)
        final_blob[43] = 2; // Pow algorithm (2 for RandomXT)
        final_blob
    } else {
        blob
    };
    let job_record = Job {
        id: id.clone(),
        algo: algo.to_string(),
        pow_algo: algo,
        blob: blob.clone(),
        height,
        target: job_target.clone(),
        job_id: job_id.clone(),
        created_at: chrono::Utc::now().naive_utc(),
        miner_address: address.clone(),
        chain_target: chain_target.clone(),
        xn: random_bytes,
        original_mining_hash,
        prev_block_hash: prev_block_hash.clone(),
    };
    Ok(job_record)
}
