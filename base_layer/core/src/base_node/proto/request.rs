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

use tari_common_types::types::HashOutput;

use crate::{
    base_node::comms_interface as ci,
    proto::base_node::{base_node_service_request::Request as ProtoNodeCommsRequest, BlockHeights, HashOutputs},
};

//---------------------------------- BaseNodeRequest --------------------------------------------//
impl TryInto<ci::NodeCommsRequest> for ProtoNodeCommsRequest {
    type Error = String;

    fn try_into(self) -> Result<ci::NodeCommsRequest, Self::Error> {
        use ProtoNodeCommsRequest::*;
        let request = match self {
            FetchBlocksByHash(block_hashes) => ci::NodeCommsRequest::FetchBlocksByHash(block_hashes.outputs),
        };
        Ok(request)
    }
}

impl TryFrom<ci::NodeCommsRequest> for ProtoNodeCommsRequest {
    type Error = String;

    fn try_from(request: ci::NodeCommsRequest) -> Result<Self, Self::Error> {
        use ci::NodeCommsRequest::*;
        match request {
            FetchBlocksByHash(block_hashes) => Ok(ProtoNodeCommsRequest::FetchBlocksByHash(block_hashes.into())),
            e => Err(format!("{} request is not supported", e)),
        }
    }
}

//---------------------------------- Wrappers --------------------------------------------//

impl From<Vec<HashOutput>> for HashOutputs {
    fn from(outputs: Vec<HashOutput>) -> Self {
        Self { outputs }
    }
}

impl From<Vec<u64>> for BlockHeights {
    fn from(heights: Vec<u64>) -> Self {
        Self { heights }
    }
}
