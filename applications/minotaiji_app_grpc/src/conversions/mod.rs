// Copyright 2020. The Taiji Project
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

mod aggregate_body;
mod base_node_state;
mod block;
mod block_header;
mod chain_metadata;
mod com_and_pub_signature;
mod commitment_signature;
mod consensus_constants;
mod historical_block;
mod new_block_template;
mod output_features;
mod peer;
mod proof_of_work;
mod sidechain_feature;
mod signature;
mod transaction;
mod transaction_input;
mod transaction_kernel;
mod transaction_output;
mod unblinded_output;

use std::convert::TryFrom;

use chrono::Utc;
use prost_types::Timestamp;
use tari_utilities::epoch_time::EpochTime;

pub use self::{
    aggregate_body::*,
    base_node_state::*,
    block::*,
    block_header::*,
    chain_metadata::*,
    com_and_pub_signature::*,
    consensus_constants::*,
    historical_block::*,
    new_block_template::*,
    output_features::*,
    peer::*,
    proof_of_work::*,
    signature::*,
    transaction::*,
    transaction_input::*,
    transaction_kernel::*,
    transaction_output::*,
};
use crate::{taiji_rpc as grpc, taiji_rpc::BlockGroupRequest};

/// Utility function that converts a `EpochTime` to a `prost::Timestamp`
/// Returns None if the EpochTime is negative or larger than i64::MAX.
pub(self) fn datetime_to_timestamp(datetime: EpochTime) -> Option<Timestamp> {
    let seconds = i64::try_from(datetime.as_u64()).ok()?;
    Some(Timestamp { seconds, nanos: 0 })
}

/// Utility function that converts a `chrono::NaiveDateTime` to a `prost::Timestamp`
pub fn naive_datetime_to_timestamp(datetime: chrono::NaiveDateTime) -> Timestamp {
    Timestamp {
        seconds: datetime.timestamp(),
        nanos: 0,
    }
}

/// Converts a protobuf Timestamp to an EpochTime.
/// Returns None if the timestamp is negative.
pub(self) fn timestamp_to_datetime(timestamp: Timestamp) -> Option<EpochTime> {
    u64::try_from(timestamp.seconds).ok().map(Into::into)
}

/// Current unix time as timestamp (seconds part only)
pub fn timestamp() -> Timestamp {
    Timestamp {
        seconds: Utc::now().timestamp(),
        nanos: 0,
    }
}

impl From<u64> for grpc::IntegerValue {
    fn from(value: u64) -> Self {
        Self { value }
    }
}

impl From<String> for grpc::StringValue {
    fn from(value: String) -> Self {
        Self { value }
    }
}

impl From<grpc::BlockGroupRequest> for grpc::HeightRequest {
    fn from(b: BlockGroupRequest) -> Self {
        Self {
            from_tip: b.from_tip,
            start_height: b.start_height,
            end_height: b.end_height,
        }
    }
}
