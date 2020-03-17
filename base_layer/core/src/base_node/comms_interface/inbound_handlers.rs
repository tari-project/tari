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
        comms_interface::{error::CommsInterfaceError, NodeCommsRequest, NodeCommsResponse},
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
    transactions::transaction::{TransactionKernel, TransactionOutput},
};
use futures::SinkExt;
use log::*;
use std::sync::Arc;
use strum_macros::Display;
use tari_broadcast_channel::Publisher;
use tari_comms::types::CommsPublicKey;
use tari_crypto::tari_utilities::hex::Hex;
use tokio::sync::RwLock;

const LOG_TARGET: &str = "c::bn::comms_interface::inbound_handler";

/// Events that can be published on the Validated Block Event Stream
#[derive(Debug, Clone, Display)]
pub enum BlockEvent {
    Verified((Box<Block>, BlockAddResult)),
    Invalid((Box<Block>, ChainStorageError)),
}

/// The InboundNodeCommsInterface is used to handle all received inbound requests from remote nodes.
pub struct InboundNodeCommsHandlers<T>
where T: BlockchainBackend + 'static
{
    event_publisher: Arc<RwLock<Publisher<BlockEvent>>>,
    blockchain_db: BlockchainDatabase<T>,
    mempool: Mempool<T>,
    consensus_manager: ConsensusManager<T>,
    outbound_nci: OutboundNodeCommsInterface,
}

impl<T> InboundNodeCommsHandlers<T>
where T: BlockchainBackend + 'static
{
    /// Construct a new InboundNodeCommsInterface.
    pub fn new(
        event_publisher: Publisher<BlockEvent>,
        blockchain_db: BlockchainDatabase<T>,
        mempool: Mempool<T>,
        consensus_manager: ConsensusManager<T>,
        outbound_nci: OutboundNodeCommsInterface,
    ) -> Self
    {
        Self {
            event_publisher: Arc::new(RwLock::new(event_publisher)),
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
                        Err(e) => info!(
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
                        Ok(None) => info!(
                            target: LOG_TARGET,
                            "Could not provide requested block {} to peer because not stored",
                            block_hash.to_hex(),
                        ),
                        Err(e) => info!(
                            target: LOG_TARGET,
                            "Could not provide requested block {} to peer because: {}",
                            block_hash.to_hex(),
                            e.to_string()
                        ),
                    }
                }
                Ok(NodeCommsResponse::HistoricalBlocks(blocks))
            },
            NodeCommsRequest::GetNewBlockTemplate => {
                let metadata = async_db::get_metadata(self.blockchain_db.clone()).await?;
                let best_block_hash = metadata
                    .best_block
                    .ok_or_else(|| CommsInterfaceError::UnexpectedApiResponse)?;
                let best_block_header =
                    async_db::fetch_header_with_block_hash(self.blockchain_db.clone(), best_block_hash).await?;
                let mut header = BlockHeader::from_previous(&best_block_header);
                header.version = self.consensus_manager.consensus_constants().blockchain_version();

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
                trace!(target: LOG_TARGET, "New block template requested {}", block_template);
                Ok(NodeCommsResponse::NewBlockTemplate(block_template))
            },
            NodeCommsRequest::GetNewBlock(block_template) => {
                let block = async_db::calculate_mmr_roots(self.blockchain_db.clone(), block_template.clone()).await?;
                Ok(NodeCommsResponse::NewBlock(block))
            },
            NodeCommsRequest::GetTargetDifficulty(pow_algo) => Ok(NodeCommsResponse::TargetDifficulty(
                self.consensus_manager.get_target_difficulty(*pow_algo)?,
            )),
        }
    }

    /// Handle inbound blocks from remote nodes and local services.
    pub async fn handle_block(
        &mut self,
        block: &Block,
        source_peer: Option<CommsPublicKey>,
    ) -> Result<(), CommsInterfaceError>
    {
        debug!(
            target: LOG_TARGET,
            "Block received from {}",
            source_peer
                .as_ref()
                .map(|p| format!("remote peer: {}", p))
                .unwrap_or_else(|| "local services".to_string())
        );
        trace!(target: LOG_TARGET, "Block: {}", block);
        let add_block_result = async_db::add_block(self.blockchain_db.clone(), block.clone()).await;
        // Create block event on block event stream
        let block_event = match add_block_result.clone() {
            Ok(block_add_result) => {
                debug!(target: LOG_TARGET, "Block event created: {:?}", block_add_result);
                BlockEvent::Verified((Box::new(block.clone()), block_add_result))
            },
            Err(e) => {
                error!(target: LOG_TARGET, "Block validation failed: {:?}", e);
                BlockEvent::Invalid((Box::new(block.clone()), e))
            },
        };
        self.event_publisher
            .write()
            .await
            .send(block_event)
            .await
            .map_err(|_| CommsInterfaceError::EventStreamError)?;
        // Propagate verified block to remote nodes
        if let Ok(add_block_result) = add_block_result {
            let propagate = match add_block_result {
                BlockAddResult::Ok => true,
                BlockAddResult::BlockExists => false,
                BlockAddResult::OrphanBlock => false,
                BlockAddResult::ChainReorg(_) => true,
            };
            if propagate {
                let exclude_peers = source_peer.into_iter().collect();
                self.outbound_nci.propagate_block(block.clone(), exclude_peers).await?;
            }
        }
        Ok(())
    }
}

impl<T> Clone for InboundNodeCommsHandlers<T>
where T: BlockchainBackend + 'static
{
    fn clone(&self) -> Self {
        // All members use Arc's internally so calling clone should be cheap.
        Self {
            event_publisher: self.event_publisher.clone(),
            blockchain_db: self.blockchain_db.clone(),
            mempool: self.mempool.clone(),
            consensus_manager: self.consensus_manager.clone(),
            outbound_nci: self.outbound_nci.clone(),
        }
    }
}
