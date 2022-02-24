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

use std::{ops::RangeInclusive, sync::Arc};

use tari_common_types::{
    chain_metadata::ChainMetadata,
    types::{BlockHash, Commitment, HashOutput, PublicKey, Signature},
};
use tari_service_framework::{reply_channel::SenderService, Service};
use tokio::sync::broadcast;

use crate::{
    base_node::comms_interface::{
        comms_request::GetNewBlockTemplateRequest,
        error::CommsInterfaceError,
        BlockEvent,
        NodeCommsRequest,
        NodeCommsResponse,
    },
    blocks::{Block, ChainHeader, HistoricalBlock, NewBlockTemplate},
    chain_storage::UtxoMinedInfo,
    proof_of_work::PowAlgorithm,
    transactions::transaction_components::{TransactionKernel, TransactionOutput},
};

pub type BlockEventSender = broadcast::Sender<Arc<BlockEvent>>;
pub type BlockEventReceiver = broadcast::Receiver<Arc<BlockEvent>>;

/// The InboundNodeCommsInterface provides an interface to request information from the current local node by other
/// internal services.
#[derive(Clone)]
pub struct LocalNodeCommsInterface {
    request_sender: SenderService<NodeCommsRequest, Result<NodeCommsResponse, CommsInterfaceError>>,
    block_sender: SenderService<Block, Result<BlockHash, CommsInterfaceError>>,
    block_event_sender: BlockEventSender,
}

impl LocalNodeCommsInterface {
    /// Construct a new LocalNodeCommsInterface with the specified SenderService.
    pub fn new(
        request_sender: SenderService<NodeCommsRequest, Result<NodeCommsResponse, CommsInterfaceError>>,
        block_sender: SenderService<Block, Result<BlockHash, CommsInterfaceError>>,
        block_event_sender: BlockEventSender,
    ) -> Self {
        Self {
            request_sender,
            block_sender,
            block_event_sender,
        }
    }

    pub fn get_block_event_stream(&self) -> BlockEventReceiver {
        self.block_event_sender.subscribe()
    }

    /// Request metadata from the current local node.
    pub async fn get_metadata(&mut self) -> Result<ChainMetadata, CommsInterfaceError> {
        match self.request_sender.call(NodeCommsRequest::GetChainMetadata).await?? {
            NodeCommsResponse::ChainMetadata(metadata) => Ok(metadata),
            _ => Err(CommsInterfaceError::UnexpectedApiResponse),
        }
    }

    /// Request the block headers within the given range
    pub async fn get_blocks(
        &mut self,
        range: RangeInclusive<u64>,
    ) -> Result<Vec<HistoricalBlock>, CommsInterfaceError> {
        match self
            .request_sender
            .call(NodeCommsRequest::FetchMatchingBlocks(range))
            .await??
        {
            NodeCommsResponse::HistoricalBlocks(blocks) => Ok(blocks),
            _ => Err(CommsInterfaceError::UnexpectedApiResponse),
        }
    }

    /// Request the block header at the given height
    pub async fn get_block(&mut self, height: u64) -> Result<Option<HistoricalBlock>, CommsInterfaceError> {
        match self
            .request_sender
            .call(NodeCommsRequest::FetchMatchingBlocks(height..=height))
            .await??
        {
            NodeCommsResponse::HistoricalBlocks(mut blocks) => Ok(blocks.pop()),
            _ => Err(CommsInterfaceError::UnexpectedApiResponse),
        }
    }

    /// Request the block headers with the given range of heights. The returned headers are ordered from lowest to
    /// highest block height
    pub async fn get_headers(&mut self, range: RangeInclusive<u64>) -> Result<Vec<ChainHeader>, CommsInterfaceError> {
        match self
            .request_sender
            .call(NodeCommsRequest::FetchHeaders(range))
            .await??
        {
            NodeCommsResponse::BlockHeaders(headers) => Ok(headers),
            _ => Err(CommsInterfaceError::UnexpectedApiResponse),
        }
    }

    /// Request the block header with the height.
    pub async fn get_header(&mut self, height: u64) -> Result<Option<ChainHeader>, CommsInterfaceError> {
        match self
            .request_sender
            .call(NodeCommsRequest::FetchHeaders(height..=height))
            .await??
        {
            NodeCommsResponse::BlockHeaders(mut headers) => Ok(headers.pop()),
            _ => Err(CommsInterfaceError::UnexpectedApiResponse),
        }
    }

    /// Request the construction of a new mineable block template from the base node service.
    pub async fn get_new_block_template(
        &mut self,
        pow_algorithm: PowAlgorithm,
        max_weight: u64,
    ) -> Result<NewBlockTemplate, CommsInterfaceError> {
        let request = GetNewBlockTemplateRequest {
            algo: pow_algorithm,
            max_weight,
        };
        match self
            .request_sender
            .call(NodeCommsRequest::GetNewBlockTemplate(request))
            .await??
        {
            NodeCommsResponse::NewBlockTemplate(new_block_template) => Ok(new_block_template),
            _ => Err(CommsInterfaceError::UnexpectedApiResponse),
        }
    }

    /// Request from base node service the construction of a block from a block template.
    pub async fn get_new_block(&mut self, block_template: NewBlockTemplate) -> Result<Block, CommsInterfaceError> {
        match self
            .request_sender
            .call(NodeCommsRequest::GetNewBlock(block_template))
            .await??
        {
            NodeCommsResponse::NewBlock { success, error, block } => {
                if success {
                    if let Some(block) = block {
                        Ok(block)
                    } else {
                        Err(CommsInterfaceError::UnexpectedApiResponse)
                    }
                } else {
                    Err(CommsInterfaceError::ApiError(
                        error.unwrap_or_else(|| "Unspecified error".to_string()),
                    ))
                }
            },
            _ => Err(CommsInterfaceError::UnexpectedApiResponse),
        }
    }

    /// Submit a block to the base node service.
    pub async fn submit_block(&mut self, block: Block) -> Result<BlockHash, CommsInterfaceError> {
        self.block_sender.call(block).await?
    }

    pub fn publish_block_event(&self, event: BlockEvent) -> usize {
        // If event send fails, that means that there are no receivers (i.e. it was sent to zero receivers)
        self.block_event_sender.send(Arc::new(event)).unwrap_or(0)
    }

    pub async fn fetch_matching_utxos(
        &mut self,
        hashes: Vec<HashOutput>,
    ) -> Result<Vec<TransactionOutput>, CommsInterfaceError> {
        match self
            .request_sender
            .call(NodeCommsRequest::FetchMatchingUtxos(hashes))
            .await??
        {
            NodeCommsResponse::TransactionOutputs(outputs) => Ok(outputs),
            _ => Err(CommsInterfaceError::UnexpectedApiResponse),
        }
    }

    /// Fetches the blocks with the specified utxo commitments
    pub async fn fetch_blocks_with_utxos(
        &mut self,
        commitments: Vec<Commitment>,
    ) -> Result<Vec<HistoricalBlock>, CommsInterfaceError> {
        match self
            .request_sender
            .call(NodeCommsRequest::FetchBlocksByUtxos(commitments))
            .await??
        {
            NodeCommsResponse::HistoricalBlocks(blocks) => Ok(blocks),
            _ => Err(CommsInterfaceError::UnexpectedApiResponse),
        }
    }

    /// Fetches the blocks with the specified kernel signatures commitments
    pub async fn get_blocks_with_kernels(
        &mut self,
        kernels: Vec<Signature>,
    ) -> Result<Vec<HistoricalBlock>, CommsInterfaceError> {
        match self
            .request_sender
            .call(NodeCommsRequest::FetchBlocksByKernelExcessSigs(kernels))
            .await??
        {
            NodeCommsResponse::HistoricalBlocks(blocks) => Ok(blocks),
            _ => Err(CommsInterfaceError::UnexpectedApiResponse),
        }
    }

    /// Return header matching the given hash. If the header cannot be found `Ok(None)` is returned.
    pub async fn get_header_by_hash(&mut self, hash: HashOutput) -> Result<Option<ChainHeader>, CommsInterfaceError> {
        match self
            .request_sender
            .call(NodeCommsRequest::GetHeaderByHash(hash))
            .await??
        {
            NodeCommsResponse::BlockHeader(header) => Ok(header),
            _ => Err(CommsInterfaceError::UnexpectedApiResponse),
        }
    }

    /// Return block matching the given hash. If the block cannot be found `Ok(None)` is returned.
    pub async fn get_block_by_hash(
        &mut self,
        hash: HashOutput,
    ) -> Result<Option<HistoricalBlock>, CommsInterfaceError> {
        match self
            .request_sender
            .call(NodeCommsRequest::GetBlockByHash(hash))
            .await??
        {
            NodeCommsResponse::HistoricalBlock(block) => Ok(*block),
            _ => Err(CommsInterfaceError::UnexpectedApiResponse),
        }
    }

    /// Searches for a kernel via the excess sig
    pub async fn get_kernel_by_excess_sig(
        &mut self,
        kernel: Signature,
    ) -> Result<Vec<TransactionKernel>, CommsInterfaceError> {
        match self
            .request_sender
            .call(NodeCommsRequest::FetchKernelByExcessSig(kernel))
            .await??
        {
            NodeCommsResponse::TransactionKernels(kernels) => Ok(kernels),
            _ => Err(CommsInterfaceError::UnexpectedApiResponse),
        }
    }

    pub async fn get_tokens(
        &mut self,
        asset_public_key: PublicKey,
        unique_ids: Vec<Vec<u8>>,
    ) -> Result<Vec<TransactionOutput>, CommsInterfaceError> {
        match self
            .request_sender
            .call(NodeCommsRequest::FetchTokens {
                asset_public_key,
                unique_ids,
            })
            .await??
        {
            NodeCommsResponse::FetchTokensResponse { outputs } => Ok(outputs),
            _ => Err(CommsInterfaceError::UnexpectedApiResponse),
        }
    }

    pub async fn get_asset_registrations(
        &mut self,
        range: RangeInclusive<usize>,
    ) -> Result<Vec<UtxoMinedInfo>, CommsInterfaceError> {
        match self
            .request_sender
            .call(NodeCommsRequest::FetchAssetRegistrations { range })
            .await??
        {
            NodeCommsResponse::FetchAssetRegistrationsResponse { outputs } => Ok(outputs),
            _ => Err(CommsInterfaceError::UnexpectedApiResponse),
        }
    }

    pub async fn get_asset_metadata(
        &mut self,
        asset_public_key: PublicKey,
    ) -> Result<Option<UtxoMinedInfo>, CommsInterfaceError> {
        match self
            .request_sender
            .call(NodeCommsRequest::FetchAssetMetadata { asset_public_key })
            .await??
        {
            NodeCommsResponse::FetchAssetMetadataResponse { output } => Ok(*output),
            _ => Err(CommsInterfaceError::UnexpectedApiResponse),
        }
    }
}
