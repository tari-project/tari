// Copyright 2020. The Tari Project
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

use crate::proto::base_node as proto;
use serde::{Deserialize, Serialize};
use std::{
    convert::TryFrom,
    fmt::{Display, Error, Formatter},
};
use tari_common_types::types::BlockHash;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TxSubmissionResponse {
    pub accepted: bool,
    pub rejection_reason: TxSubmissionRejectionReason,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TxSubmissionRejectionReason {
    None,
    AlreadyMined,
    DoubleSpend,
    Orphan,
    TimeLocked,
    ValidationFailed,
}

impl Display for TxSubmissionRejectionReason {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), Error> {
        let response = match self {
            TxSubmissionRejectionReason::AlreadyMined => "Already Mined ",
            TxSubmissionRejectionReason::DoubleSpend => "Double Spend",
            TxSubmissionRejectionReason::Orphan => "Orphan",
            TxSubmissionRejectionReason::TimeLocked => "Time Locked",
            TxSubmissionRejectionReason::ValidationFailed => "Validation Failed",
            TxSubmissionRejectionReason::None => "None",
        };
        fmt.write_str(&response)
    }
}

impl TryFrom<proto::TxSubmissionRejectionReason> for TxSubmissionRejectionReason {
    type Error = String;

    fn try_from(tx_rejection_reason: proto::TxSubmissionRejectionReason) -> Result<Self, Self::Error> {
        use proto::TxSubmissionRejectionReason::*;
        Ok(match tx_rejection_reason {
            None => TxSubmissionRejectionReason::None,
            AlreadyMined => TxSubmissionRejectionReason::AlreadyMined,
            DoubleSpend => TxSubmissionRejectionReason::DoubleSpend,
            Orphan => TxSubmissionRejectionReason::Orphan,
            TimeLocked => TxSubmissionRejectionReason::TimeLocked,
            ValidationFailed => TxSubmissionRejectionReason::ValidationFailed,
        })
    }
}

impl From<TxSubmissionRejectionReason> for proto::TxSubmissionRejectionReason {
    fn from(resp: TxSubmissionRejectionReason) -> Self {
        use TxSubmissionRejectionReason::*;
        match resp {
            None => proto::TxSubmissionRejectionReason::None,
            AlreadyMined => proto::TxSubmissionRejectionReason::AlreadyMined,
            DoubleSpend => proto::TxSubmissionRejectionReason::DoubleSpend,
            Orphan => proto::TxSubmissionRejectionReason::Orphan,
            TimeLocked => proto::TxSubmissionRejectionReason::TimeLocked,
            ValidationFailed => proto::TxSubmissionRejectionReason::ValidationFailed,
        }
    }
}

impl TryFrom<proto::TxSubmissionResponse> for TxSubmissionResponse {
    type Error = String;

    fn try_from(value: proto::TxSubmissionResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            accepted: value.accepted,
            rejection_reason: TxSubmissionRejectionReason::try_from(
                proto::TxSubmissionRejectionReason::from_i32(value.rejection_reason)
                    .ok_or_else(|| "Invalid or unrecognised `TxSubmissionRejectionReason` enum".to_string())?,
            )?,
        })
    }
}

impl From<TxSubmissionResponse> for proto::TxSubmissionResponse {
    fn from(value: TxSubmissionResponse) -> Self {
        Self {
            accepted: value.accepted,
            rejection_reason: proto::TxSubmissionRejectionReason::from(value.rejection_reason) as i32,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TxQueryResponse {
    pub location: TxLocation,
    pub block_hash: Option<BlockHash>,
    pub confirmations: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TxLocation {
    NotStored,
    InMempool,
    Mined,
}

impl Display for TxLocation {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), Error> {
        let response = match self {
            TxLocation::NotStored => "Not Stored",
            TxLocation::InMempool => "In Mempool",
            TxLocation::Mined => "Mined",
        };
        fmt.write_str(&response)
    }
}

impl TryFrom<proto::TxLocation> for TxLocation {
    type Error = String;

    fn try_from(tx_location: proto::TxLocation) -> Result<Self, Self::Error> {
        use proto::TxLocation::*;
        Ok(match tx_location {
            None => return Err("TxLocation not provided".to_string()),
            NotStored => TxLocation::NotStored,
            InMempool => TxLocation::InMempool,
            Mined => TxLocation::Mined,
        })
    }
}

impl From<TxLocation> for proto::TxLocation {
    fn from(resp: TxLocation) -> Self {
        use TxLocation::*;
        match resp {
            NotStored => proto::TxLocation::NotStored,
            InMempool => proto::TxLocation::InMempool,
            Mined => proto::TxLocation::Mined,
        }
    }
}

impl TryFrom<proto::TxQueryResponse> for TxQueryResponse {
    type Error = String;

    fn try_from(proto_response: proto::TxQueryResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            location: TxLocation::try_from(
                proto::TxLocation::from_i32(proto_response.location)
                    .ok_or_else(|| "Invalid or unrecognised `TxLocation` enum".to_string())?,
            )?,
            block_hash: proto_response.block_hash,
            confirmations: proto_response.confirmations,
        })
    }
}

impl From<TxQueryResponse> for proto::TxQueryResponse {
    fn from(response: TxQueryResponse) -> Self {
        Self {
            location: proto::TxLocation::from(response.location) as i32,
            block_hash: response.block_hash,
            confirmations: response.confirmations,
        }
    }
}
