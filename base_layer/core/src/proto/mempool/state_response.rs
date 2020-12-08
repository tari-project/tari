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

use crate::{mempool::StateResponse, proto};
use std::convert::{TryFrom, TryInto};
use tari_crypto::tari_utilities::ByteArrayError;

//--------------------------------- StateResponse -------------------------------------------//

impl TryFrom<proto::mempool::StateResponse> for StateResponse {
    type Error = String;

    fn try_from(state: proto::mempool::StateResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            unconfirmed_pool: state
                .unconfirmed_pool
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|err: ByteArrayError| err.to_string())?,
            orphan_pool: state
                .orphan_pool
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|err: ByteArrayError| err.to_string())?,
            pending_pool: state
                .pending_pool
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|err: ByteArrayError| err.to_string())?,
            reorg_pool: state
                .reorg_pool
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|err: ByteArrayError| err.to_string())?,
        })
    }
}

impl From<StateResponse> for proto::mempool::StateResponse {
    fn from(state: StateResponse) -> Self {
        Self {
            unconfirmed_pool: state.unconfirmed_pool.into_iter().map(Into::into).collect(),
            orphan_pool: state.orphan_pool.into_iter().map(Into::into).collect(),
            pending_pool: state.pending_pool.into_iter().map(Into::into).collect(),
            reorg_pool: state.reorg_pool.into_iter().map(Into::into).collect(),
        }
    }
}
