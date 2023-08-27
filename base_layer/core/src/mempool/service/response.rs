// Copyright 2019 The Taiji Project
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

use std::{fmt, fmt::Formatter};

use taiji_common_types::waiting_requests::RequestKey;

use crate::mempool::{FeePerGramStat, StateResponse, StatsResponse, TxStorageResponse};

/// API Response enum for Mempool responses.
#[derive(Clone, Debug)]
pub enum MempoolResponse {
    Stats(StatsResponse),
    State(StateResponse),
    TxStorage(TxStorageResponse),
    FeePerGramStats { response: Vec<FeePerGramStat> },
}

impl fmt::Display for MempoolResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        use MempoolResponse::{FeePerGramStats, State, Stats, TxStorage};
        match &self {
            Stats(_) => write!(f, "Stats"),
            State(_) => write!(f, "State"),
            TxStorage(_) => write!(f, "TxStorage"),
            FeePerGramStats { response } => write!(f, "FeePerGramStats({} item(s))", response.len()),
        }
    }
}

/// Response type for a received MempoolService requests
#[derive(Clone, Debug)]
pub struct MempoolServiceResponse {
    pub request_key: RequestKey,
    pub response: MempoolResponse,
}
