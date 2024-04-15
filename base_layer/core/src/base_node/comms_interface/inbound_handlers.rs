// Copyright 2019. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

#[cfg(feature = "metrics")]
use std::convert::{TryFrom, TryInto};
use std::{cmp::max, collections::HashSet, sync::Arc, time::Instant};

use log::*;
use strum_macros::Display;
use tari_common_types::types::{BlockHash, FixedHash, HashOutput};
use tari_comms::{connectivity::ConnectivityRequester, peer_manager::NodeId};
use tari_utilities::hex::Hex;
use tokio::sync::RwLock;

#[cfg(feature = "metrics")]
use crate::base_node::metrics;
use crate::{
    base_node::comms_interface::{
        error::CommsInterfaceError,
        local_interface::BlockEventSender,
        FetchMempoolTransactionsResponse,
        NodeCommsRequest,
        NodeCommsResponse,
        OutboundNodeCommsInterface,
    },
    blocks::{Block, BlockBuilder, BlockHeader, BlockHeaderValidationError, ChainBlock, NewBlock, NewBlockTemplate},
    chain_storage::{async_db::AsyncBlockchainDb, BlockAddResult, BlockchainBackend, ChainStorageError},
    consensus::{ConsensusConstants, ConsensusManager},
    mempool::Mempool,
    proof_of_work::{
        randomx_difficulty,
        randomx_factory::RandomXFactory,
        sha3x_difficulty,
        Difficulty,
        PowAlgorithm,
        PowError,
    },
    transactions::aggregated_body::AggregateBody,
    validation::{helpers, ValidationError},
};

const LOG_TARGET: &str = "c::bn::comms_interface::inbound_handler";
const MAX_REQUEST_BY_BLOCK_HASHES: usize = 100;
const MAX_REQUEST_BY_KERNEL_EXCESS_SIGS: usize = 100;
const MAX_REQUEST_BY_UTXO_HASHES: usize = 100;

/// Events that can be published on the Validated Block Event Stream
/// Broadcast is to notify subscribers if this is a valid propagated block event
#[derive(Debug, Clone, Display)]
pub enum BlockEvent {
    ValidBlockAdded(Arc<Block>, BlockAddResult),
    AddBlockValidationFailed {
        block: Arc<Block>,
        source_peer: Option<NodeId>,
    },
    AddBlockErrored {
        block: Arc<Block>,
    },
    BlockSyncComplete(Arc<ChainBlock>, u64),
    BlockSyncRewind(Vec<Arc<ChainBlock>>),
}

/// The InboundNodeCommsInterface is used to handle all received inbound requests from remote nodes.
pub struct InboundNodeCommsHandlers<B> {
    block_event_sender: BlockEventSender,
    blockchain_db: AsyncBlockchainDb<B>,
    mempool: Mempool,
    consensus_manager: ConsensusManager,
    list_of_reconciling_blocks: Arc<RwLock<HashSet<HashOutput>>>,
    outbound_nci: OutboundNodeCommsInterface,
    connectivity: ConnectivityRequester,
    randomx_factory: RandomXFactory,
}

impl<B> InboundNodeCommsHandlers<B>
where B: BlockchainBackend + 'static
{
    /// Construct a new InboundNodeCommsInterface.
    pub fn new(
        block_event_sender: BlockEventSender,
        blockchain_db: AsyncBlockchainDb<B>,
        mempool: Mempool,
        consensus_manager: ConsensusManager,
        outbound_nci: OutboundNodeCommsInterface,
        connectivity: ConnectivityRequester,
        randomx_factory: RandomXFactory,
    ) -> Self {
        Self {
            block_event_sender,
            blockchain_db,
            mempool,
            consensus_manager,
            list_of_reconciling_blocks: Arc::new(RwLock::new(HashSet::new())),
            outbound_nci,
            connectivity,
            randomx_factory,
        }
    }

    /// Handle inbound node comms requests from remote nodes and local services.
    #[allow(clippy::too_many_lines)]
    pub async fn handle_request(&self, request: NodeCommsRequest) -> Result<NodeCommsResponse, CommsInterfaceError> {
        debug!(target: LOG_TARGET, "Handling remote request {}", request);
        match request {
            NodeCommsRequest::GetChainMetadata => Ok(NodeCommsResponse::ChainMetadata(
                self.blockchain_db.get_chain_metadata().await?,
            )),
            NodeCommsRequest::FetchHeaders(range) => {
                let headers = self.blockchain_db.fetch_chain_headers(range).await?;
                Ok(NodeCommsResponse::BlockHeaders(headers))
            },
            NodeCommsRequest::FetchHeadersByHashes(block_hashes) => {
                if block_hashes.len() > MAX_REQUEST_BY_BLOCK_HASHES {
                    return Err(CommsInterfaceError::InvalidRequest {
                        request: "FetchHeadersByHashes",
                        details: format!(
                            "Exceeded maximum block hashes request (max: {}, got:{})",
                            MAX_REQUEST_BY_BLOCK_HASHES,
                            block_hashes.len()
                        ),
                    });
                }
                let mut block_headers = Vec::with_capacity(block_hashes.len());
                for block_hash in block_hashes {
                    let block_hex = block_hash.to_hex();
                    match self.blockchain_db.fetch_chain_header_by_block_hash(block_hash).await? {
                        Some(block_header) => {
                            block_headers.push(block_header);
                        },
                        None => {
                            error!(target: LOG_TARGET, "Could not fetch headers with hashes:{}", block_hex);
                            return Err(CommsInterfaceError::InternalError(format!(
                                "Could not fetch headers with hashes:{}",
                                block_hex
                            )));
                        },
                    }
                }
                Ok(NodeCommsResponse::BlockHeaders(block_headers))
            },
            NodeCommsRequest::FetchMatchingUtxos(utxo_hashes) => {
                let mut res = Vec::with_capacity(utxo_hashes.len());
                for (output, spent) in (self
                    .blockchain_db
                    .fetch_outputs_with_spend_status_at_tip(utxo_hashes)
                    .await?)
                    .into_iter()
                    .flatten()
                {
                    if !spent {
                        res.push(output);
                    }
                }
                Ok(NodeCommsResponse::TransactionOutputs(res))
            },
            NodeCommsRequest::FetchMatchingBlocks { range, compact } => {
                let blocks = self.blockchain_db.fetch_blocks(range, compact).await?;
                Ok(NodeCommsResponse::HistoricalBlocks(blocks))
            },
            NodeCommsRequest::FetchBlocksByKernelExcessSigs(excess_sigs) => {
                if excess_sigs.len() > MAX_REQUEST_BY_KERNEL_EXCESS_SIGS {
                    return Err(CommsInterfaceError::InvalidRequest {
                        request: "FetchBlocksByKernelExcessSigs",
                        details: format!(
                            "Exceeded maximum number of kernel excess sigs in request (max: {}, got:{})",
                            MAX_REQUEST_BY_KERNEL_EXCESS_SIGS,
                            excess_sigs.len()
                        ),
                    });
                }
                let mut blocks = Vec::with_capacity(excess_sigs.len());
                for sig in excess_sigs {
                    let sig_hex = sig.get_signature().to_hex();
                    debug!(
                        target: LOG_TARGET,
                        "A peer has requested a block with kernel with sig {}", sig_hex
                    );
                    match self.blockchain_db.fetch_block_with_kernel(sig).await {
                        Ok(Some(block)) => blocks.push(block),
                        Ok(None) => warn!(
                            target: LOG_TARGET,
                            "Could not provide requested block containing kernel with sig {} to peer because not \
                             stored",
                            sig_hex
                        ),
                        Err(e) => warn!(
                            target: LOG_TARGET,
                            "Could not provide requested block containing kernel with sig {} to peer because: {}",
                            sig_hex,
                            e.to_string()
                        ),
                    }
                }
                Ok(NodeCommsResponse::HistoricalBlocks(blocks))
            },
            NodeCommsRequest::FetchBlocksByUtxos(commitments) => {
                if commitments.len() > MAX_REQUEST_BY_UTXO_HASHES {
                    return Err(CommsInterfaceError::InvalidRequest {
                        request: "FetchBlocksByUtxos",
                        details: format!(
                            "Exceeded maximum number of utxo hashes in request (max: {}, got:{})",
                            MAX_REQUEST_BY_UTXO_HASHES,
                            commitments.len()
                        ),
                    });
                }
                let mut blocks = Vec::with_capacity(commitments.len());
                for commitment in commitments {
                    let commitment_hex = commitment.to_hex();
                    debug!(
                        target: LOG_TARGET,
                        "A peer has requested a block with commitment {}", commitment_hex,
                    );
                    match self.blockchain_db.fetch_block_with_utxo(commitment).await {
                        Ok(Some(block)) => blocks.push(block),
                        Ok(None) => warn!(
                            target: LOG_TARGET,
                            "Could not provide requested block with commitment {} to peer because not stored",
                            commitment_hex,
                        ),
                        Err(e) => warn!(
                            target: LOG_TARGET,
                            "Could not provide requested block with commitment {} to peer because: {}",
                            commitment_hex,
                            e.to_string()
                        ),
                    }
                }
                Ok(NodeCommsResponse::HistoricalBlocks(blocks))
            },
            NodeCommsRequest::GetHeaderByHash(hash) => {
                let header = self.blockchain_db.fetch_chain_header_by_block_hash(hash).await?;
                Ok(NodeCommsResponse::BlockHeader(header))
            },
            NodeCommsRequest::GetBlockByHash(hash) => {
                let block = self.blockchain_db.fetch_block_by_hash(hash, false).await?;
                Ok(NodeCommsResponse::HistoricalBlock(Box::new(block)))
            },
            NodeCommsRequest::GetNewBlockTemplate(request) => {
                let best_block_header = self.blockchain_db.fetch_tip_header().await?;
                let mut header = BlockHeader::from_previous(best_block_header.header());
                let constants = self.consensus_manager.consensus_constants(header.height);
                header.version = constants.blockchain_version();
                header.pow.pow_algo = request.algo;

                let constants_weight = constants
                    .max_block_weight_excluding_coinbase()
                    .map_err(|e| CommsInterfaceError::InternalError(e.to_string()))?;
                let asking_weight = if request.max_weight > constants_weight || request.max_weight == 0 {
                    constants_weight
                } else {
                    request.max_weight
                };

                debug!(
                    target: LOG_TARGET,
                    "Fetching transactions with a maximum weight of {} for the template", asking_weight
                );
                let transactions = self
                    .mempool
                    .retrieve(asking_weight)
                    .await?
                    .into_iter()
                    .map(|tx| Arc::try_unwrap(tx).unwrap_or_else(|tx| (*tx).clone()))
                    .collect::<Vec<_>>();

                debug!(
                    target: LOG_TARGET,
                    "Adding {} transaction(s) to new block template",
                    transactions.len(),
                );

                let prev_hash = header.prev_hash;
                let height = header.height;

                let block = header.into_builder().with_transactions(transactions).build();
                let block_hash = block.hash();
                let block_template = NewBlockTemplate::from_block(
                    block,
                    self.get_target_difficulty_for_next_block(request.algo, constants, prev_hash)
                        .await?,
                    self.consensus_manager.get_block_reward_at(height),
                )?;

                debug!(target: LOG_TARGET,
                    "New block template requested and prepared at height: #{}, target difficulty: {}, block hash: `{}`, weight: {}, {}",
                    block_template.header.height,
                    block_template.target_difficulty,
                    block_hash.to_hex(),
                    block_template
                        .body
                        .calculate_weight(constants.transaction_weight_params())
                        .map_err(|e| CommsInterfaceError::InternalError(e.to_string()))?,
                    block_template.body.to_counts_string()
                );

                Ok(NodeCommsResponse::NewBlockTemplate(block_template))
            },
            NodeCommsRequest::GetNewBlock(block_template) => {
                let height = block_template.header.height;
                let target_difficulty = block_template.target_difficulty;
                let block = self.blockchain_db.prepare_new_block(block_template).await?;
                let constants = self.consensus_manager.consensus_constants(block.header.height);
                debug!(target: LOG_TARGET,
                    "Prepared block: #{}, target difficulty: {}, block hash: `{}`, weight: {}, {}",
                    height,
                    target_difficulty,
                    block.hash().to_hex(),
                    block
                        .body
                        .calculate_weight(constants.transaction_weight_params())
                        .map_err(|e| CommsInterfaceError::InternalError(e.to_string()))?,
                    block.body.to_counts_string()
                );
                Ok(NodeCommsResponse::NewBlock {
                    success: true,
                    error: None,
                    block: Some(block),
                })
            },
            NodeCommsRequest::GetBlockFromAllChains(hash) => {
                let block_hex = hash.to_hex();
                debug!(
                    target: LOG_TARGET,
                    "A peer has requested a block with hash {}", block_hex
                );

                #[allow(clippy::blocks_in_conditions)]
                let maybe_block = match self
                    .blockchain_db
                    .fetch_block_by_hash(hash, true)
                    .await
                    .unwrap_or_else(|e| {
                        warn!(
                            target: LOG_TARGET,
                            "Could not provide requested block {} to peer because: {}",
                            block_hex,
                            e.to_string()
                        );

                        None
                    }) {
                    None => self.blockchain_db.fetch_orphan(hash).await.map_or_else(
                        |e| {
                            warn!(
                                target: LOG_TARGET,
                                "Could not provide requested block {} to peer because: {}", block_hex, e,
                            );

                            None
                        },
                        Some,
                    ),
                    Some(block) => Some(block.into_block()),
                };

                Ok(NodeCommsResponse::Block(Box::new(maybe_block)))
            },
            NodeCommsRequest::FetchKernelByExcessSig(signature) => {
                let kernels = match self.blockchain_db.fetch_kernel_by_excess_sig(signature).await {
                    Ok(Some((kernel, _))) => vec![kernel],
                    Ok(None) => vec![],
                    Err(err) => {
                        error!(target: LOG_TARGET, "Could not fetch kernel {}", err.to_string());
                        return Err(err.into());
                    },
                };

                Ok(NodeCommsResponse::TransactionKernels(kernels))
            },
            NodeCommsRequest::FetchMempoolTransactionsByExcessSigs { excess_sigs } => {
                let (transactions, not_found) = self.mempool.retrieve_by_excess_sigs(excess_sigs).await?;
                Ok(NodeCommsResponse::FetchMempoolTransactionsByExcessSigsResponse(
                    FetchMempoolTransactionsResponse {
                        transactions,
                        not_found,
                    },
                ))
            },
            NodeCommsRequest::FetchValidatorNodesKeys { height } => {
                let active_validator_nodes = self.blockchain_db.fetch_active_validator_nodes(height).await?;
                Ok(NodeCommsResponse::FetchValidatorNodesKeysResponse(
                    active_validator_nodes,
                ))
            },
            NodeCommsRequest::GetShardKey { height, public_key } => {
                let shard_key = self.blockchain_db.get_shard_key(height, public_key).await?;
                Ok(NodeCommsResponse::GetShardKeyResponse(shard_key))
            },
            NodeCommsRequest::FetchTemplateRegistrations {
                start_height,
                end_height,
            } => {
                let template_registrations = self
                    .blockchain_db
                    .fetch_template_registrations(start_height..=end_height)
                    .await?;
                Ok(NodeCommsResponse::FetchTemplateRegistrationsResponse(
                    template_registrations,
                ))
            },
            NodeCommsRequest::FetchUnspentUtxosInBlock { block_hash } => {
                let utxos = self.blockchain_db.fetch_outputs_in_block(block_hash).await?;
                Ok(NodeCommsResponse::TransactionOutputs(utxos))
            },
        }
    }

    /// Handles a `NewBlock` message. Only a single `NewBlock` message can be handled at once to prevent extraneous
    /// requests for the full block.
    /// This may (asynchronously) block until the other request(s) complete or time out and so should typically be
    /// executed in a dedicated task.
    pub async fn handle_new_block_message(
        &mut self,
        new_block: NewBlock,
        source_peer: NodeId,
    ) -> Result<(), CommsInterfaceError> {
        let block_hash = new_block.header.hash();

        if self.blockchain_db.inner().is_add_block_disabled() {
            info!(
                target: LOG_TARGET,
                "Ignoring block message ({}) because add_block is locked",
                block_hash.to_hex()
            );
            return Ok(());
        }

        // Lets check if the block exists before we try and ask for a complete block
        if self.check_exists_and_not_bad_block(block_hash).await? {
            return Ok(());
        }

        // lets check that the difficulty at least matches 50% of the tip header. The max difficulty drop is 16%, thus
        // 50% is way more than that and in order to attack the node, you need 50% of the mining power. We cannot check
        // the target difficulty as orphan blocks dont have a target difficulty. All we care here is that bad
        // blocks are not free to make, and that they are more expensive to make then they are to validate. As
        // soon as a block can be linked to the main chain, a proper full proof of work check will
        // be done before any other validation.
        self.check_min_block_difficulty(&new_block).await?;

        {
            // we use a double lock to make sure we can only reconcile one unique block at a time. We may receive the
            // same block from multiple peer near simultaneously. We should only reconcile each unique block once.
            let read_lock = self.list_of_reconciling_blocks.read().await;
            if read_lock.contains(&block_hash) {
                debug!(
                    target: LOG_TARGET,
                    "Block with hash `{}` is already being reconciled",
                    block_hash.to_hex()
                );
                return Ok(());
            }
        }
        {
            let mut write_lock = self.list_of_reconciling_blocks.write().await;
            if self.check_exists_and_not_bad_block(block_hash).await? {
                return Ok(());
            }

            if !write_lock.insert(block_hash) {
                debug!(
                    target: LOG_TARGET,
                    "Block with hash `{}` is already being reconciled",
                    block_hash.to_hex()
                );
                return Ok(());
            }
        }

        debug!(
            target: LOG_TARGET,
            "Block with hash `{}` is unknown. Constructing block from known mempool transactions / requesting missing \
             transactions from peer '{}'.",
            block_hash.to_hex(),
            source_peer
        );

        let result = self.reconcile_and_add_block(source_peer.clone(), new_block).await;

        {
            let mut write_lock = self.list_of_reconciling_blocks.write().await;
            write_lock.remove(&block_hash);
        }
        result?;
        Ok(())
    }

    async fn check_min_block_difficulty(&self, new_block: &NewBlock) -> Result<(), CommsInterfaceError> {
        let constants = self.consensus_manager.consensus_constants(new_block.header.height);
        let gen_hash = *self.consensus_manager.get_genesis_block().hash();
        let mut min_difficulty = constants.min_pow_difficulty(new_block.header.pow.pow_algo);
        let mut header = self.blockchain_db.fetch_last_chain_header().await?;
        loop {
            if new_block.header.pow_algo() == header.header().pow_algo() {
                min_difficulty = max(
                    header
                        .accumulated_data()
                        .target_difficulty
                        .checked_div_u64(2)
                        .unwrap_or(min_difficulty),
                    min_difficulty,
                );
                break;
            }
            if header.height() == 0 {
                break;
            }
            // we have not reached gen block, and the pow algo does not match, so lets go further back
            header = self
                .blockchain_db
                .fetch_chain_header(header.height().saturating_sub(1))
                .await?;
        }
        let achieved = match new_block.header.pow_algo() {
            PowAlgorithm::RandomX => randomx_difficulty(
                &new_block.header,
                &self.randomx_factory,
                &gen_hash,
                &self.consensus_manager,
            )?,
            PowAlgorithm::Sha3x => sha3x_difficulty(&new_block.header)?,
        };
        if achieved < min_difficulty {
            return Err(CommsInterfaceError::InvalidBlockHeader(
                BlockHeaderValidationError::ProofOfWorkError(PowError::AchievedDifficultyBelowMin),
            ));
        }
        Ok(())
    }

    async fn check_exists_and_not_bad_block(&self, block: FixedHash) -> Result<bool, CommsInterfaceError> {
        if self.blockchain_db.chain_header_or_orphan_exists(block).await? {
            debug!(
                target: LOG_TARGET,
                "Block with hash `{}` already stored",
                block.to_hex()
            );
            return Ok(true);
        }
        let block_exist = self.blockchain_db.bad_block_exists(block).await?;
        if block_exist.0 {
            debug!(
                target: LOG_TARGET,
                "Block with hash `{}` already validated as a bad block due to {}",
                block.to_hex(), block_exist.1
            );
            return Err(CommsInterfaceError::ChainStorageError(
                ChainStorageError::ValidationError {
                    source: ValidationError::BadBlockFound {
                        hash: block.to_hex(),
                        reason: block_exist.1,
                    },
                },
            ));
        }
        Ok(false)
    }

    async fn reconcile_and_add_block(
        &mut self,
        source_peer: NodeId,
        new_block: NewBlock,
    ) -> Result<(), CommsInterfaceError> {
        let block = self.reconcile_block(source_peer.clone(), new_block).await?;
        self.handle_block(block, Some(source_peer)).await?;
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    async fn reconcile_block(
        &mut self,
        source_peer: NodeId,
        new_block: NewBlock,
    ) -> Result<Block, CommsInterfaceError> {
        let NewBlock {
            header,
            coinbase_kernels,
            coinbase_outputs,
            kernel_excess_sigs: excess_sigs,
        } = new_block;
        // If the block is empty, we dont have to ask for the block, as we already have the full block available
        // to us.
        if excess_sigs.is_empty() {
            let block = BlockBuilder::new(header.version)
                .add_outputs(coinbase_outputs)
                .add_kernels(coinbase_kernels)
                .with_header(header)
                .build();
            return Ok(block);
        }

        let block_hash = header.hash();
        // We check the current tip and orphan status of the block because we cannot guarantee that mempool state is
        // correct and the mmr root calculation is only valid if the block is building on the tip.
        let current_meta = self.blockchain_db.get_chain_metadata().await?;
        if header.prev_hash != *current_meta.best_block_hash() {
            debug!(
                target: LOG_TARGET,
                "Orphaned block #{}: ({}), current tip is: #{} ({}). We need to fetch the complete block from peer: \
                 ({})",
                header.height,
                block_hash.to_hex(),
                current_meta.best_block_height(),
                current_meta.best_block_hash().to_hex(),
                source_peer,
            );
            #[allow(clippy::cast_possible_wrap)]
            #[cfg(feature = "metrics")]
            metrics::compact_block_tx_misses(header.height).set(excess_sigs.len() as i64);
            let block = self.request_full_block_from_peer(source_peer, block_hash).await?;
            return Ok(block);
        }

        // We know that the block is neither and orphan or a coinbase, so lets ask our mempool for the transactions
        let (known_transactions, missing_excess_sigs) = self.mempool.retrieve_by_excess_sigs(excess_sigs).await?;
        let known_transactions = known_transactions.into_iter().map(|tx| (*tx).clone()).collect();

        #[allow(clippy::cast_possible_wrap)]
        #[cfg(feature = "metrics")]
        metrics::compact_block_tx_misses(header.height).set(missing_excess_sigs.len() as i64);

        let mut builder = BlockBuilder::new(header.version)
            .add_outputs(coinbase_outputs)
            .add_kernels(coinbase_kernels)
            .with_transactions(known_transactions);

        if missing_excess_sigs.is_empty() {
            debug!(
                target: LOG_TARGET,
                "All transactions for block #{} ({}) found in mempool",
                header.height,
                block_hash.to_hex()
            );
        } else {
            debug!(
                target: LOG_TARGET,
                "Requesting {} unknown transaction(s) from peer '{}'.",
                missing_excess_sigs.len(),
                source_peer
            );

            let FetchMempoolTransactionsResponse {
                transactions,
                not_found,
            } = self
                .outbound_nci
                .request_transactions_by_excess_sig(source_peer.clone(), missing_excess_sigs)
                .await?;

            // Add returned transactions to unconfirmed pool
            if !transactions.is_empty() {
                self.mempool.insert_all(transactions.clone()).await?;
            }

            if !not_found.is_empty() {
                warn!(
                    target: LOG_TARGET,
                    "Peer {} was not able to return all transactions for block #{} ({}). {} transaction(s) not found. \
                     Requesting full block.",
                    source_peer,
                    header.height,
                    block_hash.to_hex(),
                    not_found.len()
                );

                #[cfg(feature = "metrics")]
                metrics::compact_block_full_misses(header.height).inc();
                let block = self.request_full_block_from_peer(source_peer, block_hash).await?;
                return Ok(block);
            }

            builder = builder.with_transactions(
                transactions
                    .into_iter()
                    .map(|tx| Arc::try_unwrap(tx).unwrap_or_else(|tx| (*tx).clone()))
                    .collect(),
            );
        }

        // NB: Add the header last because `with_transactions` etc updates the current header, but we have the final one
        // already
        builder = builder.with_header(header.clone());
        let block = builder.build();

        // Perform a sanity check on the reconstructed block, if the MMR roots don't match then it's possible one or
        // more transactions in our mempool had the same excess/signature for a *different* transaction.
        // This is extremely unlikely, but still possible. In case of a mismatch, request the full block from the peer.
        let (block, mmr_roots) = match self.blockchain_db.calculate_mmr_roots(block).await {
            Err(_) => {
                let block = self.request_full_block_from_peer(source_peer, block_hash).await?;
                return Ok(block);
            },
            Ok(v) => v,
        };
        if let Err(e) = helpers::check_mmr_roots(&header, &mmr_roots) {
            warn!(
                target: LOG_TARGET,
                "Reconstructed block #{} ({}) failed MMR check validation!. Requesting full block. Error: {}",
                header.height,
                block_hash.to_hex(),
                e,
            );

            #[cfg(feature = "metrics")]
            metrics::compact_block_mmr_mismatch(header.height).inc();
            let block = self.request_full_block_from_peer(source_peer, block_hash).await?;
            return Ok(block);
        }

        Ok(block)
    }

    async fn request_full_block_from_peer(
        &mut self,
        source_peer: NodeId,
        block_hash: BlockHash,
    ) -> Result<Block, CommsInterfaceError> {
        match self
            .outbound_nci
            .request_blocks_by_hashes_from_peer(block_hash, Some(source_peer.clone()))
            .await
        {
            Ok(Some(block)) => Ok(block),
            Ok(None) => {
                debug!(
                    target: LOG_TARGET,
                    "Peer `{}` failed to return the block that was requested.", source_peer
                );
                Err(CommsInterfaceError::InvalidPeerResponse(format!(
                    "Invalid response from peer `{}`: Peer failed to provide the block that was propagated",
                    source_peer
                )))
            },
            Err(CommsInterfaceError::UnexpectedApiResponse) => {
                debug!(
                    target: LOG_TARGET,
                    "Peer `{}` sent unexpected API response.", source_peer
                );
                Err(CommsInterfaceError::UnexpectedApiResponse)
            },
            Err(e) => Err(e),
        }
    }

    /// Handle inbound blocks from remote nodes and local services.
    ///
    /// ## Arguments
    /// block - the block to store
    /// new_block_msg - propagate this new block message
    /// source_peer - the peer that sent this new block message, or None if the block was generated by a local miner
    pub async fn handle_block(
        &mut self,
        block: Block,
        source_peer: Option<NodeId>,
    ) -> Result<BlockHash, CommsInterfaceError> {
        let block_hash = block.hash();
        let block_height = block.header.height;

        info!(
            target: LOG_TARGET,
            "Block #{} ({}) received from {}",
            block_height,
            block_hash.to_hex(),
            source_peer
                .as_ref()
                .map(|p| format!("remote peer: {}", p))
                .unwrap_or_else(|| "local services".to_string())
        );
        debug!(target: LOG_TARGET, "Incoming block: {}", block);
        let timer = Instant::now();
        let block = self.hydrate_block(block).await?;

        let add_block_result = self.blockchain_db.add_block(block.clone()).await;
        // Create block event on block event stream
        match add_block_result {
            Ok(block_add_result) => {
                debug!(
                    target: LOG_TARGET,
                    "Block #{} ({}) added ({}) to blockchain in {:.2?}",
                    block_height,
                    block_hash.to_hex(),
                    block_add_result,
                    timer.elapsed()
                );

                let should_propagate = match &block_add_result {
                    BlockAddResult::Ok(_) => true,
                    BlockAddResult::BlockExists => false,
                    BlockAddResult::OrphanBlock => false,
                    BlockAddResult::ChainReorg { .. } => true,
                };

                #[cfg(feature = "metrics")]
                self.update_block_result_metrics(&block_add_result).await?;

                self.publish_block_event(BlockEvent::ValidBlockAdded(block.clone(), block_add_result));

                if should_propagate {
                    debug!(
                        target: LOG_TARGET,
                        "Propagate block ({}) to network.",
                        block_hash.to_hex()
                    );
                    let exclude_peers = source_peer.into_iter().collect();
                    let new_block_msg = NewBlock::from(&*block);
                    if let Err(e) = self.outbound_nci.propagate_block(new_block_msg, exclude_peers).await {
                        warn!(
                            target: LOG_TARGET,
                            "Failed to propagate block ({}) to network: {}.",
                            block_hash.to_hex(), e
                        );
                    }
                }
                Ok(block_hash)
            },

            Err(e @ ChainStorageError::ValidationError { .. }) => {
                #[cfg(feature = "metrics")]
                {
                    let block_hash = block.hash();
                    metrics::rejected_blocks(block.header.height, &block_hash).inc();
                }
                warn!(
                    target: LOG_TARGET,
                    "Peer {} sent an invalid block: {}",
                    source_peer
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| "<local request>".to_string()),
                    e
                );
                self.publish_block_event(BlockEvent::AddBlockValidationFailed { block, source_peer });
                Err(e.into())
            },

            Err(e) => {
                #[cfg(feature = "metrics")]
                metrics::rejected_blocks(block.header.height, &block.hash()).inc();

                self.publish_block_event(BlockEvent::AddBlockErrored { block });
                Err(e.into())
            },
        }
    }

    async fn hydrate_block(&mut self, block: Block) -> Result<Arc<Block>, CommsInterfaceError> {
        let block_hash = block.hash();
        let block_height = block.header.height;
        if block.body.inputs().is_empty() {
            debug!(
                target: LOG_TARGET,
                "Block #{} ({}) contains no inputs so nothing to hydrate",
                block_height,
                block_hash.to_hex(),
            );
            return Ok(Arc::new(block));
        }

        let timer = Instant::now();
        let (header, mut inputs, outputs, kernels) = block.dissolve();

        let db = self.blockchain_db.inner().db_read_access()?;
        for input in &mut inputs {
            if !input.is_compact() {
                continue;
            }

            let output_mined_info =
                db.fetch_output(&input.output_hash())?
                    .ok_or_else(|| CommsInterfaceError::InvalidFullBlock {
                        hash: block_hash,
                        details: format!("Output {} to be spent does not exist in db", input.output_hash()),
                    })?;

            let rp_hash = match output_mined_info.output.proof {
                Some(proof) => proof.hash(),
                None => FixedHash::zero(),
            };
            input.add_output_data(
                output_mined_info.output.version,
                output_mined_info.output.features,
                output_mined_info.output.commitment,
                output_mined_info.output.script,
                output_mined_info.output.sender_offset_public_key,
                output_mined_info.output.covenant,
                output_mined_info.output.encrypted_data,
                output_mined_info.output.metadata_signature,
                rp_hash,
                output_mined_info.output.minimum_value_promise,
            );
        }
        debug!(
            target: LOG_TARGET,
            "Hydrated block #{} ({}) with {} input(s) in {:.2?}",
            block_height,
            block_hash.to_hex(),
            inputs.len(),
            timer.elapsed()
        );
        let block = Block::new(header, AggregateBody::new(inputs, outputs, kernels));
        Ok(Arc::new(block))
    }

    fn publish_block_event(&self, event: BlockEvent) {
        if let Err(event) = self.block_event_sender.send(Arc::new(event)) {
            debug!(target: LOG_TARGET, "No event subscribers. Event {} dropped.", event.0)
        }
    }

    #[cfg(feature = "metrics")]
    async fn update_block_result_metrics(&self, block_add_result: &BlockAddResult) -> Result<(), CommsInterfaceError> {
        fn update_target_difficulty(block: &ChainBlock) {
            match block.header().pow_algo() {
                PowAlgorithm::Sha3x => {
                    metrics::target_difficulty_sha()
                        .set(i64::try_from(block.accumulated_data().target_difficulty.as_u64()).unwrap_or(i64::MAX));
                },
                PowAlgorithm::RandomX => {
                    metrics::target_difficulty_randomx()
                        .set(i64::try_from(block.accumulated_data().target_difficulty.as_u64()).unwrap_or(i64::MAX));
                },
            }
        }

        match block_add_result {
            BlockAddResult::Ok(ref block) => {
                update_target_difficulty(block);
                #[allow(clippy::cast_possible_wrap)]
                metrics::tip_height().set(block.height() as i64);
                let utxo_set_size = self.blockchain_db.utxo_count().await?;
                metrics::utxo_set_size().set(utxo_set_size.try_into().unwrap_or(i64::MAX));
            },
            BlockAddResult::ChainReorg { added, removed } => {
                if let Some(fork_height) = added.last().map(|b| b.height()) {
                    #[allow(clippy::cast_possible_wrap)]
                    metrics::tip_height().set(fork_height as i64);
                    metrics::reorg(fork_height, added.len(), removed.len()).inc();

                    let utxo_set_size = self.blockchain_db.utxo_count().await?;
                    metrics::utxo_set_size().set(utxo_set_size.try_into().unwrap_or(i64::MAX));
                }
                for block in added {
                    update_target_difficulty(block);
                }
            },
            BlockAddResult::OrphanBlock => {
                metrics::orphaned_blocks().inc();
            },
            _ => {},
        }
        Ok(())
    }

    async fn get_target_difficulty_for_next_block(
        &self,
        pow_algo: PowAlgorithm,
        constants: &ConsensusConstants,
        current_block_hash: HashOutput,
    ) -> Result<Difficulty, CommsInterfaceError> {
        let target_difficulty = self
            .blockchain_db
            .fetch_target_difficulty_for_next_block(pow_algo, current_block_hash)
            .await?;

        let target = target_difficulty.calculate(
            constants.min_pow_difficulty(pow_algo),
            constants.max_pow_difficulty(pow_algo),
        );
        debug!(target: LOG_TARGET, "Target difficulty {} for PoW {}", target, pow_algo);
        Ok(target)
    }
}

impl<B> Clone for InboundNodeCommsHandlers<B> {
    fn clone(&self) -> Self {
        Self {
            block_event_sender: self.block_event_sender.clone(),
            blockchain_db: self.blockchain_db.clone(),
            mempool: self.mempool.clone(),
            consensus_manager: self.consensus_manager.clone(),
            list_of_reconciling_blocks: self.list_of_reconciling_blocks.clone(),
            outbound_nci: self.outbound_nci.clone(),
            connectivity: self.connectivity.clone(),
            randomx_factory: self.randomx_factory.clone(),
        }
    }
}
