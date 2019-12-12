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
    base_node::comms_interface::{error::CommsInterfaceError, NodeCommsRequest, NodeCommsResponse},
    blocks::{blockheader::BlockHeader, Block, BlockBuilder, NewBlockTemplate},
    chain_storage::{
        async_db,
        BlockAddResult,
        BlockchainBackend,
        BlockchainDatabase,
        ChainStorageError,
        HistoricalBlock,
        MmrTree,
    },
    consensus::{ConsensusConstants, ConsensusManager},
    mempool::Mempool,
};
use futures::SinkExt;
use tari_broadcast_channel::Publisher;
use tari_transactions::{
    transaction::{TransactionKernel, TransactionOutput},
    types::HashOutput,
};
use tari_utilities::Hashable;

/// Events that can be published on the Validated Block Event Stream
#[derive(Debug)]
pub enum BlockEvent {
    Verified((Block, BlockAddResult)),
    Invalid((Block, ChainStorageError)),
}

/// The InboundNodeCommsInterface is used to handle all received inbound requests from remote nodes.
pub struct InboundNodeCommsHandlers<T>
where T: BlockchainBackend
{
    event_publisher: Publisher<BlockEvent>,
    blockchain_db: BlockchainDatabase<T>,
    mempool: Mempool<T>,
    consensus_manager: ConsensusManager<T>,
}

impl<T> InboundNodeCommsHandlers<T>
where T: BlockchainBackend
{
    /// Construct a new InboundNodeCommsInterface.
    pub fn new(
        event_publisher: Publisher<BlockEvent>,
        blockchain_db: BlockchainDatabase<T>,
        mempool: Mempool<T>,
        consensus_manager: ConsensusManager<T>,
    ) -> Self
    {
        Self {
            event_publisher,
            blockchain_db,
            mempool,
            consensus_manager,
        }
    }

    /// Handle inbound node comms requests from remote nodes and local services.
    pub async fn handle_request(&self, request: &NodeCommsRequest) -> Result<NodeCommsResponse, CommsInterfaceError> {
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
                    if let Ok(block) = async_db::fetch_block(self.blockchain_db.clone(), *block_num).await {
                        blocks.push(block);
                    }
                }
                Ok(NodeCommsResponse::HistoricalBlocks(blocks))
            },
            NodeCommsRequest::FetchMmrState(mmr_state_request) => Ok(NodeCommsResponse::MmrState(
                async_db::fetch_mmr_base_leaf_nodes(
                    self.blockchain_db.clone(),
                    mmr_state_request.tree.clone(),
                    mmr_state_request.index as usize,
                    mmr_state_request.count as usize,
                )
                .await?,
            )),
            NodeCommsRequest::GetNewBlockTemplate => {
                let metadata = async_db::get_metadata(self.blockchain_db.clone()).await?;
                let best_block_hash = metadata.best_block.ok_or(CommsInterfaceError::UnexpectedApiResponse)?;
                let best_block_header =
                    async_db::fetch_header_with_block_hash(self.blockchain_db.clone(), best_block_hash).await?;
                let header = BlockHeader::from_previous(&best_block_header);

                let transactions = self
                    .mempool
                    .retrieve(ConsensusConstants::current().get_max_block_transaction_weight())
                    .map_err(|e| CommsInterfaceError::MempoolError(e.to_string()))?
                    .iter()
                    .map(|tx| (**tx).clone())
                    .collect();

                let block_template = NewBlockTemplate::from(
                    BlockBuilder::new()
                        .with_header(header)
                        .with_transactions(transactions)
                        .build(),
                );

                Ok(NodeCommsResponse::NewBlockTemplate(block_template))
            },
            NodeCommsRequest::GetNewBlock(block_template) => {
                let NewBlockTemplate { header, body } = block_template.clone();

                let kernel_hashes: Vec<HashOutput> = body.kernels().iter().map(|k| k.hash()).collect();
                let out_hashes: Vec<HashOutput> = body.outputs().iter().map(|out| out.hash()).collect();
                let rp_hashes: Vec<HashOutput> = body.outputs().iter().map(|out| out.proof().hash()).collect();
                let inp_hashes: Vec<HashOutput> = body.inputs().iter().map(|inp| inp.hash()).collect();

                let mut header = BlockHeader::from(header);
                header.kernel_mr = async_db::calculate_mmr_root(
                    self.blockchain_db.clone(),
                    MmrTree::Kernel,
                    kernel_hashes,
                    Vec::new(),
                )
                .await?;
                header.output_mr =
                    async_db::calculate_mmr_root(self.blockchain_db.clone(), MmrTree::Utxo, out_hashes, inp_hashes)
                        .await?;
                header.range_proof_mr = async_db::calculate_mmr_root(
                    self.blockchain_db.clone(),
                    MmrTree::RangeProof,
                    rp_hashes,
                    Vec::new(),
                )
                .await?;

                Ok(NodeCommsResponse::NewBlock(Block { header, body }))
            },
            NodeCommsRequest::GetTargetDifficulty(pow_algo) => Ok(NodeCommsResponse::TargetDifficulty(
                self.consensus_manager.get_target_difficulty(pow_algo)?,
            )),
        }
    }

    /// Handle inbound blocks from remote nodes and local services.
    pub async fn handle_block(&mut self, block: &Block) -> Result<(), CommsInterfaceError> {
        let block_event = match self.blockchain_db.add_block(block.clone()) {
            Ok(block_add_result) => BlockEvent::Verified((block.clone(), block_add_result)),
            Err(e) => BlockEvent::Invalid((block.clone(), e)),
        };
        self.event_publisher
            .send(block_event)
            .await
            .map_err(|_| CommsInterfaceError::EventStreamError)
    }
}
