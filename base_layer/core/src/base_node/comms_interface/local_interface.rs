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
    base_node::comms_interface::{error::CommsInterfaceError, BlockEvent, NodeCommsRequest, NodeCommsResponse},
    blocks::{Block, BlockHeader, NewBlockTemplate},
    chain_storage::{ChainMetadata, HistoricalBlock},
    proof_of_work::{Difficulty, PowAlgorithm},
};
use futures::{stream::Fuse, StreamExt};
use tari_broadcast_channel::Subscriber;
use tari_service_framework::reply_channel::SenderService;
use tower_service::Service;

/// The InboundNodeCommsInterface provides an interface to request information from the current local node by other
/// internal services.
#[derive(Clone)]
pub struct LocalNodeCommsInterface {
    request_sender: SenderService<NodeCommsRequest, Result<NodeCommsResponse, CommsInterfaceError>>,
    block_sender: SenderService<Block, Result<(), CommsInterfaceError>>,
    block_event_stream: Subscriber<BlockEvent>,
}

impl LocalNodeCommsInterface {
    /// Construct a new LocalNodeCommsInterface with the specified SenderService.
    pub fn new(
        request_sender: SenderService<NodeCommsRequest, Result<NodeCommsResponse, CommsInterfaceError>>,
        block_sender: SenderService<Block, Result<(), CommsInterfaceError>>,
        block_event_stream: Subscriber<BlockEvent>,
    ) -> Self
    {
        Self {
            request_sender,
            block_sender,
            block_event_stream,
        }
    }

    pub fn get_block_event_stream(&self) -> Subscriber<BlockEvent> {
        self.block_event_stream.clone()
    }

    pub fn get_block_event_stream_fused(&self) -> Fuse<Subscriber<BlockEvent>> {
        self.get_block_event_stream().fuse()
    }

    /// Request metadata from the current local node.
    pub async fn get_metadata(&mut self) -> Result<ChainMetadata, CommsInterfaceError> {
        match self.request_sender.call(NodeCommsRequest::GetChainMetadata).await?? {
            NodeCommsResponse::ChainMetadata(metadata) => Ok(metadata),
            _ => Err(CommsInterfaceError::UnexpectedApiResponse),
        }
    }

    /// Request the block header of the current tip at the block height
    pub async fn get_blocks(&mut self, block_heights: Vec<u64>) -> Result<Vec<HistoricalBlock>, CommsInterfaceError> {
        match self
            .request_sender
            .call(NodeCommsRequest::FetchBlocks(block_heights))
            .await??
        {
            NodeCommsResponse::HistoricalBlocks(blocks) => Ok(blocks),
            _ => Err(CommsInterfaceError::UnexpectedApiResponse),
        }
    }

    /// Request the block header of the current tip at the block height
    pub async fn get_headers(&mut self, block_heights: Vec<u64>) -> Result<Vec<BlockHeader>, CommsInterfaceError> {
        match self
            .request_sender
            .call(NodeCommsRequest::FetchHeaders(block_heights))
            .await??
        {
            NodeCommsResponse::BlockHeaders(headers) => Ok(headers),
            _ => Err(CommsInterfaceError::UnexpectedApiResponse),
        }
    }

    /// Request the construction of a new mineable block template from the base node service.
    pub async fn get_new_block_template(
        &mut self,
        pow_algorithm: PowAlgorithm,
    ) -> Result<NewBlockTemplate, CommsInterfaceError>
    {
        match self
            .request_sender
            .call(NodeCommsRequest::GetNewBlockTemplate(pow_algorithm))
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
            NodeCommsResponse::NewBlock(block) => Ok(block),
            _ => Err(CommsInterfaceError::UnexpectedApiResponse),
        }
    }

    /// Request the PoW target difficulty for mining on the main chain from the base node service.
    pub async fn get_target_difficulty(
        &mut self,
        pow_algorithm: PowAlgorithm,
    ) -> Result<Difficulty, CommsInterfaceError>
    {
        match self
            .request_sender
            .call(NodeCommsRequest::GetTargetDifficulty(pow_algorithm))
            .await??
        {
            NodeCommsResponse::TargetDifficulty(difficulty) => Ok(difficulty),
            _ => Err(CommsInterfaceError::UnexpectedApiResponse),
        }
    }

    /// Submit a block to the base node service.
    pub async fn submit_block(&mut self, block: Block) -> Result<(), CommsInterfaceError> {
        self.block_sender.call(block).await?
    }
}
