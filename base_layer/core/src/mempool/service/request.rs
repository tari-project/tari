//  Copyright 2019 The Taiji Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use core::fmt::{Display, Error, Formatter};

use serde::{Deserialize, Serialize};
use taiji_common_types::{types::Signature, waiting_requests::RequestKey};
use tari_utilities::hex::Hex;

use crate::transactions::transaction_components::Transaction;

/// API Request enum for Mempool requests.
#[derive(Debug, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum MempoolRequest {
    GetStats,
    GetState,
    GetTxStateByExcessSig(Signature),
    SubmitTransaction(Transaction),
    GetFeePerGramStats { count: usize, tip_height: u64 },
}

impl Display for MempoolRequest {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            MempoolRequest::GetStats => write!(f, "GetStats"),
            MempoolRequest::GetState => write!(f, "GetState"),
            MempoolRequest::GetTxStateByExcessSig(sig) => {
                write!(f, "GetTxStateByExcessSig ({})", sig.get_signature().to_hex())
            },
            MempoolRequest::SubmitTransaction(tx) => write!(
                f,
                "SubmitTransaction ({})",
                tx.body.kernels()[0].excess_sig.get_signature().to_hex()
            ),
            MempoolRequest::GetFeePerGramStats { count, tip_height } => {
                write!(f, "GetFeePerGramStats(count: {}, tip_height: {})", *count, *tip_height)
            },
        }
    }
}

/// Request type for a received MempoolService request.
#[derive(Debug, Serialize, Deserialize)]
pub struct MempoolServiceRequest {
    pub request_key: RequestKey,
    pub request: MempoolRequest,
}
