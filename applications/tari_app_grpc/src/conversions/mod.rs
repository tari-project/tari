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

mod aggregate_body;
mod base_node_state;
mod block;
mod block_header;
mod chain_metadata;
mod com_signature;
mod consensus_constants;
mod historical_block;
mod new_block_template;
mod output_features;
mod peer;
mod proof_of_work;
mod signature;
mod transaction;
mod transaction_input;
mod transaction_kernel;
mod transaction_output;
mod unblinded_output;

pub use self::{
    aggregate_body::*,
    base_node_state::*,
    block::*,
    block_header::*,
    chain_metadata::*,
    com_signature::*,
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

use crate::{tari_rpc as grpc, tari_rpc::BlockGroupRequest};
use prost_types::Timestamp;
use tari_core::crypto::tari_utilities::epoch_time::EpochTime;

/// Utility function that converts a `EpochTime` to a `prost::Timestamp`
pub fn datetime_to_timestamp(datetime: EpochTime) -> Timestamp {
    Timestamp {
        seconds: datetime.as_u64() as i64,
        nanos: 0,
    }
}

/// Utility function that converts a `chrono::NaiveDateTime` to a `prost::Timestamp`
pub fn naive_datetime_to_timestamp(datetime: chrono::NaiveDateTime) -> Timestamp {
    Timestamp {
        seconds: datetime.timestamp(),
        nanos: 0,
    }
}

pub(crate) fn timestamp_to_datetime(timestamp: Timestamp) -> EpochTime {
    (timestamp.seconds as u64).into()
}

/// Current unix time as timestamp (seconds part only)
pub fn timestamp() -> Timestamp {
    datetime_to_timestamp(EpochTime::now())
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
