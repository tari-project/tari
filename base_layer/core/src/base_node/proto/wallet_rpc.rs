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

use std::{
    convert::TryFrom,
    fmt::{Display, Error, Formatter},
};

use serde::{Deserialize, Serialize};
use tari_common_types::types::{BlockHash, Signature};
use tari_utilities::ByteArray;

use crate::proto::base_node as proto;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TxSubmissionResponse {
    pub accepted: bool,
    pub rejection_reason: TxSubmissionRejectionReason,
    pub is_synced: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TxSubmissionRejectionReason {
    None,
    AlreadyMined,
    DoubleSpend,
    Orphan,
    TimeLocked,
    ValidationFailed,
    FeeTooLow,
}

impl Display for TxSubmissionRejectionReason {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), Error> {
        #[allow(clippy::enum_glob_use)]
        use TxSubmissionRejectionReason::*;
        let response = match self {
            AlreadyMined => "Already Mined ",
            DoubleSpend => "Double Spend",
            Orphan => "Orphan",
            TimeLocked => "Time Locked",
            ValidationFailed => "Validation Failed",
            FeeTooLow => "Fee too low",
            None => "None",
        };
        fmt.write_str(response)
    }
}

impl TryFrom<proto::TxSubmissionRejectionReason> for TxSubmissionRejectionReason {
    type Error = String;

    fn try_from(tx_rejection_reason: proto::TxSubmissionRejectionReason) -> Result<Self, Self::Error> {
        #[allow(clippy::enum_glob_use)]
        use proto::TxSubmissionRejectionReason::*;
        Ok(match tx_rejection_reason {
            None => TxSubmissionRejectionReason::None,
            AlreadyMined => TxSubmissionRejectionReason::AlreadyMined,
            DoubleSpend => TxSubmissionRejectionReason::DoubleSpend,
            Orphan => TxSubmissionRejectionReason::Orphan,
            TimeLocked => TxSubmissionRejectionReason::TimeLocked,
            ValidationFailed => TxSubmissionRejectionReason::ValidationFailed,
            FeeTooLow => TxSubmissionRejectionReason::FeeTooLow,
        })
    }
}

impl From<TxSubmissionRejectionReason> for proto::TxSubmissionRejectionReason {
    fn from(response: TxSubmissionRejectionReason) -> Self {
        #[allow(clippy::enum_glob_use)]
        use TxSubmissionRejectionReason::*;
        match response {
            None => proto::TxSubmissionRejectionReason::None,
            AlreadyMined => proto::TxSubmissionRejectionReason::AlreadyMined,
            DoubleSpend => proto::TxSubmissionRejectionReason::DoubleSpend,
            Orphan => proto::TxSubmissionRejectionReason::Orphan,
            TimeLocked => proto::TxSubmissionRejectionReason::TimeLocked,
            ValidationFailed => proto::TxSubmissionRejectionReason::ValidationFailed,
            FeeTooLow => proto::TxSubmissionRejectionReason::FeeTooLow,
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
            is_synced: value.is_synced,
        })
    }
}

impl From<TxSubmissionResponse> for proto::TxSubmissionResponse {
    fn from(value: TxSubmissionResponse) -> Self {
        Self {
            accepted: value.accepted,
            rejection_reason: proto::TxSubmissionRejectionReason::from(value.rejection_reason) as i32,
            is_synced: value.is_synced,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TxQueryResponse {
    pub location: TxLocation,
    pub best_block_hash: Option<BlockHash>,
    pub confirmations: u64,
    pub is_synced: bool,
    pub best_block_height: u64,
    pub mined_timestamp: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TxQueryBatchResponse {
    pub signature: Signature,
    pub location: TxLocation,
    pub best_block_hash: Option<BlockHash>,
    pub confirmations: u64,
    pub best_block_height: u64,
    pub mined_timestamp: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
        fmt.write_str(response)
    }
}

impl TryFrom<proto::TxLocation> for TxLocation {
    type Error = String;

    fn try_from(tx_location: proto::TxLocation) -> Result<Self, Self::Error> {
        use proto::TxLocation::{InMempool, Mined, None, NotStored};
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
        use TxLocation::{InMempool, Mined, NotStored};
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
        let hash = if proto_response.best_block_hash.is_empty() {
            None
        } else {
            Some(match BlockHash::try_from(proto_response.best_block_hash.clone()) {
                Ok(h) => h,
                Err(e) => {
                    return Err(format!("Malformed block hash: {}", e));
                },
            })
        };

        let mined_timestamp = match proto_response.mined_timestamp {
            0 => None,
            t => Some(t),
        };

        Ok(Self {
            location: TxLocation::try_from(
                proto::TxLocation::from_i32(proto_response.location)
                    .ok_or_else(|| "Invalid or unrecognised `TxLocation` enum".to_string())?,
            )?,
            best_block_hash: hash,
            confirmations: proto_response.confirmations,
            is_synced: proto_response.is_synced,
            best_block_height: proto_response.best_block_height,
            mined_timestamp,
        })
    }
}

impl From<TxQueryResponse> for proto::TxQueryResponse {
    fn from(response: TxQueryResponse) -> Self {
        Self {
            location: proto::TxLocation::from(response.location) as i32,
            best_block_hash: response.best_block_hash.map(|v| v.to_vec()).unwrap_or_default(),
            confirmations: response.confirmations,
            is_synced: response.is_synced,
            best_block_height: response.best_block_height,
            mined_timestamp: response.mined_timestamp.unwrap_or_default(),
        }
    }
}

impl TryFrom<proto::TxQueryBatchResponse> for TxQueryBatchResponse {
    type Error = String;

    fn try_from(proto_response: proto::TxQueryBatchResponse) -> Result<Self, Self::Error> {
        let hash = if proto_response.best_block_hash.is_empty() {
            None
        } else {
            Some(match BlockHash::try_from(proto_response.best_block_hash.clone()) {
                Ok(h) => h,
                Err(e) => {
                    return Err(format!("Malformed block hash: {}", e));
                },
            })
        };
        let mined_timestamp = match proto_response.mined_timestamp {
            0 => None,
            t => Some(t),
        };
        Ok(Self {
            signature: Signature::try_from(
                proto_response
                    .signature
                    .ok_or_else(|| "Signature not present".to_string())?,
            )?,
            location: TxLocation::try_from(
                proto::TxLocation::from_i32(proto_response.location)
                    .ok_or_else(|| "Invalid or unrecognised `TxLocation` enum".to_string())?,
            )?,
            best_block_hash: hash,
            best_block_height: proto_response.best_block_height,
            confirmations: proto_response.confirmations,
            mined_timestamp,
        })
    }
}
