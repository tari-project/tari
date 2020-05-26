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
    blocks::{blockheader::BlockHeader, Block, NewBlockTemplate},
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
use log::*;
use std::{
    fmt::{Display, Error, Formatter},
    sync::Arc,
};
use strum_macros::Display;
use tari_comms::peer_manager::NodeId;
use tari_crypto::tari_utilities::{hash::Hashable, hex::Hex};

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
pub struct InboundNodeCommsHandlers<T>
where T: BlockchainBackend + 'static
{
    block_event_sender: BlockEventSender,
    blockchain_db: BlockchainDatabase<T>,
    mempool: Mempool<T>,
    consensus_manager: ConsensusManager,
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
            outbound_nci,
        }
    }

    /// Handle inbound node comms requests from remote nodes and local services.
    pub async fn handle_request(&self, request: &NodeCommsRequest) -> Result<NodeCommsResponse, CommsInterfaceError> {
        debug!(target: LOG_TARGET, "Handling remote request: {}", request);
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
                        .get_max_block_transaction_weight(),
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
        }
    }

    /// Handle inbound blocks from remote nodes and local services.
    pub async fn handle_block(
        &mut self,
        block_context: &(Block, Broadcast),
        source_peer: Option<NodeId>,
    ) -> Result<(), CommsInterfaceError>
    {
        let (block, broadcast) = block_context;
        debug!(
            target: LOG_TARGET,
            "Block #{} ({}) received from {}",
            block.header.height,
            block.hash().to_hex(),
            source_peer
                .as_ref()
                .map(|p| format!("remote peer: {}", p))
                .unwrap_or_else(|| "local services".to_string())
        );
        trace!(target: LOG_TARGET, "Block: {}", block);
        let add_block_result = async_db::add_block(self.blockchain_db.clone(), block.clone()).await;
        // Create block event on block event stream
        let mut result = Ok(());
        let block_event = match add_block_result.clone() {
            Ok(block_add_result) => {
                trace!(target: LOG_TARGET, "Block event created: {}", block_add_result);
                BlockEvent::Verified((Box::new(block.clone()), block_add_result, *broadcast))
            },
            Err(e) => {
                warn!(target: LOG_TARGET, "Block validation failed: {:?}", e);
                result = Err(CommsInterfaceError::ChainStorageError(e.clone()));
                BlockEvent::Invalid((Box::new(block.clone()), e, *broadcast))
            },
        };
        self.block_event_sender
            .send(Arc::new(block_event))
            .map_err(|_| CommsInterfaceError::EventStreamError)?;

        // Propagate verified block to remote nodes
        if let Ok(add_block_result) = add_block_result {
            let propagate = match add_block_result {
                BlockAddResult::Ok => true,
                BlockAddResult::BlockExists => false,
                BlockAddResult::OrphanBlock => false,
                BlockAddResult::ChainReorg(_) => true,
            };
            if propagate && bool::from(*broadcast) {
                info!(
                    target: LOG_TARGET,
                    "Propagate block ({}) to network.",
                    block.hash().to_hex()
                );
                let exclude_peers = source_peer.into_iter().collect();
                self.outbound_nci.propagate_block(block.clone(), exclude_peers).await?;
            }
        }
        result
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

impl<T> Clone for InboundNodeCommsHandlers<T>
where T: BlockchainBackend + 'static
{
    fn clone(&self) -> Self {
        // All members use Arc's internally so calling clone should be cheap.
        Self {
            block_event_sender: self.block_event_sender.clone(),
            blockchain_db: self.blockchain_db.clone(),
            mempool: self.mempool.clone(),
            consensus_manager: self.consensus_manager.clone(),
            outbound_nci: self.outbound_nci.clone(),
        }
    }
}
