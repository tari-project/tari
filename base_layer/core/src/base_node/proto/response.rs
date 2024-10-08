// Copyright 2019, The Tari Project
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

use std::{
    convert::{TryFrom, TryInto},
    iter::FromIterator,
    sync::Arc,
};

use tari_common_types::types::PrivateKey;
pub use tari_p2p::{proto, proto::base_node::base_node_service_response::Response as ProtoNodeCommsResponse};
use tari_utilities::{convert::try_convert_all, ByteArray};

use crate::{
    base_node::comms_interface::{FetchMempoolTransactionsResponse, NodeCommsResponse},
    blocks::{Block, BlockHeader, HistoricalBlock},
};

impl TryInto<NodeCommsResponse> for ProtoNodeCommsResponse {
    type Error = String;

    fn try_into(self) -> Result<NodeCommsResponse, Self::Error> {
        use ProtoNodeCommsResponse::{BlockResponse, FetchMempoolTransactionsByExcessSigsResponse, HistoricalBlocks};
        let response = match self {
            BlockResponse(block) => NodeCommsResponse::Block(Box::new(block.try_into()?)),
            HistoricalBlocks(blocks) => {
                let blocks = try_convert_all(blocks.blocks)?;
                NodeCommsResponse::HistoricalBlocks(blocks)
            },
            FetchMempoolTransactionsByExcessSigsResponse(response) => {
                let transactions = response
                    .transactions
                    .into_iter()
                    .map(|tx| tx.try_into().map(Arc::new))
                    .collect::<Result<_, _>>()?;
                let not_found = response
                    .not_found
                    .into_iter()
                    .map(|bytes| {
                        PrivateKey::from_canonical_bytes(&bytes).map_err(|_| "Malformed excess signature".to_string())
                    })
                    .collect::<Result<_, _>>()?;
                NodeCommsResponse::FetchMempoolTransactionsByExcessSigsResponse(
                    self::FetchMempoolTransactionsResponse {
                        transactions,
                        not_found,
                    },
                )
            },
        };

        Ok(response)
    }
}

impl TryFrom<NodeCommsResponse> for ProtoNodeCommsResponse {
    type Error = String;

    fn try_from(response: NodeCommsResponse) -> Result<Self, Self::Error> {
        use NodeCommsResponse::{FetchMempoolTransactionsByExcessSigsResponse, HistoricalBlocks};
        match response {
            NodeCommsResponse::Block(block) => Ok(ProtoNodeCommsResponse::BlockResponse((*block).try_into()?)),
            HistoricalBlocks(historical_blocks) => {
                let historical_blocks = historical_blocks
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<proto::core::HistoricalBlock>, _>>()?
                    .into_iter()
                    .map(Into::into)
                    .collect();
                Ok(ProtoNodeCommsResponse::HistoricalBlocks(historical_blocks))
            },
            FetchMempoolTransactionsByExcessSigsResponse(resp) => {
                let transactions = resp
                    .transactions
                    .into_iter()
                    .map(|tx| tx.try_into())
                    .collect::<Result<_, _>>()?;
                Ok(ProtoNodeCommsResponse::FetchMempoolTransactionsByExcessSigsResponse(
                    proto::base_node::FetchMempoolTransactionsResponse {
                        transactions,
                        not_found: resp.not_found.into_iter().map(|s| s.to_vec()).collect(),
                    },
                ))
            },
            // This would only occur if a programming error sent out the unsupported response
            resp => Err(format!("Response not supported {:?}", resp)),
        }
    }
}
