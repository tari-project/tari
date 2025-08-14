use std::{cmp, str::FromStr, vec};

use async_trait::async_trait;
use minotari_app_grpc::conversions::chain_metadata;
use rand::{rngs::OsRng, RngCore};
use tari_common_types::tari_address::TariAddress;
use tari_core::proof_of_work::PowAlgorithm;
use tokio::sync::watch;

use crate::stratum::{
    block_template_repository::{BlockTemplate, BlockTemplateRepository},
    job::{Job, SubmittedJob},
    job_repository::JobRepository,
    stratum_server::{
        AuthorizeResponse,
        LoginResponse,
        StratumJob,
        StratumJobHandler,
        SubmitResponse,
        SubscribeResponse,
    },
    LatestBlockBroadcastReceiver,
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
        id: String,
        login: String,
        is_solo: bool,
        algo: &[String],
        pass: String,
        agent: String,
        endpoint_difficulty: u64,
    ) -> anyhow::Result<LoginResponse> {
        // Handle login request
        let solo_address = if is_solo {
            Some(TariAddress::from_str(&login)?)
        } else {
            None
        };

        let main_algo;
        if algo.is_empty() {
            main_algo = "sha3x".to_string();
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
            extra_nonce,
        } = self
            .block_template_repository
            .get_block_template(algo, solo_address)
            .await?;
        let mut r = OsRng;
        let job_id = hex::encode(r.next_u64().to_le_bytes());

        let id = hex::encode(r.next_u64().to_le_bytes());
        let random_bytes = rand::random::<u16>();
        let xn = hex::encode(&random_bytes.to_le_bytes());
        // let algo = "sha3x".to_string();
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
            miner_address: login.clone(),
            chain_target: chain_target.clone(),
            xn: random_bytes,
            original_mining_hash,
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

    async fn submit(&self, job_id: String, nonce: u64, result: String, id: String) -> anyhow::Result<SubmitResponse> {
        let job = self.job_repository.get_job(job_id.clone()).await?;
        if job.is_none() {
            return Err(anyhow::anyhow!("Job with id {} not found", job_id));
        }
        let job = job.unwrap();

        // Quick check the nonce against extra nonce.
        let extra_nonce = job.xn;
        // dbg!(extra_nonce);
        // dbg!(extra_nonce.to_le_bytes());
        let nonce_bytes = nonce.to_be_bytes();
        // dbg!(nonce_bytes);
        // if nonce.to_be_bytes()[..2] != extra_nonce.to_le_bytes()[..2] {
        //     return Err(anyhow::anyhow!("Nonce does not match extra nonce"));
        // }

        // Quick check for the target.
        let result = hex::decode(result)?;
        let target = &hex::decode(job.target)?;
        dbg!(&result);
        dbg!(&target);
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
        dbg!(result_u64);
        dbg!(target_u64);
        if result_u64 > target_u64 {
            dbg!("Result is greater than target");
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
                    pow_algo: job.pow_algo.clone(),
                    target: target_u64,
                    chain_target: chain_target_u64,
                    blob: job.blob.clone(),
                    original_mining_hash: job.original_mining_hash.clone(),
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

    async fn subscribe(&self, id: String, agent: String) -> anyhow::Result<SubscribeResponse> {
        dbg!("here");
        // let algo = main_algo.parse()?;
        // let algo = main_algo.parse()?;
        // let solo_address = if is_solo {
        //     Some(TariAddress::from_str(&address)?)
        // } else {
        //     None
        // };
        // dbg!("here");
        // let BlockTemplate {
        //     blob,
        //     height,
        //     target,
        //     seed_hash,
        //     extra_nonce,
        // } = self
        //     .block_template_repository
        //     .get_block_template(algo, solo_address)
        //     .await?;
        // dbg!("here");
        let mut r = OsRng;
        let subscription_id = hex::encode(r.next_u64().to_le_bytes());
        // let job_id = hex::encode(r.next_u64().to_le_bytes());

        // let id = hex::encode(r.next_u64().to_le_bytes());
        let random_bytes = rand::random::<u16>();
        let xn = hex::encode(&random_bytes.to_le_bytes());
        // let algo = "sha3x".to_string();
        // let job_target = hex::encode((u64::MAX / target).to_le_bytes());
        // let chain_target = hex::encode(target.to_le_bytes());

        // if blob.is_empty() {
        // return Err(anyhow::anyhow!("No blob available for the latest block"));
        // }
        // let original_mining_hash = blob.clone();
        // let blob = if main_algo == "RandomXT" {
        // The format is:
        // | 1 byte | 1 byte | 1 bytes | 32 bytes | 8 bytes | 1 byte | 32 bytes|
        // | major version | minor version | timestamp | mining_hash | nonce (big endian) | pow_algo | pow_data, excluding algo, padded to 32 bytes |
        //
        // Major version: 0
        // Minor version: 0
        // Timestamp: 0
        //     let mut final_blob = vec![0u8; 76];
        //     final_blob[0] = 0; // Major version
        //     final_blob[1] = 0; // Minor version
        //     final_blob[2] = 0; // Timestamp
        //     final_blob[3..35].copy_from_slice(&blob); // Mining hash
        //                                               //   final_blob[35..43] nonce (0)
        //     final_blob[43] = 2; // Pow algorithm (2 for RandomXT)
        //     final_blob
        // } else {
        //     blob
        // };
        // dbg!("here");
        // let job_record = Job {
        //     id: id.clone(),
        //     algo: main_algo.clone(),
        //     pow_algo: algo,
        //     blob: blob.clone(),
        //     height,
        //     target: job_target.clone(),
        //     job_id: job_id.clone(),
        //     created_at: chrono::Utc::now().naive_utc(),
        //     miner_address: address.clone(),
        //     chain_target: chain_target.clone(),
        //     xn: random_bytes,
        //     original_mining_hash,
        // };
        // dbg!("here");
        // self.job_repository.insert_job(job_record).await?;

        // Ok(LoginResponse {
        //     id: id.to_string(),
        //     job: StratumJob {
        //         job_id: job_id.to_string(),
        //         algo: if main_algo == "RandomXT" {
        //             "rx/0".to_string()
        //         } else {
        //             main_algo
        //         },
        //         blob: hex::encode(blob),
        //         height,
        //         target: job_target,
        //         seed_hash: seed_hash.map(hex::encode),
        //         // xn,
        //     },
        //     status: "OK".to_string(),
        // })
        dbg!("here");

        Ok(SubscribeResponse {
            // difficulty: job_target,
            // block_template: hex::encode(blob),
            // nonce: xn,
            // height,
            subscription_id: subscription_id.clone(),
            extra_nonce: xn.clone(),
        })
    }

    async fn authorize(
        &self,
        id: String,
        main_algo: String,
        is_solo: bool,
        login: String,
        worker_name: Option<String>,
        pass: String,
        nonce: Option<String>,
    ) -> anyhow::Result<AuthorizeResponse> {
        dbg!("here");
        let algo = main_algo.parse()?;
        let solo_address = if is_solo {
            Some(TariAddress::from_str(&login)?)
        } else {
            None
        };
        // dbg!("here");
        let BlockTemplate {
            blob,
            height,
            target,
            seed_hash,
            extra_nonce,
        } = self
            .block_template_repository
            .get_block_template(algo, solo_address)
            .await?;
        dbg!("here");
        let mut r = OsRng;
        // let subscription_id = hex::encode(r.next_u64().to_le_bytes());
        let job_id = hex::encode(r.next_u64().to_le_bytes());

        let id = hex::encode(r.next_u64().to_le_bytes());
        let random_bytes = rand::random::<u16>();
        let xn = nonce
            .clone()
            .unwrap_or_else(|| hex::encode(&random_bytes.to_le_bytes()));
        // let algo = "sha3x".to_string();
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
        dbg!("here");
        let job_record = Job {
            id: id.clone(),
            algo: main_algo.clone(),
            pow_algo: algo,
            blob: blob.clone(),
            height,
            target: job_target.clone(),
            job_id: job_id.clone(),
            created_at: chrono::Utc::now().naive_utc(),
            miner_address: login.clone(),
            chain_target: chain_target.clone(),
            xn: random_bytes,
            original_mining_hash,
        };
        dbg!("here");
        self.job_repository.insert_job(job_record).await?;

        Ok(AuthorizeResponse {
            difficulty: job_target,
            block_template: hex::encode(blob),
            nonce: xn,
            height,
            // subscription_id: subscription_id.clone(),
            // extra_nonce: xn.clone(),
        })
    }
}
