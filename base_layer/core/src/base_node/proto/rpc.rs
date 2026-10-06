//  Copyright 2020, The Tari Project
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

use std::convert::{TryFrom, TryInto};

use tari_node_components::blocks::Block;
use tari_transaction_components::rpc::models::FeePerGramStat;
use tari_utilities::ByteArray;

use crate::proto::base_node as proto;

impl TryFrom<Block> for proto::BlockBodyResponse {
    type Error = String;

    fn try_from(block: Block) -> Result<Self, Self::Error> {
        Ok(Self {
            hash: block.hash().to_vec(),
            body: Some(block.body.try_into()?),
        })
    }
}

impl From<Vec<FeePerGramStat>> for proto::GetMempoolFeePerGramStatsResponse {
    fn from(stats: Vec<FeePerGramStat>) -> Self {
        Self {
            stats: stats.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<FeePerGramStat> for proto::MempoolFeePerGramStat {
    fn from(stat: FeePerGramStat) -> Self {
        Self {
            order: stat.order,
            min_fee_per_gram: stat.min_fee_per_gram.as_u64(),
            avg_fee_per_gram: stat.avg_fee_per_gram.as_u64(),
            max_fee_per_gram: stat.max_fee_per_gram.as_u64(),
        }
    }
}

#[cfg(test)]
mod test {
    use prost::Message;
    use tari_common_types::types::{ComAndPubSignature, FixedHash};
    use tari_comms::protocol::rpc::max_response_payload_size;
    use tari_node_components::blocks::BlockHeader;
    use tari_script::{CompressedCheckSigSchnorrSignature, ExecutionStack, MAX_STACK_SIZE, StackItem};
    use tari_transaction_components::{
        aggregated_body::AggregateBody,
        consensus::consensus_constants::MAX_BLOCK_BODY_BYTES,
        helpers::borsh::SerializedSize,
        transaction_components::{SpentOutput, TransactionInput},
    };

    use super::*;

    /// A compact input carrying the largest possible `input_data`
    fn max_input_data_input() -> TransactionInput {
        let input_data = ExecutionStack::new(vec![
            StackItem::Signature(CompressedCheckSigSchnorrSignature::default());
            MAX_STACK_SIZE
        ]);
        TransactionInput::new_current_version(
            SpentOutput::OutputHash(FixedHash::zero()),
            input_data,
            ComAndPubSignature::default(),
        )
    }

    /// A block body at the consensus byte limit, made of maximum `input_data` inputs, must fit in a single block sync
    /// RPC response (one `BlockBodyResponse` per block).
    #[test]
    fn a_block_body_at_the_byte_limit_fits_in_one_rpc_response() {
        let input = max_input_data_input();
        let input_size = input.get_serialized_size().unwrap();
        let empty_size = AggregateBody::empty().compact_serialized_size().unwrap();
        // The most inputs that fit within the limit, plus one: this body is just over the limit, so it bounds the
        // encoding of any body at or under the limit from above.
        let num_inputs = (MAX_BLOCK_BODY_BYTES - empty_size) / input_size + 1;
        let body = AggregateBody::new_unsorted(vec![input; num_inputs], vec![], vec![]);
        let body_size = body.compact_serialized_size().unwrap();
        assert!(body_size > MAX_BLOCK_BODY_BYTES);
        assert!(body_size - MAX_BLOCK_BODY_BYTES < input_size);

        let block = Block::new(BlockHeader::new(0), body);
        let response = proto::BlockBodyResponse::try_from(block).unwrap();
        let encoded_len = response.encoded_len();
        assert!(
            encoded_len <= max_response_payload_size(),
            "{encoded_len} bytes does not fit in an RPC response of {} bytes",
            max_response_payload_size()
        );
    }
}
