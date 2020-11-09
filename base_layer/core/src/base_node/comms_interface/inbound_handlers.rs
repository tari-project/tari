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
use crate::{
    base_node::{
        comms_interface::{
            error::CommsInterfaceError,
            local_interface::BlockEventSender,
            NodeCommsRequest,
            NodeCommsResponse,
        },
        OutboundNodeCommsInterface,
    },
    blocks::{blockheader::BlockHeader, Block, NewBlock, NewBlockTemplate},
    chain_storage::{async_db, BlockAddResult, BlockchainBackend, BlockchainDatabase, HistoricalBlock},
    consensus::ConsensusManager,
    mempool::{async_mempool, Mempool},
    proof_of_work::{get_target_difficulty, Difficulty, PowAlgorithm},
    transactions::{transaction::TransactionKernel, TransactionOutput},
};
use croaring::Bitmap;
use log::*;
use std::{
    fmt::{Display, Error, Formatter},
    sync::Arc,
};
use strum_macros::Display;
use tari_comms::peer_manager::NodeId;
use tari_crypto::tari_utilities::{hash::Hashable, hex::Hex};
use tokio::sync::Semaphore;

const LOG_TARGET: &str = "c::bn::comms_interface::inbound_handler";
const MAX_HEADERS_PER_RESPONSE: u32 = 100;

/// Events that can be published on the Validated Block Event Stream
/// Broadcast is to notify subscribers if this is a valid propagated block event
#[derive(Debug, Clone, Display)]
pub enum BlockEvent {
    ValidBlockAdded(Arc<Block>, BlockAddResult, Broadcast),
    AddBlockFailed(Arc<Block>, Broadcast),
    BlockSyncComplete(Arc<Block>),
}

/// Used to notify if the block event is for a propagated block.
#[derive(Debug, Clone, Copy)]
pub struct Broadcast(bool);

impl Broadcast {
    #[inline]
    pub fn is_true(&self) -> bool {
        self.0
    }
}

#[allow(clippy::identity_op)]
impl Display for Broadcast {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        write!(f, "Broadcast[{}]", self.0)
    }
}

impl From<Broadcast> for bool {
    fn from(v: Broadcast) -> Self {
        v.0
    }
}

impl From<bool> for Broadcast {
    fn from(v: bool) -> Self {
        Broadcast(v)
    }
}

/// The InboundNodeCommsInterface is used to handle all received inbound requests from remote nodes.
pub struct InboundNodeCommsHandlers<T> {
    block_event_sender: BlockEventSender,
    blockchain_db: BlockchainDatabase<T>,
    mempool: Mempool,
    consensus_manager: ConsensusManager,
    new_block_request_semaphore: Arc<Semaphore>,
    outbound_nci: OutboundNodeCommsInterface,
}

impl<T> InboundNodeCommsHandlers<T>
where T: BlockchainBackend + 'static
{
    /// Construct a new InboundNodeCommsInterface.
    pub fn new(
        block_event_sender: BlockEventSender,
        blockchain_db: BlockchainDatabase<T>,
        mempool: Mempool,
        consensus_manager: ConsensusManager,
        outbound_nci: OutboundNodeCommsInterface,
    ) -> Self
    {
        Self {
            block_event_sender,
            blockchain_db,
            mempool,
            consensus_manager,
            new_block_request_semaphore: Arc::new(Semaphore::new(1)),
            outbound_nci,
        }
    }

    /// Handle inbound node comms requests from remote nodes and local services.
    pub async fn handle_request(&self, request: &NodeCommsRequest) -> Result<NodeCommsResponse, CommsInterfaceError> {
        debug!(target: LOG_TARGET, "Handling remote request {}", request);
        match request {
            NodeCommsRequest::GetChainMetadata => Ok(NodeCommsResponse::ChainMetadata(
                async_db::get_chain_metadata(self.blockchain_db.clone()).await?,
            )),
            NodeCommsRequest::FetchKernels(kernel_hashes) => {
                let mut kernels = Vec::<TransactionKernel>::new();
                for hash in kernel_hashes {
                    match async_db::fetch_kernel(self.blockchain_db.clone(), hash.clone()).await {
                        Ok(kernel) => kernels.push(kernel),
                        Err(err) => {
                            error!(target: LOG_TARGET, "Could not fetch kernel {}", err.to_string());
                            return Err(err.into());
                        },
                    }
                }
                Ok(NodeCommsResponse::TransactionKernels(kernels))
            },
            NodeCommsRequest::FetchHeaders(block_nums) => {
                let mut block_headers = Vec::<BlockHeader>::new();
                for block_num in block_nums {
                    match async_db::fetch_header(self.blockchain_db.clone(), *block_num).await {
                        Ok(block_header) => {
                            block_headers.push(block_header);
                        },
                        Err(err) => {
                            error!(target: LOG_TARGET, "Could not fetch headers: {}", err.to_string());
                            return Err(err.into());
                        },
                    }
                }
                Ok(NodeCommsResponse::BlockHeaders(block_headers))
            },
            NodeCommsRequest::FetchHeadersWithHashes(block_hashes) => {
                let mut block_headers = Vec::<BlockHeader>::new();
                for block_hash in block_hashes {
                    match async_db::fetch_header_by_block_hash(self.blockchain_db.clone(), block_hash.clone()).await {
                        Ok(block_header) => {
                            block_headers.push(block_header);
                        },
                        Err(err) => {
                            error!(
                                target: LOG_TARGET,
                                "Could not fetch headers with hashes:{}",
                                err.to_string()
                            );
                            return Err(err.into());
                        },
                    }
                }
                Ok(NodeCommsResponse::BlockHeaders(block_headers))
            },
            NodeCommsRequest::FetchHeadersAfter(header_hashes, stopping_hash) => {
                // Send from genesis block if none match
                let mut starting_block = async_db::fetch_header(self.blockchain_db.clone(), 0).await?;
                // Find first header that matches
                for header_hash in header_hashes {
                    match async_db::fetch_header_by_block_hash(self.blockchain_db.clone(), header_hash.clone()).await {
                        Ok(from_block) => {
                            starting_block = from_block;
                            break;
                        },
                        Err(err) => {
                            // Not an error. The header requested is simply not in our chain.
                            // Logging it as debug because it may not just be not found.
                            debug!(
                                target: LOG_TARGET,
                                "Skipping header {} when searching for matching headers in our chain. Err: {}",
                                header_hash.to_hex(),
                                err.to_string()
                            );
                        },
                    }
                }
                let mut headers = vec![];
                for i in 1..MAX_HEADERS_PER_RESPONSE {
                    match async_db::fetch_header(self.blockchain_db.clone(), starting_block.height + i as u64).await {
                        Ok(header) => {
                            let hash = header.hash();
                            headers.push(header);
                            if &hash == stopping_hash {
                                break;
                            }
                        },
                        Err(err) => {
                            error!(
                                target: LOG_TARGET,
                                "Could not fetch header at {}:{}",
                                starting_block.height + i as u64,
                                err.to_string()
                            );
                            return Err(err.into());
                        },
                    }
                }

                Ok(NodeCommsResponse::FetchHeadersAfterResponse(headers))
            },
            NodeCommsRequest::FetchMatchingUtxos(utxo_hashes) => {
                let mut utxos = Vec::<TransactionOutput>::new();
                for hash in utxo_hashes {
                    // Not finding a requested UTXO is not an error; UTXO validation depends on this
                    if let Ok(utxo) = async_db::fetch_utxo(self.blockchain_db.clone(), hash.clone()).await {
                        utxos.push(utxo);
                    }
                }
                Ok(NodeCommsResponse::TransactionOutputs(utxos))
            },
            NodeCommsRequest::FetchMatchingTxos(txo_hashes) => {
                let mut txos = Vec::<TransactionOutput>::new();
                for hash in txo_hashes {
                    // Not finding a requested TXO is not an error; TXO validation depends on this
                    if let Ok(Some(txo)) = async_db::fetch_txo(self.blockchain_db.clone(), hash.clone()).await {
                        txos.push(txo);
                    }
                }
                Ok(NodeCommsResponse::TransactionOutputs(txos))
            },
            NodeCommsRequest::FetchMatchingBlocks(block_nums) => {
                let mut blocks = Vec::<HistoricalBlock>::with_capacity(block_nums.len());
                for block_num in block_nums {
                    debug!(target: LOG_TARGET, "A peer has requested block {}", block_num);
                    match async_db::fetch_block(self.blockchain_db.clone(), *block_num).await {
                        Ok(block) => blocks.push(block),
                        // We need to suppress the error as another node might ask for a block we dont have, so we
                        // return ok([])
                        Err(e) => debug!(
                            target: LOG_TARGET,
                            "Could not provide requested block {} to peer because: {}",
                            block_num,
                            e.to_string()
                        ),
                    }
                }
                Ok(NodeCommsResponse::HistoricalBlocks(blocks))
            },
            NodeCommsRequest::FetchBlocksWithHashes(block_hashes) => {
                let mut blocks = Vec::<HistoricalBlock>::with_capacity(block_hashes.len());
                for block_hash in block_hashes {
                    debug!(
                        target: LOG_TARGET,
                        "A peer has requested a block with hash {}",
                        block_hash.to_hex()
                    );
                    match async_db::fetch_block_with_hash(self.blockchain_db.clone(), block_hash.clone()).await {
                        Ok(Some(block)) => blocks.push(block),
                        Ok(None) => warn!(
                            target: LOG_TARGET,
                            "Could not provide requested block {} to peer because not stored",
                            block_hash.to_hex(),
                        ),
                        Err(e) => warn!(
                            target: LOG_TARGET,
                            "Could not provide requested block {} to peer because: {}",
                            block_hash.to_hex(),
                            e.to_string()
                        ),
                    }
                }
                Ok(NodeCommsResponse::HistoricalBlocks(blocks))
            },
            NodeCommsRequest::FetchBlocksWithKernels(excess_sigs) => {
                let mut blocks = Vec::<HistoricalBlock>::with_capacity(excess_sigs.len());
                for sig in excess_sigs {
                    debug!(
                        target: LOG_TARGET,
                        "A peer has requested a block with kernel with sig {}",
                        sig.get_signature().to_hex(),
                    );
                    match async_db::fetch_block_with_kernel(self.blockchain_db.clone(), sig.clone()).await {
                        Ok(Some(block)) => blocks.push(block),
                        Ok(None) => warn!(
                            target: LOG_TARGET,
                            "Could not provide requested block containing kernel with sig {} to peer because not \
                             stored",
                            sig.get_signature().to_hex(),
                        ),
                        Err(e) => warn!(
                            target: LOG_TARGET,
                            "Could not provide requested block containing kernel with sig {} to peer because: {}",
                            sig.get_signature().to_hex(),
                            e.to_string()
                        ),
                    }
                }
                Ok(NodeCommsResponse::HistoricalBlocks(blocks))
            },
            NodeCommsRequest::FetchBlocksWithStxos(hashes) => {
                let mut blocks = Vec::<HistoricalBlock>::with_capacity(hashes.len());
                for hash in hashes {
                    debug!(
                        target: LOG_TARGET,
                        "A peer has requested a block with hash {}",
                        hash.to_hex()
                    );
                    match async_db::fetch_block_with_stxo(self.blockchain_db.clone(), hash.clone()).await {
                        Ok(Some(block)) => blocks.push(block),
                        Ok(None) => warn!(
                            target: LOG_TARGET,
                            "Could not provide requested block {} to peer because not stored",
                            hash.to_hex(),
                        ),
                        Err(e) => warn!(
                            target: LOG_TARGET,
                            "Could not provide requested block {} to peer because: {}",
                            hash.to_hex(),
                            e.to_string()
                        ),
                    }
                }
                Ok(NodeCommsResponse::HistoricalBlocks(blocks))
            },
            NodeCommsRequest::FetchBlocksWithUtxos(hashes) => {
                let mut blocks = Vec::<HistoricalBlock>::with_capacity(hashes.len());
                for hash in hashes {
                    debug!(
                        target: LOG_TARGET,
                        "A peer has requested a block with hash {}",
                        hash.to_hex()
                    );
                    match async_db::fetch_block_with_utxo(self.blockchain_db.clone(), hash.clone()).await {
                        Ok(Some(block)) => blocks.push(block),
                        Ok(None) => warn!(
                            target: LOG_TARGET,
                            "Could not provide requested block {} to peer because not stored",
                            hash.to_hex(),
                        ),
                        Err(e) => warn!(
                            target: LOG_TARGET,
                            "Could not provide requested block {} to peer because: {}",
                            hash.to_hex(),
                            e.to_string()
                        ),
                    }
                }
                Ok(NodeCommsResponse::HistoricalBlocks(blocks))
            },
            NodeCommsRequest::GetNewBlockTemplate(pow_algo) => {
                let metadata = async_db::get_chain_metadata(self.blockchain_db.clone()).await?;

                let best_block_hash = metadata
                    .best_block
                    .ok_or_else(|| CommsInterfaceError::UnexpectedApiResponse)?;
                let best_block_header =
                    async_db::fetch_header_by_block_hash(self.blockchain_db.clone(), best_block_hash).await?;

                let mut header = BlockHeader::from_previous(&best_block_header)?;
                let constants = self.consensus_manager.consensus_constants(header.height);
                header.version = constants.blockchain_version();
                header.pow.target_difficulty = self.get_target_difficulty(*pow_algo, header.height).await?;
                header.pow.pow_algo = *pow_algo;

                let transactions = async_mempool::retrieve(
                    self.mempool.clone(),
                    constants.get_max_block_weight_excluding_coinbase(),
                )
                .await?
                .iter()
                .map(|tx| (**tx).clone())
                .collect();

                let block_template =
                    NewBlockTemplate::from(header.into_builder().with_transactions(transactions).build());
                debug!(
                    target: LOG_TARGET,
                    "New block template requested at height {}", block_template.header.height
                );
                Ok(NodeCommsResponse::NewBlockTemplate(block_template))
            },
            NodeCommsRequest::GetNewBlock(block_template) => {
                let block = async_db::calculate_mmr_roots(self.blockchain_db.clone(), block_template.clone()).await?;
                Ok(NodeCommsResponse::NewBlock(block))
            },
            NodeCommsRequest::FetchMmrNodeCount(tree, height) => {
                let node_count = async_db::fetch_mmr_node_count(self.blockchain_db.clone(), *tree, *height).await?;
                Ok(NodeCommsResponse::MmrNodeCount(node_count))
            },
            NodeCommsRequest::FetchMatchingMmrNodes(tree, pos, count, hist_height) => {
                let mut added = Vec::<Vec<u8>>::with_capacity(*count as usize);
                let mut deleted = Bitmap::create();
                match async_db::fetch_mmr_nodes(self.blockchain_db.clone(), *tree, *pos, *count, Some(*hist_height))
                    .await
                {
                    Ok(mmr_nodes) => {
                        for (index, (leaf_hash, deletion_status)) in mmr_nodes.into_iter().enumerate() {
                            added.push(leaf_hash);
                            if deletion_status {
                                deleted.add(*pos + index as u32);
                            }
                        }
                        deleted.run_optimize();
                    },
                    // We need to suppress the error as another node might ask for mmr nodes we dont have, so we
                    // return ok([])
                    Err(e) => warn!(
                        target: LOG_TARGET,
                        "Could not provide requested mmr nodes (pos:{},count:{}) to peer because: {}",
                        pos,
                        count,
                        e.to_string()
                    ),
                }
                deleted.run_optimize();
                Ok(NodeCommsResponse::MmrNodes(added, deleted.serialize()))
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
    ) -> Result<(), CommsInterfaceError>
    {
        let NewBlock { block_hash } = new_block;

        // Only a single block request can complete at a time.
        // As multiple NewBlock requests arrive from propagation, this semaphore prevents multiple requests to nodes for
        // the same full block. The first request that succeeds will stop the node from requesting the block from any
        // other node (block_exists is true).
        let _permit = self.new_block_request_semaphore.acquire().await;

        if async_db::block_exists(self.blockchain_db.clone(), block_hash.clone()).await? {
            debug!(
                target: LOG_TARGET,
                "Block with hash `{}` already stored",
                block_hash.to_hex()
            );
            return Ok(());
        }

        debug!(
            target: LOG_TARGET,
            "Block with hash `{}` is unknown. Requesting it from peer `{}`.",
            block_hash.to_hex(),
            source_peer.short_str()
        );
        let mut block = self
            .outbound_nci
            .request_blocks_with_hashes_from_peer(vec![block_hash], Some(source_peer.clone()))
            .await?;

        match block.pop() {
            Some(block) => {
                self.handle_block(Arc::new(block.block), true.into(), Some(source_peer))
                    .await
            },
            None => {
                // TODO: #banheuristic - peer propagated block hash for which it could not return the full block
                debug!(
                    target: LOG_TARGET,
                    "Peer `{}` failed to return the block that was requested.",
                    source_peer.short_str()
                );
                Err(CommsInterfaceError::InvalidPeerResponse(format!(
                    "Invalid response from peer `{}`: Peer failed to provide the block that was propagated",
                    source_peer.short_str()
                )))
            },
        }
    }

    /// Handle inbound blocks from remote nodes and local services.
    pub async fn handle_block(
        &self,
        block: Arc<Block>,
        broadcast: Broadcast,
        source_peer: Option<NodeId>,
    ) -> Result<(), CommsInterfaceError>
    {
        let block_hash = block.hash();
        let block_height = block.header.height;
        debug!(
            target: LOG_TARGET,
            "Block #{} ({}) received from {}",
            block_height,
            block_hash.to_hex(),
            source_peer
                .as_ref()
                .map(|p| format!("remote peer: {}", p))
                .unwrap_or_else(|| "local services".to_string())
        );
        trace!(target: LOG_TARGET, "Block: {}", block);
        let add_block_result = async_db::add_block(self.blockchain_db.clone(), block.clone()).await;
        // Create block event on block event stream
        match add_block_result {
            Ok(block_add_result) => {
                trace!(target: LOG_TARGET, "Block event created: {}", block_add_result);

                let should_propagate = match &block_add_result {
                    BlockAddResult::Ok => true,
                    BlockAddResult::BlockExists => false,
                    BlockAddResult::OrphanBlock => false,
                    BlockAddResult::ChainReorg(_, _) => true,
                };

                self.publish_block_event(BlockEvent::ValidBlockAdded(block, block_add_result, broadcast));

                if should_propagate && broadcast.is_true() {
                    info!(
                        target: LOG_TARGET,
                        "Propagate block ({}) to network.",
                        block_hash.to_hex()
                    );
                    let exclude_peers = source_peer.into_iter().collect();
                    let new_block = NewBlock::new(block_hash);
                    self.outbound_nci.propagate_block(new_block, exclude_peers).await?;
                }
                Ok(())
            },
            Err(e) => {
                warn!(
                    target: LOG_TARGET,
                    "Block #{} ({}) validation failed: {:?}",
                    block_height,
                    block_hash.to_hex(),
                    e
                );
                self.publish_block_event(BlockEvent::AddBlockFailed(block, broadcast));
                Err(CommsInterfaceError::ChainStorageError(e))
            },
        }
    }

    fn publish_block_event(&self, event: BlockEvent) {
        if let Err(event) = self.block_event_sender.send(Arc::new(event)) {
            debug!(target: LOG_TARGET, "No event subscribers. Event {} dropped.", event.0)
        }
    }

    async fn get_target_difficulty(
        &self,
        pow_algo: PowAlgorithm,
        height: u64,
    ) -> Result<Difficulty, CommsInterfaceError>
    {
        let height_of_longest_chain = async_db::get_chain_metadata(self.blockchain_db.clone())
            .await?
            .height_of_longest_chain
            .ok_or_else(|| CommsInterfaceError::UnexpectedApiResponse)?;
        trace!(
            target: LOG_TARGET,
            "Calculating target difficulty at height:{} for PoW:{}",
            height,
            pow_algo
        );
        let constants = self.consensus_manager.consensus_constants(height);
        let block_window = constants.get_difficulty_block_window() as usize;
        let target_difficulties = async_db::fetch_target_difficulties(
            self.blockchain_db.clone(),
            pow_algo,
            height_of_longest_chain,
            block_window,
        )
        .await?;

        let target = get_target_difficulty(
            target_difficulties,
            block_window,
            constants.get_diff_target_block_interval(pow_algo),
            constants.min_pow_difficulty(pow_algo),
            constants.max_pow_difficulty(pow_algo),
            constants.get_difficulty_max_block_interval(pow_algo),
        )?;
        debug!(target: LOG_TARGET, "Target difficulty:{} for PoW:{}", target, pow_algo);
        Ok(target)
    }
}

impl<T> Clone for InboundNodeCommsHandlers<T> {
    fn clone(&self) -> Self {
        Self {
            block_event_sender: self.block_event_sender.clone(),
            blockchain_db: self.blockchain_db.clone(),
            mempool: self.mempool.clone(),
            consensus_manager: self.consensus_manager.clone(),
            new_block_request_semaphore: self.new_block_request_semaphore.clone(),
            outbound_nci: self.outbound_nci.clone(),
        }
    }
}
