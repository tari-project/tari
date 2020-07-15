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
    chain_storage::{
        async_db,
        BlockAddResult,
        BlockchainBackend,
        BlockchainDatabase,
        ChainStorageError,
        HistoricalBlock,
    },
    consensus::ConsensusManager,
    mempool::{async_mempool, Mempool},
    proof_of_work::{get_target_difficulty, Difficulty, PowAlgorithm},
    transactions::transaction::{TransactionKernel, TransactionOutput},
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
    Verified((Box<Block>, BlockAddResult, Broadcast)),
    Invalid((Box<Block>, ChainStorageError, Broadcast)),
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
    mempool: Mempool<T>,
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
        mempool: Mempool<T>,
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
                async_db::get_metadata(self.blockchain_db.clone()).await?,
            )),
            NodeCommsRequest::FetchKernels(kernel_hashes) => {
                let mut kernels = Vec::<TransactionKernel>::new();
                for hash in kernel_hashes {
                    if let Ok(kernel) = async_db::fetch_kernel(self.blockchain_db.clone(), hash.clone()).await {
                        kernels.push(kernel);
                    }
                }
                Ok(NodeCommsResponse::TransactionKernels(kernels))
            },
            NodeCommsRequest::FetchHeaders(block_nums) => {
                let mut block_headers = Vec::<BlockHeader>::new();
                for block_num in block_nums {
                    if let Ok(block_header) = async_db::fetch_header(self.blockchain_db.clone(), *block_num).await {
                        block_headers.push(block_header);
                    }
                }
                Ok(NodeCommsResponse::BlockHeaders(block_headers))
            },
            NodeCommsRequest::FetchHeadersWithHashes(block_hashes) => {
                let mut block_headers = Vec::<BlockHeader>::new();
                for block_hash in block_hashes {
                    if let Ok(block_header) =
                        async_db::fetch_header_with_block_hash(self.blockchain_db.clone(), block_hash.clone()).await
                    {
                        block_headers.push(block_header);
                    }
                }
                Ok(NodeCommsResponse::BlockHeaders(block_headers))
            },
            NodeCommsRequest::FetchHeadersAfter(header_hashes, stopping_hash) => {
                // Send from genesis block if none match
                let mut starting_block = async_db::fetch_header(self.blockchain_db.clone(), 0).await?;
                // Find first header that matches
                for header_hash in header_hashes {
                    if let Ok(from_block) =
                        async_db::fetch_header_with_block_hash(self.blockchain_db.clone(), header_hash.clone()).await
                    {
                        starting_block = from_block;
                        break;
                    }
                }
                let mut headers = vec![];
                for i in 1..MAX_HEADERS_PER_RESPONSE {
                    if let Ok(header) =
                        async_db::fetch_header(self.blockchain_db.clone(), starting_block.height + i as u64).await
                    {
                        let hash = header.hash();
                        headers.push(header);
                        if &hash == stopping_hash {
                            break;
                        }
                    }
                }

                Ok(NodeCommsResponse::FetchHeadersAfterResponse(headers))
            },
            NodeCommsRequest::FetchUtxos(utxo_hashes) => {
                let mut utxos = Vec::<TransactionOutput>::new();
                for hash in utxo_hashes {
                    if let Ok(utxo) = async_db::fetch_utxo(self.blockchain_db.clone(), hash.clone()).await {
                        utxos.push(utxo);
                    }
                }
                Ok(NodeCommsResponse::TransactionOutputs(utxos))
            },
            NodeCommsRequest::FetchTxos(txo_hashes) => {
                let mut txos = Vec::<TransactionOutput>::new();
                for hash in txo_hashes {
                    if let Ok(Some(txo)) = async_db::fetch_txo(self.blockchain_db.clone(), hash.clone()).await {
                        txos.push(txo);
                    }
                }
                Ok(NodeCommsResponse::TransactionOutputs(txos))
            },
            NodeCommsRequest::FetchBlocks(block_nums) => {
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
                let metadata = async_db::get_metadata(self.blockchain_db.clone()).await?;
                let best_block_hash = metadata
                    .best_block
                    .ok_or_else(|| CommsInterfaceError::UnexpectedApiResponse)?;
                let best_block_header =
                    async_db::fetch_header_with_block_hash(self.blockchain_db.clone(), best_block_hash).await?;

                let constants = self.consensus_manager.consensus_constants();
                let mut header = BlockHeader::from_previous(&best_block_header);
                header.version = constants.blockchain_version();
                header.pow.target_difficulty = self.get_target_difficulty(*pow_algo).await?;

                let transactions = async_mempool::retrieve(
                    self.mempool.clone(),
                    self.consensus_manager
                        .consensus_constants()
                        .get_max_block_weight_excluding_coinbase(),
                )
                .await
                .map_err(|e| CommsInterfaceError::MempoolError(e.to_string()))?
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
            NodeCommsRequest::GetTargetDifficulty(pow_algo) => Ok(NodeCommsResponse::TargetDifficulty(
                self.get_target_difficulty(*pow_algo).await?,
            )),
            NodeCommsRequest::FetchMmrNodeCount(tree, height) => {
                let node_count =
                    async_db::fetch_mmr_node_count(self.blockchain_db.clone(), tree.clone(), *height).await?;
                Ok(NodeCommsResponse::MmrNodeCount(node_count))
            },
            NodeCommsRequest::FetchMmrNodes(tree, pos, count, hist_height) => {
                let mut added = Vec::<Vec<u8>>::with_capacity(*count as usize);
                let mut deleted = Bitmap::create();
                match async_db::fetch_mmr_nodes(
                    self.blockchain_db.clone(),
                    tree.clone(),
                    *pos,
                    *count,
                    Some(*hist_height),
                )
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
                    Err(e) => debug!(
                        target: LOG_TARGET,
                        "Could not provide requested mmr nodes (pos:{},count:{}) to peer because: {}",
                        pos,
                        count,
                        e.to_string()
                    ),
                }
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
            Some(block) => self.handle_block(block.block, true.into(), Some(source_peer)).await,
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
        block: Block,
        broadcast: Broadcast,
        source_peer: Option<NodeId>,
    ) -> Result<(), CommsInterfaceError>
    {
        let block_hash = block.hash();
        debug!(
            target: LOG_TARGET,
            "Block #{} ({}) received from {}",
            block.header.height,
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
                    BlockAddResult::ChainReorg(_) => true,
                };

                self.publish_block_event(BlockEvent::Verified((Box::new(block), block_add_result, broadcast)));

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
                    block.header.height,
                    block_hash.to_hex(),
                    e
                );
                self.publish_block_event(BlockEvent::Invalid((Box::new(block), e.clone(), broadcast)));
                Err(CommsInterfaceError::ChainStorageError(e))
            },
        }
    }

    fn publish_block_event(&self, event: BlockEvent) {
        if let Err(event) = self.block_event_sender.send(Arc::new(event)) {
            debug!(target: LOG_TARGET, "No event subscribers. Event {} dropped.", event.0)
        }
    }

    async fn get_target_difficulty(&self, pow_algo: PowAlgorithm) -> Result<Difficulty, CommsInterfaceError> {
        let height_of_longest_chain = async_db::get_metadata(self.blockchain_db.clone())
            .await?
            .height_of_longest_chain
            .ok_or_else(|| CommsInterfaceError::UnexpectedApiResponse)?;
        trace!(
            target: LOG_TARGET,
            "Calculating target difficulty at height:{} for PoW:{}",
            height_of_longest_chain,
            pow_algo
        );
        let constants = self.consensus_manager.consensus_constants();
        let target_difficulties = self.blockchain_db.fetch_target_difficulties(
            pow_algo,
            height_of_longest_chain,
            constants.get_difficulty_block_window() as usize,
        )?;
        let target = get_target_difficulty(
            target_difficulties,
            constants.get_difficulty_block_window() as usize,
            constants.get_diff_target_block_interval(),
            constants.min_pow_difficulty(pow_algo),
            constants.get_difficulty_max_block_interval(),
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
