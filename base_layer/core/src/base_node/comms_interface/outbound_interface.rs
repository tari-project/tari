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
    blocks::{blockheader::BlockHeader, Block},
    chain_storage::{ChainMetadata, HistoricalBlock},
    transactions::{
        transaction::{TransactionKernel, TransactionOutput},
        types::HashOutput,
    },
};
use futures::channel::mpsc::UnboundedSender;
use log::*;
use tari_comms::{peer_manager::NodeId, types::CommsPublicKey};
use tari_service_framework::reply_channel::SenderService;
use tower_service::Service;

pub const LOG_TARGET: &str = "c::bn::comms_interface::outbound_interface";

/// The OutboundNodeCommsInterface provides an interface to request information from remove nodes.
#[derive(Clone)]
pub struct OutboundNodeCommsInterface {
    request_sender: SenderService<(NodeCommsRequest, Option<NodeId>), Result<NodeCommsResponse, CommsInterfaceError>>,
    block_sender: UnboundedSender<(Block, Vec<CommsPublicKey>)>,
}

impl OutboundNodeCommsInterface {
    /// Construct a new OutboundNodeCommsInterface with the specified SenderService.
    pub fn new(
        request_sender: SenderService<
            (NodeCommsRequest, Option<NodeId>),
            Result<NodeCommsResponse, CommsInterfaceError>,
        >,
        block_sender: UnboundedSender<(Block, Vec<CommsPublicKey>)>,
    ) -> Self
    {
        Self {
            request_sender,
            block_sender,
        }
    }

    /// Request metadata from remote base nodes.
    pub async fn get_metadata(&mut self) -> Result<ChainMetadata, CommsInterfaceError> {
        self.request_metadata_from_peer(None).await
    }

    /// Request metadata from a specific base node, if None is provided as a node_id then a random base node will be
    /// queried.
    pub async fn request_metadata_from_peer(
        &mut self,
        node_id: Option<NodeId>,
    ) -> Result<ChainMetadata, CommsInterfaceError>
    {
        if let NodeCommsResponse::ChainMetadata(metadata) = self
            .request_sender
            .call((NodeCommsRequest::GetChainMetadata, node_id))
            .await??
        {
            trace!(target: LOG_TARGET, "Remote metadata requested: {:?}", metadata,);
            Ok(metadata)
        } else {
            // TODO: Potentially ban peer
            Err(CommsInterfaceError::UnexpectedApiResponse)
        }
    }

    /// Fetch the transaction kernels with the provided hashes from remote base nodes.
    pub async fn fetch_kernels(
        &mut self,
        hashes: Vec<HashOutput>,
    ) -> Result<Vec<TransactionKernel>, CommsInterfaceError>
    {
        self.request_kernels_from_peer(hashes, None).await
    }

    /// Fetch the transaction kernels with the provided hashes from a specific base node, if None is provided as a
    /// node_id then a random base node will be queried.
    pub async fn request_kernels_from_peer(
        &mut self,
        hashes: Vec<HashOutput>,
        node_id: Option<NodeId>,
    ) -> Result<Vec<TransactionKernel>, CommsInterfaceError>
    {
        if let NodeCommsResponse::TransactionKernels(kernels) = self
            .request_sender
            .call((NodeCommsRequest::FetchKernels(hashes), node_id))
            .await??
        {
            Ok(kernels)
        } else {
            Err(CommsInterfaceError::UnexpectedApiResponse)
        }
    }

    /// Fetch the block headers corresponding to the provided block numbers from remote base nodes.
    pub async fn fetch_headers(&mut self, block_nums: Vec<u64>) -> Result<Vec<BlockHeader>, CommsInterfaceError> {
        self.request_headers_from_peer(block_nums, None).await
    }

    /// Fetch the block headers corresponding to the provided block numbers from a specific base node, if None is
    /// provided as a node_id then a random base node will be queried.
    pub async fn request_headers_from_peer(
        &mut self,
        block_nums: Vec<u64>,
        node_id: Option<NodeId>,
    ) -> Result<Vec<BlockHeader>, CommsInterfaceError>
    {
        if let NodeCommsResponse::BlockHeaders(headers) = self
            .request_sender
            .call((NodeCommsRequest::FetchHeaders(block_nums), node_id))
            .await??
        {
            Ok(headers)
        } else {
            Err(CommsInterfaceError::UnexpectedApiResponse)
        }
    }

    /// Fetch the Headers corresponding to the provided block hashes from remote base nodes.
    pub async fn fetch_headers_with_hashes(
        &mut self,
        block_hashes: Vec<HashOutput>,
    ) -> Result<Vec<BlockHeader>, CommsInterfaceError>
    {
        self.request_headers_with_hashes_from_peer(block_hashes, None).await
    }

    /// Fetch the Headers corresponding to the provided block hashes from a specific base node, if None is provided as a
    /// node_id then a random base node will be queried.
    pub async fn request_headers_with_hashes_from_peer(
        &mut self,
        block_hashes: Vec<HashOutput>,
        node_id: Option<NodeId>,
    ) -> Result<Vec<BlockHeader>, CommsInterfaceError>
    {
        if let NodeCommsResponse::BlockHeaders(headers) = self
            .request_sender
            .call((NodeCommsRequest::FetchHeadersWithHashes(block_hashes), node_id))
            .await??
        {
            Ok(headers)
        } else {
            Err(CommsInterfaceError::UnexpectedApiResponse)
        }
    }

    /// Fetch the Headers corresponding to the provided block hashes from remote base nodes.
    pub async fn fetch_headers_between(
        &mut self,
        from_hash: Vec<HashOutput>,
        to_hash: Option<HashOutput>,
        node_id: Option<NodeId>,
    ) -> Result<Vec<BlockHeader>, CommsInterfaceError>
    {
        let to_hash = to_hash.unwrap_or(HashOutput::new());
        if let NodeCommsResponse::FetchHeadersAfterResponse(headers) = self
            .request_sender
            .call((NodeCommsRequest::FetchHeadersAfter(from_hash, to_hash), node_id))
            .await??
        {
            Ok(headers.clone())
        } else {
            Err(CommsInterfaceError::UnexpectedApiResponse)
        }
    }

    /// Fetch the UTXOs with the provided hashes from remote base nodes.
    pub async fn fetch_utxos(
        &mut self,
        hashes: Vec<HashOutput>,
    ) -> Result<Vec<TransactionOutput>, CommsInterfaceError>
    {
        self.request_utxos_from_peer(hashes, None).await
    }

    /// Fetch the UTXOs with the provided hashes from a specific base node, if None is provided as a node_id then a
    /// random base node will be queried.
    pub async fn request_utxos_from_peer(
        &mut self,
        hashes: Vec<HashOutput>,
        node_id: Option<NodeId>,
    ) -> Result<Vec<TransactionOutput>, CommsInterfaceError>
    {
        if let NodeCommsResponse::TransactionOutputs(utxos) = self
            .request_sender
            .call((NodeCommsRequest::FetchUtxos(hashes), node_id))
            .await??
        {
            Ok(utxos)
        } else {
            Err(CommsInterfaceError::UnexpectedApiResponse)
        }
    }

    /// Fetch the Historical Blocks corresponding to the provided block numbers from remote base nodes.
    pub async fn fetch_blocks(&mut self, block_nums: Vec<u64>) -> Result<Vec<HistoricalBlock>, CommsInterfaceError> {
        self.request_blocks_from_peer(block_nums, None).await
    }

    /// Fetch the Historical Blocks corresponding to the provided block numbers from a specific base node, if None is
    /// provided as a node_id then a random base node will be queried.
    pub async fn request_blocks_from_peer(
        &mut self,
        block_nums: Vec<u64>,
        node_id: Option<NodeId>,
    ) -> Result<Vec<HistoricalBlock>, CommsInterfaceError>
    {
        if let NodeCommsResponse::HistoricalBlocks(blocks) = self
            .request_sender
            .call((NodeCommsRequest::FetchBlocks(block_nums), node_id))
            .await??
        {
            Ok(blocks)
        } else {
            Err(CommsInterfaceError::UnexpectedApiResponse)
        }
    }

    /// Fetch the Blocks corresponding to the provided block hashes from remote base nodes. The requested blocks could
    /// be chain blocks or orphan blocks.
    pub async fn fetch_blocks_with_hashes(
        &mut self,
        block_hashes: Vec<HashOutput>,
    ) -> Result<Vec<HistoricalBlock>, CommsInterfaceError>
    {
        self.request_blocks_with_hashes_from_peer(block_hashes, None).await
    }

    /// Fetch the Blocks corresponding to the provided block hashes from a specific base node. The requested blocks
    /// could be chain blocks or orphan blocks.
    pub async fn request_blocks_with_hashes_from_peer(
        &mut self,
        block_hashes: Vec<HashOutput>,
        node_id: Option<NodeId>,
    ) -> Result<Vec<HistoricalBlock>, CommsInterfaceError>
    {
        if let NodeCommsResponse::HistoricalBlocks(blocks) = self
            .request_sender
            .call((NodeCommsRequest::FetchBlocksWithHashes(block_hashes), node_id))
            .await??
        {
            Ok(blocks)
        } else {
            Err(CommsInterfaceError::UnexpectedApiResponse)
        }
    }

    /// Transmit a block to remote base nodes, excluding the provided peers.
    pub async fn propagate_block(
        &mut self,
        block: Block,
        exclude_peers: Vec<CommsPublicKey>,
    ) -> Result<(), CommsInterfaceError>
    {
        self.block_sender
            .unbounded_send((block, exclude_peers))
            .map_err(|_| CommsInterfaceError::BroadcastFailed)
    }
}
