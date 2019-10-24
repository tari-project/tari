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
    chain_storage::{async_db, BlockchainBackend, BlockchainDatabase},
    transaction::{TransactionKernel, TransactionOutput},
};

/// The InboundNodeCommsInterface is used to handle all received inbound requests from remote nodes.
pub struct InboundNodeCommsInterface<T>
where T: BlockchainBackend
{
    blockchain_db: BlockchainDatabase<T>,
}

impl<T> InboundNodeCommsInterface<T>
where T: BlockchainBackend
{
    /// Construct a new InboundNodeCommsInterface.
    pub fn new(blockchain_db: BlockchainDatabase<T>) -> Self {
        Self { blockchain_db }
    }

    /// Handle inbound node comms requests from remote nodes.
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
        }
    }
}
