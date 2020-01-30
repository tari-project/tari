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

use super::mempool::{
    mempool_service_request::Request as ProtoMempoolRequest,
    MempoolServiceRequest as ProtoMempoolServiceRequest,
};
use crate::mempool::service::{MempoolRequest, MempoolServiceRequest};
use std::convert::{TryFrom, TryInto};
use tari_utilities::ByteArrayError;

impl TryInto<MempoolRequest> for ProtoMempoolRequest {
    type Error = String;

    fn try_into(self) -> Result<MempoolRequest, Self::Error> {
        use ProtoMempoolRequest::*;
        let request = match self {
            // Field was not specified
            GetStats(_) => MempoolRequest::GetStats,
            GetTxStateWithExcessSig(excess_sig) => MempoolRequest::GetTxStateWithExcessSig(
                excess_sig.try_into().map_err(|err: ByteArrayError| err.to_string())?,
            ),
        };
        Ok(request)
    }
}

impl From<MempoolRequest> for ProtoMempoolRequest {
    fn from(request: MempoolRequest) -> Self {
        use MempoolRequest::*;
        match request {
            GetStats => ProtoMempoolRequest::GetStats(true),
            GetTxStateWithExcessSig(excess_sig) => ProtoMempoolRequest::GetTxStateWithExcessSig(excess_sig.into()),
        }
    }
}

impl TryFrom<ProtoMempoolServiceRequest> for MempoolServiceRequest {
    type Error = String;

    fn try_from(request: ProtoMempoolServiceRequest) -> Result<Self, Self::Error> {
        Ok(Self {
            request_key: request.request_key,
            request: request
                .request
                .ok_or("Response field not present".to_string())?
                .try_into()?,
        })
    }
}
