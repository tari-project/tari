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
    blocks::blockheader::BlockHeader,
    chain_storage::{BlockchainBackend, BlockchainDatabase},
    transaction::TransactionKernel,
};
use std::sync::Arc;

/// The InboundNodeCommsInterface is used to handle all received inbound requests from remote nodes.
pub struct InboundNodeCommsInterface<T>
where T: BlockchainBackend
{
    blockchain_db: Arc<BlockchainDatabase<T>>,
}

impl<T> InboundNodeCommsInterface<T>
where T: BlockchainBackend
{
    /// Construct a new InboundNodeCommsInterface.
    pub fn new(blockchain_db: Arc<BlockchainDatabase<T>>) -> Self {
        Self { blockchain_db }
    }

    /// Handle inbound node comms requests from remote nodes.
    pub async fn handle_request(&self, request: &NodeCommsRequest) -> Result<NodeCommsResponse, CommsInterfaceError> {
        match request {
            // TODO: replace with async calls
            NodeCommsRequest::GetChainMetadata => {
                Ok(NodeCommsResponse::ChainMetadata(self.blockchain_db.get_metadata()?))
            },
            NodeCommsRequest::FetchHeaders(block_nums) => {
                let mut block_headers = Vec::<BlockHeader>::new();
                block_nums.iter().for_each(|block_num| {
                    if let Ok(block_header) = self.blockchain_db.fetch_header(*block_num) {
                        block_headers.push(block_header);
                    }
                });
                Ok(NodeCommsResponse::BlockHeaders(block_headers))
            },
            NodeCommsRequest::FetchKernels(kernel_hashes) => {
                let mut kernels = Vec::<TransactionKernel>::new();
                kernel_hashes.iter().for_each(|hash| {
                    if let Ok(kernel) = self.blockchain_db.fetch_kernel(hash.clone()) {
                        kernels.push(kernel);
                    }
                });
                Ok(NodeCommsResponse::TransactionKernels(kernels))
            },
        }
    }
}
