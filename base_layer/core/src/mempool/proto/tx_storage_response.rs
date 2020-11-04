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

use crate::mempool::{proto::mempool as proto, TxStorageResponse};
use std::convert::TryFrom;

impl TryFrom<proto::TxStorageResponse> for TxStorageResponse {
    type Error = String;

    fn try_from(tx_storage: proto::TxStorageResponse) -> Result<Self, Self::Error> {
        use proto::TxStorageResponse::*;
        Ok(match tx_storage {
            None => return Err("TxStorageResponse not provided".to_string()),
            UnconfirmedPool => TxStorageResponse::UnconfirmedPool,
            OrphanPool => TxStorageResponse::OrphanPool,
            PendingPool => TxStorageResponse::PendingPool,
            ReorgPool => TxStorageResponse::ReorgPool,
            NotStored => TxStorageResponse::NotStored,
        })
    }
}

impl From<TxStorageResponse> for proto::TxStorageResponse {
    fn from(resp: TxStorageResponse) -> Self {
        use TxStorageResponse::*;
        match resp {
            UnconfirmedPool => proto::TxStorageResponse::UnconfirmedPool,
            OrphanPool => proto::TxStorageResponse::OrphanPool,
            PendingPool => proto::TxStorageResponse::PendingPool,
            ReorgPool => proto::TxStorageResponse::ReorgPool,
            NotStored => proto::TxStorageResponse::NotStored,
        }
    }
}

impl From<TxStorageResponse> for proto::TxStorage {
    fn from(resp: TxStorageResponse) -> Self {
        Self {
            response: proto::TxStorageResponse::from(resp) as i32,
        }
    }
}
