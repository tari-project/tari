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

use super::mempool::mempool_service_response::Response as ProtoMempoolResponse;
use crate::mempool::{
    proto::mempool::{
        MempoolServiceResponse as ProtoMempoolServiceResponse,
        TxStorageResponse as ProtoTxStorageResponse,
    },
    service::{MempoolResponse, MempoolServiceResponse},
};
use std::convert::{TryFrom, TryInto};

impl TryInto<MempoolResponse> for ProtoMempoolResponse {
    type Error = String;

    fn try_into(self) -> Result<MempoolResponse, Self::Error> {
        use ProtoMempoolResponse::*;
        let response = match self {
            Stats(stats_response) => MempoolResponse::Stats(stats_response.try_into()?),
            State(state_response) => MempoolResponse::State(state_response.try_into()?),
            TxStorage(tx_storage_response) => {
                let tx_storage_response = ProtoTxStorageResponse::from_i32(tx_storage_response)
                    .ok_or_else(|| "Invalid or unrecognised `TxStorageResponse` enum".to_string())?;
                MempoolResponse::TxStorage(tx_storage_response.try_into()?)
            },
        };
        Ok(response)
    }
}

impl TryFrom<ProtoMempoolServiceResponse> for MempoolServiceResponse {
    type Error = String;

    fn try_from(response: ProtoMempoolServiceResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            request_key: response.request_key,
            response: response
                .response
                .ok_or_else(|| "Response field not present to convert".to_string())?
                .try_into()?,
        })
    }
}

impl From<MempoolResponse> for ProtoMempoolResponse {
    fn from(response: MempoolResponse) -> Self {
        use MempoolResponse::*;
        match response {
            Stats(stats_response) => ProtoMempoolResponse::Stats(stats_response.into()),
            State(state_response) => ProtoMempoolResponse::State(state_response.into()),
            TxStorage(tx_storage_response) => {
                let tx_storage_response: ProtoTxStorageResponse = tx_storage_response.into();
                ProtoMempoolResponse::TxStorage(tx_storage_response.into())
            },
        }
    }
}
