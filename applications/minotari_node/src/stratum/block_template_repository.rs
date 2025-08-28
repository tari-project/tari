use std::sync::Arc;

use anyhow::anyhow;
use dashmap::DashMap;
use log::{debug, info, warn};
use minotari_app_grpc::tari_rpc::{MinerData, PowAlgo};
use tari_common_types::{
    key_branches::TransactionKeyManagerBranch,
    tari_address::TariAddress,
    types::{
        CompressedCommitment,
        CompressedPublicKey,
        UncompressedCommitment,
        UncompressedPublicKey,
        UncompressedSignature,
    },
};
use tari_comms::types::CompressedSignature;
use tari_core::{
    base_node::LocalNodeCommsInterface,
    blocks::Block,
    consensus::BaseConsensusManager,
    proof_of_work::cuckaroo_pow::pack_nonces,
    validation::tari_rx_vm_key_height,
};
use tari_shutdown::ShutdownSignal;
use tari_transaction_components::{
    generate_coinbase_with_wallet_output,
    key_manager::{TariKeyId, TransactionKeyManagerInterface, TxoStage},
    tari_amount::MicroMinotari,
    tari_proof_of_work::PowAlgorithm,
    transaction_components::{
        memo_field::{MemoField, TxType},
        CoinBaseExtra,
        KernelBuilder,
        RangeProofType,
        TransactionKernel,
        TransactionKernelVersion,
    },
};
use tari_transaction_key_manager::create_memory_db_key_manager;
use tokio::select;

use crate::stratum::SubmitJobQueueReceiver;

const LOG_TARGET: &str = "minotari::base_node::stratum::block_template_repository";

pub struct BlockTemplate {
    pub blob: Vec<u8>,
    pub height: u64,
    pub target: u64,
    pub seed_hash: Option<Vec<u8>>,
    pub prev_block_hash: Vec<u8>,
}
#[async_trait::async_trait]
pub trait BlockTemplateRepository {
    async fn get_block_template(
        &self,
        algo: PowAlgorithm,
        solo_address: Option<TariAddress>,
    ) -> Result<BlockTemplate, anyhow::Error>;
    async fn get_tip(&self) -> Result<(u64, Vec<u8>), anyhow::Error>;
}

#[derive(Clone)]
pub struct DefaultBlockTemplateRepository {
    node_service: LocalNodeCommsInterface,
    consensus_rules: BaseConsensusManager,
    block_templates: Arc<DashMap<Vec<u8>, Block>>,
}

impl DefaultBlockTemplateRepository {
    pub fn new(node_service: LocalNodeCommsInterface, consensus_rules: BaseConsensusManager) -> Self {
        Self {
            node_service,
            consensus_rules,
            block_templates: DashMap::new().into(),
        }
    }

    pub async fn start(
        &self,
        mut shutdown_signal: ShutdownSignal,
        mut job_receiver: SubmitJobQueueReceiver,
    ) -> Result<(), anyhow::Error> {
        let mut node_service = self.node_service.clone();
        let templates = self.block_templates.clone();
        tokio::spawn(async move {
            loop {
                select! {
                    _ = shutdown_signal.wait() => {
                        debug!(target: LOG_TARGET, "Shutting down Block Template Repository");
                        break;
                    }
                    Some((job, responder)) = job_receiver.recv() => {
                        debug!(target: LOG_TARGET, "Received job submission for job ID: {}", job.job_id);
                        let block_template =templates.get(&job.original_mining_hash);
                        if let Some(block) = block_template {
                            debug!(target: LOG_TARGET, "Found block template for job ID: {}", job.job_id);

                            let mut block = block.clone();
                            info!(target: LOG_TARGET, "Submitting block at height: {} for job ID: {} algo: {}", block.header.height, job.job_id, job.pow_algo);
                            match job.pow_algo {
                              PowAlgorithm::RandomXT => {
                                block.header.nonce = job.nonce;
                              }
                              PowAlgorithm::Sha3x => {
                                block.header.nonce = job.nonce.to_be();
                              }
                              PowAlgorithm::Cuckaroo => {
                                block.header.nonce = job.nonce;
                                block.header.pow.pow_algo = PowAlgorithm::Cuckaroo;
                                info!(target: LOG_TARGET, "Packing nonces for Cuckaroo job ID: {} with nonces: {:?}", job.job_id, job.cuckaroo_nonces);
                                block.header.pow.pow_data = match pack_nonces(&job.cuckaroo_nonces, 29).try_into() {
                                  Ok(r) => {
                                    info!(target: LOG_TARGET, "Packed nonces: {}", hex::encode(&r));
                                    r
                                },
                                  Err(e) => {
                                    warn!(target: LOG_TARGET, "Failed to pack nonces: {}", e);
                                    let _ = responder.send(Err(format!("Failed to pack nonces: {}", e)));
                                    continue;
                                  }
                                }
                              }
                              _ => {
                                warn!(target: LOG_TARGET, "Unsupported PoW algorithm for job ID: {}", job.job_id);
                                 let _ = responder.send(Err("Unsupported PoW algorithm".to_string()));

                                continue;
                              }
                            }

                            let res = node_service
                                .submit_block(block)
                                .await
                                .inspect_err(|e| warn!(target: LOG_TARGET, "Failed to submit block: {}", e))
                                .map(|_| ())
                                .map_err(|e| format!("Failed to submit block: {}", e));

                            let _ = responder.send(res);
                        } else {
                            warn!(target: LOG_TARGET, "No block template found for job ID: {}", job.job_id);
                            let _ = responder.send(Err(format!("No block template found for job ID: {}", job.job_id)));
                        }

                    }
                }
            }
        });
        Ok(())
    }

    pub async fn create_block(
        &self,
        algo: PowAlgorithm,
        solo_address: TariAddress,
    ) -> Result<(Block, Vec<u8>, Vec<u8>, MinerData), anyhow::Error> {
        debug!(target: LOG_TARGET, "Incoming request for get new block template");

        let mut handler = self.node_service.clone();
        let meta = handler.get_metadata().await?;

        let constants_weight = self
            .consensus_rules
            .consensus_constants(meta.best_block_height().saturating_add(1))
            .max_block_transaction_weight();

        let asking_weight = constants_weight;

        let mut new_template = handler
            .get_new_block_template(algo, asking_weight)
            .await
            .inspect_err(|e| {
                warn!(
                    target: LOG_TARGET,
                    "Could not get new block template: {}",
                    e
                )
            })?;

        let pow = algo as i32;

        let miner_data = MinerData {
            reward: new_template.reward.into(),
            target_difficulty: new_template.target_difficulty.as_u64(),
            total_fees: new_template.total_fees.into(),
            algo: Some(PowAlgo { pow_algo: pow }),
        };

        // let validate the coinbase amounts;
        let reward = self
            .consensus_rules
            .calculate_coinbase_and_fees(new_template.header.height, new_template.body.kernels())
            .map_err(|s| anyhow!("Could not calculate coinbase and fees:{}", s))?
            .as_u64();

        let key_manager = create_memory_db_key_manager()?;
        let height = new_template.header.height;
        let script_key_id = TariKeyId::default();

        let mut total_excess = UncompressedCommitment::default();
        let mut total_nonce = UncompressedPublicKey::default();
        let range_proof_type = RangeProofType::RevealedValue;
        let (_, coinbase_output, coinbase_kernel, wallet_output) = generate_coinbase_with_wallet_output(
            0.into(),
            MicroMinotari::from(reward),
            height,
            &CoinBaseExtra::try_from(vec![])?,
            &key_manager,
            &script_key_id,
            &solo_address,
            true,
            self.consensus_rules.consensus_constants(height),
            range_proof_type,
            MemoField::new_open(vec![], TxType::Coinbase).expect("empty user-data should always be valid"),
        )
        .await?;

        new_template.body.add_output(coinbase_output);
        let new_nonce = key_manager
            .get_next_key(TransactionKeyManagerBranch::KernelNonce.get_branch_key())
            .await?;

        total_nonce = &total_nonce +
            &new_nonce
                .pub_key
                .to_public_key()
                .map_err(|e| anyhow!("Failed to get public key: {}", e))?;
        total_excess = &total_excess +
            &coinbase_kernel
                .excess
                .to_commitment()
                .map_err(|e| anyhow!("Failed to get commitment: {}", e))?;
        let (spending_key_id, nonce) = (wallet_output.commitment_mask_key_id, new_nonce.key_id);
        let kernel_message = TransactionKernel::build_kernel_signature_message(
            &TransactionKernelVersion::get_current_version(),
            coinbase_kernel.fee,
            coinbase_kernel.lock_height,
            &coinbase_kernel.features,
            &None,
        );
        let mut kernel_signature = UncompressedSignature::default();
        kernel_signature = &kernel_signature +
            &key_manager
                .get_partial_txo_kernel_signature(
                    &spending_key_id,
                    &nonce,
                    &CompressedPublicKey::new_from_pk(total_nonce.clone()),
                    &CompressedPublicKey::new_from_pk(total_excess.as_public_key().clone()),
                    &TransactionKernelVersion::get_current_version(),
                    &kernel_message,
                    &coinbase_kernel.features,
                    TxoStage::Output,
                )
                .await?
                .to_schnorr_signature()
                .map_err(|e| anyhow!("Failed to get schnorr signature: {}", e))?;
        let kernel_new = KernelBuilder::new()
            .with_fee(0.into())
            .with_features(coinbase_kernel.features)
            .with_lock_height(coinbase_kernel.lock_height)
            .with_excess(&CompressedCommitment::from_commitment(total_excess))
            .with_signature(CompressedSignature::new_from_schnorr(kernel_signature))
            .build()
            .unwrap();

        new_template.body.add_kernel(kernel_new);
        new_template.body.sort();

        let new_block = match handler.get_new_block(new_template).await {
            Ok(b) => b,
            Err(e) => {
                warn!(target: LOG_TARGET, "Failed to get new block: {}", e);
                return Err(anyhow!("Failed to get new block: {}", e));
            },
        };
        let mining_hash = match new_block.header.pow.pow_algo {
            PowAlgorithm::Sha3x => new_block.header.mining_hash().to_vec(),
            PowAlgorithm::RandomXT => new_block.header.mining_hash().to_vec(),
            PowAlgorithm::RandomXM => new_block.header.merge_mining_hash().to_vec(),
            PowAlgorithm::Cuckaroo => new_block.header.mining_hash().to_vec(),
        };
        let vm_key = *handler
            .get_header(tari_rx_vm_key_height(new_block.header.height))
            .await?
            .unwrap()
            .hash();

        self.block_templates.insert(mining_hash.clone(), new_block.clone());

        Ok((new_block, mining_hash, vm_key.to_vec(), miner_data))
    }
}

#[async_trait::async_trait]
impl BlockTemplateRepository for DefaultBlockTemplateRepository {
    async fn get_block_template(
        &self,
        algo: PowAlgorithm,
        solo_address: Option<TariAddress>,
    ) -> Result<BlockTemplate, anyhow::Error> {
        if let Some(address) = solo_address {
            debug!(target: LOG_TARGET, "Creating block template for solo mining with address: {}", address);
            let (block, mining_hash, vm_key, miner_data) = self.create_block(algo, address).await?;
            Ok(BlockTemplate {
                blob: mining_hash,
                height: block.header.height,
                target: miner_data.target_difficulty,
                // extra_nonce: match algo {
                //     PowAlgorithm::RandomXT | PowAlgorithm::Sha3x | PowAlgorithm::RandomXM => None,
                //     PowAlgorithm::Cuckaroo => Some(block.header.nonce),
                // },
                seed_hash: match algo {
                    PowAlgorithm::RandomXT => Some(vm_key),
                    PowAlgorithm::Sha3x | PowAlgorithm::RandomXM | PowAlgorithm::Cuckaroo => None,
                },
                prev_block_hash: block.header.prev_hash.to_vec(),
            })
        } else {
            warn!(target: LOG_TARGET, "Cannot create block template for non-solo mining");
            Err(anyhow!("Block template creation for non-solo mining is not supported"))
        }
    }

    async fn get_tip(&self) -> Result<(u64, Vec<u8>), anyhow::Error> {
        let mut handler = self.node_service.clone();
        let meta = handler.get_metadata().await?;
        let best_block_height = meta.best_block_height();
        let best_block_hash = meta.best_block_hash().to_vec();
        Ok((best_block_height, best_block_hash))
    }
}
