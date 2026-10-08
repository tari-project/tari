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

use std::convert::{TryFrom, TryInto};

use tari_common_types::types::PrivateKey;
use tari_utilities::ByteArray;

use crate::{
    base_node::comms_interface::NodeCommsRequest,
    proto::{base_node as proto, base_node::base_node_service_request::Request as ProtoNodeCommsRequest},
};

/// The most kernel excess signatures a `FetchMempoolTransactionsByExcessSigs` request may carry. A node asks for the
/// excess signatures of a new block's kernels that it is missing from its mempool, so a request never needs more than a
/// block's kernels: about 9,000 on mainnet and 12,780 on the 127,795-weight networks (checked for every network in
/// `proto::decode_budget_tests::per_network`). Without a cap the decode budget alone allows ~246k signatures in an
/// 8 MiB frame, each parsed and looked up, with a not-found list of the same size echoed back.
pub(crate) const MAX_EXCESS_SIGS_PER_REQUEST: usize = 16_384;

//---------------------------------- BaseNodeRequest --------------------------------------------//
impl TryInto<NodeCommsRequest> for ProtoNodeCommsRequest {
    type Error = String;

    fn try_into(self) -> Result<NodeCommsRequest, Self::Error> {
        use ProtoNodeCommsRequest::{FetchMempoolTransactionsByExcessSigs, GetBlockFromAllChains};
        let request = match self {
            GetBlockFromAllChains(req) => {
                NodeCommsRequest::GetBlockFromAllChains(req.hash.try_into().map_err(|_| "Malformed hash".to_string())?)
            },
            FetchMempoolTransactionsByExcessSigs(excess_sigs) => {
                if excess_sigs.excess_sigs.len() > MAX_EXCESS_SIGS_PER_REQUEST {
                    return Err(format!(
                        "Too many excess sigs: {}, at most {MAX_EXCESS_SIGS_PER_REQUEST} allowed",
                        excess_sigs.excess_sigs.len()
                    ));
                }
                let excess_sigs = excess_sigs
                    .excess_sigs
                    .into_iter()
                    .map(|bytes| {
                        PrivateKey::from_canonical_bytes(&bytes).map_err(|_| "Malformed excess sig".to_string())
                    })
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
        use NodeCommsRequest::{FetchMempoolTransactionsByExcessSigs, GetBlockFromAllChains};
        match request {
            GetBlockFromAllChains(hash) => Ok(ProtoNodeCommsRequest::GetBlockFromAllChains(
                proto::GetBlockFromAllChainsRequest { hash: hash.to_vec() },
            )),
            FetchMempoolTransactionsByExcessSigs { excess_sigs } => Ok(
                ProtoNodeCommsRequest::FetchMempoolTransactionsByExcessSigs(proto::ExcessSigs {
                    excess_sigs: excess_sigs.into_iter().map(|sig| sig.to_vec()).collect(),
                }),
            ),
            e => Err(format!("{e} request is not supported")),
        }
    }
}

//---------------------------------- Wrappers --------------------------------------------//

impl From<Vec<u64>> for proto::BlockHeights {
    fn from(heights: Vec<u64>) -> Self {
        Self { heights }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn request(count: usize) -> ProtoNodeCommsRequest {
        // A canonical scalar
        let sig = PrivateKey::from(1u64).to_vec();
        ProtoNodeCommsRequest::FetchMempoolTransactionsByExcessSigs(proto::ExcessSigs {
            excess_sigs: vec![sig; count],
        })
    }

    #[test]
    fn excess_sig_requests_are_capped() {
        let converted: NodeCommsRequest = request(MAX_EXCESS_SIGS_PER_REQUEST).try_into().unwrap();
        assert!(matches!(
            converted,
            NodeCommsRequest::FetchMempoolTransactionsByExcessSigs { excess_sigs } if excess_sigs.len() == MAX_EXCESS_SIGS_PER_REQUEST
        ));

        let err = TryInto::<NodeCommsRequest>::try_into(request(MAX_EXCESS_SIGS_PER_REQUEST + 1)).unwrap_err();
        assert!(err.contains("Too many excess sigs"), "{err}");
    }
}
