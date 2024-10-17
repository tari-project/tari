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

pub mod aggregate_body;
pub mod base_node_state;
pub mod block;
pub mod block_header;
pub mod chain_metadata;
pub mod com_and_pub_signature;
pub mod commitment_signature;
pub mod consensus_constants;
pub mod historical_block;
pub mod new_block_template;
pub mod output_features;
// pub mod peer;
pub mod connected_peer;
pub mod multiaddr;
pub mod proof_of_work;
pub mod sidechain_feature;
pub mod signature;
pub mod transaction;
pub mod transaction_input;
pub mod transaction_kernel;
pub mod transaction_output;
pub mod unblinded_output;

use prost_types::Timestamp;

use crate::{tari_rpc as grpc, tari_rpc::BlockGroupRequest};

/// Utility function that converts a `chrono::NaiveDateTime` to a `prost::Timestamp`
pub fn naive_datetime_to_timestamp(datetime: chrono::NaiveDateTime) -> Timestamp {
    Timestamp {
        seconds: datetime.timestamp(),
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
