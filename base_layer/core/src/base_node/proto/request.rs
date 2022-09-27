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

use std::convert::{From, TryFrom, TryInto};

use tari_common_types::types::PrivateKey;
use tari_utilities::ByteArray;

use crate::{
    base_node::comms_interface::NodeCommsRequest,
    proto::{base_node as proto, base_node::base_node_service_request::Request as ProtoNodeCommsRequest},
};

//---------------------------------- BaseNodeRequest --------------------------------------------//
impl TryInto<NodeCommsRequest> for ProtoNodeCommsRequest {
    type Error = String;

    fn try_into(self) -> Result<NodeCommsRequest, Self::Error> {
        use ProtoNodeCommsRequest::{FetchBlocksByHash, FetchMempoolTransactionsByExcessSigs};
        let request = match self {
            FetchBlocksByHash(req) => {
                let block_hashes = req
                    .block_hashes
                    .into_iter()
                    .map(|hash| hash.try_into().map_err(|_| "Malformed hash".to_string()))
                    .collect::<Result<_, _>>()?;
                NodeCommsRequest::FetchBlocksByHash {
                    block_hashes,
                    compact: req.compact,
                }
            },
            FetchMempoolTransactionsByExcessSigs(excess_sigs) => {
                let excess_sigs = excess_sigs
                    .excess_sigs
                    .into_iter()
                    .map(|bytes| PrivateKey::from_bytes(&bytes).map_err(|_| "Malformed excess sig".to_string()))
                    .collect::<Result<_, _>>()?;

                NodeCommsRequest::FetchMempoolTransactionsByExcessSigs { excess_sigs }
            },
        };
        Ok(request)
    }
}

impl TryFrom<NodeCommsRequest> for ProtoNodeCommsRequest {
    type Error = String;

    fn try_from(request: NodeCommsRequest) -> Result<Self, Self::Error> {
        use NodeCommsRequest::{FetchBlocksByHash, FetchMempoolTransactionsByExcessSigs};
        match request {
            FetchBlocksByHash { block_hashes, compact } => Ok(ProtoNodeCommsRequest::FetchBlocksByHash(
                proto::FetchBlocksByHashRequest {
                    block_hashes: block_hashes.into_iter().map(|hash| hash.to_vec()).collect(),
                    compact,
                },
            )),
            FetchMempoolTransactionsByExcessSigs { excess_sigs } => Ok(
                ProtoNodeCommsRequest::FetchMempoolTransactionsByExcessSigs(proto::ExcessSigs {
                    excess_sigs: excess_sigs.into_iter().map(|sig| sig.to_vec()).collect(),
                }),
            ),
            e => Err(format!("{} request is not supported", e)),
        }
    }
}

//---------------------------------- Wrappers --------------------------------------------//

impl From<Vec<u64>> for proto::BlockHeights {
    fn from(heights: Vec<u64>) -> Self {
        Self { heights }
    }
}
