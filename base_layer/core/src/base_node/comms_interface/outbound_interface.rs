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
    base_node::comms_interface::{
        comms_request::MmrStateRequest,
        error::CommsInterfaceError,
        NodeCommsRequest,
        NodeCommsRequestType,
        NodeCommsResponse,
    },
    blocks::blockheader::BlockHeader,
    chain_storage::{ChainMetadata, HistoricalBlock, MmrTree, MutableMmrState},
};
use tari_service_framework::reply_channel::SenderService;
use tari_transactions::{
    transaction::{TransactionKernel, TransactionOutput},
    types::HashOutput,
};
use tower_service::Service;

/// The OutboundNodeCommsInterface provides an interface to request information from remove nodes.
#[derive(Clone)]
pub struct OutboundNodeCommsInterface {
    sender:
        SenderService<(NodeCommsRequest, NodeCommsRequestType), Result<Vec<NodeCommsResponse>, CommsInterfaceError>>,
}

impl OutboundNodeCommsInterface {
    /// Construct a new OutboundNodeCommsInterface with the specified SenderService.
    pub fn new(
        sender: SenderService<
            (NodeCommsRequest, NodeCommsRequestType),
            Result<Vec<NodeCommsResponse>, CommsInterfaceError>,
        >,
    ) -> Self
    {
        Self { sender }
    }

    /// Request metadata from remote base nodes.
    pub async fn get_metadata(&mut self) -> Result<Vec<ChainMetadata>, CommsInterfaceError> {
        let mut responses = Vec::<ChainMetadata>::new();
        self.sender
            .call((NodeCommsRequest::GetChainMetadata, NodeCommsRequestType::Many))
            .await??
            .into_iter()
            .for_each(|response| {
                if let NodeCommsResponse::ChainMetadata(metadata) = response {
                    responses.push(metadata);
                }
            });
        Ok(responses)
    }

    /// Fetch the transaction kernels with the provided hashes from remote base nodes.
    pub async fn fetch_kernels(
        &mut self,
        hashes: Vec<HashOutput>,
    ) -> Result<Vec<TransactionKernel>, CommsInterfaceError>
    {
        if let Some(NodeCommsResponse::TransactionKernels(kernels)) = self
            .sender
            .call((NodeCommsRequest::FetchKernels(hashes), NodeCommsRequestType::Single))
            .await??
            .first()
        {
            Ok(kernels.clone())
        } else {
            Err(CommsInterfaceError::UnexpectedApiResponse)
        }
    }

    /// Fetch the block headers corresponding to the provided block numbers from remote base nodes.
    pub async fn fetch_headers(&mut self, block_nums: Vec<u64>) -> Result<Vec<BlockHeader>, CommsInterfaceError> {
        if let Some(NodeCommsResponse::BlockHeaders(headers)) = self
            .sender
            .call((NodeCommsRequest::FetchHeaders(block_nums), NodeCommsRequestType::Single))
            .await??
            .first()
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
        if let Some(NodeCommsResponse::TransactionOutputs(utxos)) = self
            .sender
            .call((NodeCommsRequest::FetchUtxos(hashes), NodeCommsRequestType::Single))
            .await??
            .first()
        {
            Ok(utxos.clone())
        } else {
            Err(CommsInterfaceError::UnexpectedApiResponse)
        }
    }

    /// Fetch the Historical Blocks corresponding to the provided block numbers from remote base nodes.
    pub async fn fetch_blocks(&mut self, block_nums: Vec<u64>) -> Result<Vec<HistoricalBlock>, CommsInterfaceError> {
        if let Some(NodeCommsResponse::HistoricalBlocks(blocks)) = self
            .sender
            .call((NodeCommsRequest::FetchBlocks(block_nums), NodeCommsRequestType::Single))
            .await??
            .first()
        {
            Ok(blocks.clone())
        } else {
            Err(CommsInterfaceError::UnexpectedApiResponse)
        }
    }

    /// Fetch the base MMR state of the specified merkle mountain range.
    pub async fn fetch_mmr_state(
        &mut self,
        tree: MmrTree,
        index: u64,
        count: u64,
    ) -> Result<MutableMmrState, CommsInterfaceError>
    {
        if let Some(NodeCommsResponse::MmrState(mmr_state)) = self
            .sender
            .call((
                NodeCommsRequest::FetchMmrState(MmrStateRequest { tree, index, count }),
                NodeCommsRequestType::Single,
            ))
            .await??
            .first()
        {
            Ok(mmr_state.clone())
        } else {
            Err(CommsInterfaceError::UnexpectedApiResponse)
        }
    }
}
