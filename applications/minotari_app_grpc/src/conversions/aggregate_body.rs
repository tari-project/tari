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

use std::convert::TryFrom;

use tari_common_types::types::BlockHash;
use tari_transaction_components::{
    aggregated_body::AggregateBody,
    consensus::ConsensusConstants,
    weight::WeightParams,
};
use tari_utilities::convert::try_convert_all;

use crate::{conversions::transaction_output::grpc_output_with_payref, tari_rpc as grpc};

/// Checks that the element counts in an `AggregateBody` do not exceed the network's maximum block transaction weight
/// under the current consensus weight parameters, accounting for allowed coinbases.
/// Rejection is O(1) based solely on collection lengths before any element parsing or allocation.
pub fn check_body_size(body: &grpc::AggregateBody, consensus_constants: &ConsensusConstants) -> Result<(), String> {
    let max_weight = consensus_constants.max_block_transaction_weight();
    let max_coinbase = consensus_constants.max_block_coinbase_count();
    let weight_params = WeightParams::v1();

    let num_inputs = body.inputs.len() as u64;
    if num_inputs.saturating_mul(weight_params.input_weight) > max_weight {
        return Err(format!(
            "AggregateBody input count ({num_inputs}) exceeds maximum block transaction weight ({max_weight})"
        ));
    }

    let num_outputs = (body.outputs.len() as u64).saturating_sub(max_coinbase);
    if num_outputs.saturating_mul(weight_params.output_weight) > max_weight {
        return Err(format!(
            "AggregateBody output count ({}) exceeds maximum block transaction weight ({max_weight})",
            body.outputs.len()
        ));
    }

    let num_kernels = (body.kernels.len() as u64).saturating_sub(max_coinbase);
    if num_kernels.saturating_mul(weight_params.kernel_weight) > max_weight {
        return Err(format!(
            "AggregateBody kernel count ({}) exceeds maximum block transaction weight ({max_weight})",
            body.kernels.len()
        ));
    }

    let min_combined_weight = num_inputs
        .saturating_mul(weight_params.input_weight)
        .saturating_add(num_outputs.saturating_mul(weight_params.output_weight))
        .saturating_add(num_kernels.saturating_mul(weight_params.kernel_weight));

    if min_combined_weight > max_weight {
        return Err(format!(
            "AggregateBody minimum weight ({min_combined_weight}) exceeds maximum block transaction weight \
             ({max_weight})"
        ));
    }

    Ok(())
}

pub fn grpc_aggregate_body_with_payrefs(
    block: AggregateBody,
    block_hash: Option<BlockHash>,
) -> Result<grpc::AggregateBody, String> {
    let (inputs, outputs, kernels) = block.dissolve();
    Ok(grpc::AggregateBody {
        inputs: inputs
            .into_iter()
            .map(grpc::TransactionInput::try_from)
            .collect::<Result<Vec<_>, _>>()?,
        outputs: outputs
            .into_iter()
            .map(|o| grpc_output_with_payref(o, block_hash))
            .collect::<Result<Vec<_>, _>>()?,
        kernels: kernels.into_iter().map(grpc::TransactionKernel::from).collect(),
    })
}

impl TryFrom<grpc::AggregateBody> for AggregateBody {
    type Error = String;

    fn try_from(body: grpc::AggregateBody) -> Result<Self, Self::Error> {
        let inputs = try_convert_all(body.inputs).map_err(|err: String| format!("inputs {err}"))?;
        let outputs = try_convert_all(body.outputs).map_err(|err: String| format!("outputs {err}"))?;
        let kernels = try_convert_all(body.kernels).map_err(|err: String| format!("kernels {err}"))?;
        let body = AggregateBody::new_unsorted(inputs, outputs, kernels);
        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use tari_common::configuration::Network;
    use tari_transaction_components::consensus::ConsensusManager;

    use super::*;

    #[test]
    fn check_body_size_rejects_oversized_inputs_in_o1() {
        let rules = ConsensusManager::builder(Network::MainNet).build();
        let constants = rules.consensus_constants(0);
        let max_weight = constants.max_block_transaction_weight();
        let weight_params = WeightParams::v1();

        let too_many_inputs = (max_weight / weight_params.input_weight) + 1;
        let mut body = grpc::AggregateBody::default();
        body.inputs = vec![grpc::TransactionInput::default(); too_many_inputs as usize];

        let err = check_body_size(&body, constants).unwrap_err();
        assert!(err.contains("input count"));
    }

    #[test]
    fn check_body_size_accepts_valid_body() {
        let rules = ConsensusManager::builder(Network::MainNet).build();
        let constants = rules.consensus_constants(0);
        let mut body = grpc::AggregateBody::default();
        body.inputs = vec![grpc::TransactionInput::default(); 5];
        body.outputs = vec![grpc::TransactionOutput::default(); 5];
        body.kernels = vec![grpc::TransactionKernel::default(); 5];
        assert!(check_body_size(&body, constants).is_ok());
    }
}
