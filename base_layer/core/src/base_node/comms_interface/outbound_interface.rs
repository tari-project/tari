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

use tari_common_types::types::{BlockHash, PrivateKey};
use tari_comms::peer_manager::NodeId;
use tari_service_framework::{reply_channel::SenderService, Service};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    base_node::comms_interface::{
        error::CommsInterfaceError,
        FetchMempoolTransactionsResponse,
        NodeCommsRequest,
        NodeCommsResponse,
    },
    blocks::{HistoricalBlock, NewBlock},
};

/// The OutboundNodeCommsInterface provides an interface to request information from remove nodes.
#[derive(Clone)]
pub struct OutboundNodeCommsInterface {
    request_sender: SenderService<(NodeCommsRequest, Option<NodeId>), Result<NodeCommsResponse, CommsInterfaceError>>,
    block_sender: UnboundedSender<(NewBlock, Vec<NodeId>)>,
}

impl OutboundNodeCommsInterface {
    /// Construct a new OutboundNodeCommsInterface with the specified SenderService.
    pub fn new(
        request_sender: SenderService<
            (NodeCommsRequest, Option<NodeId>),
            Result<NodeCommsResponse, CommsInterfaceError>,
        >,
        block_sender: UnboundedSender<(NewBlock, Vec<NodeId>)>,
    ) -> Self {
        Self {
            request_sender,
            block_sender,
        }
    }

    /// Fetch the Blocks corresponding to the provided block hashes from a specific base node. The requested blocks
    /// could be chain blocks or orphan blocks.
    pub async fn request_blocks_by_hashes_from_peer(
        &mut self,
        block_hashes: Vec<BlockHash>,
        node_id: Option<NodeId>,
    ) -> Result<Vec<HistoricalBlock>, CommsInterfaceError> {
        if let NodeCommsResponse::HistoricalBlocks(blocks) = self
            .request_sender
            .call((NodeCommsRequest::FetchBlocksByHash(block_hashes), node_id))
            .await??
        {
            Ok(blocks)
        } else {
            Err(CommsInterfaceError::UnexpectedApiResponse)
        }
    }

    /// Fetch the transactions corresponding to the provided excess_sigs from the given peer `NodeId`.
    pub async fn request_transactions_by_excess_sig(
        &mut self,
        node_id: NodeId,
        excess_sigs: Vec<PrivateKey>,
    ) -> Result<FetchMempoolTransactionsResponse, CommsInterfaceError> {
        if let NodeCommsResponse::FetchMempoolTransactionsByExcessSigsResponse(resp) = self
            .request_sender
            .call((
                NodeCommsRequest::FetchMempoolTransactionsByExcessSigs { excess_sigs },
                Some(node_id),
            ))
            .await??
        {
            Ok(resp)
        } else {
            Err(CommsInterfaceError::UnexpectedApiResponse)
        }
    }

    /// Transmit a block to remote base nodes, excluding the provided peers.
    pub async fn propagate_block(
        &self,
        new_block: NewBlock,
        exclude_peers: Vec<NodeId>,
    ) -> Result<(), CommsInterfaceError> {
        self.block_sender.send((new_block, exclude_peers)).map_err(|err| {
            CommsInterfaceError::InternalChannelError(format!("Failed to send on block_sender: {}", err))
        })
    }
}
