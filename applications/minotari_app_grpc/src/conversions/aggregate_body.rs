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
use tari_transaction_components::{aggregated_body::AggregateBody, consensus::ConsensusConstants};
use tari_utilities::convert::try_convert_all;

use crate::{conversions::transaction_output::grpc_output_with_payref, tari_rpc as grpc};

/// Checks that the input, output and kernel counts of a block or block-template gRPC `AggregateBody` can fit within
/// the network's `max_block_transaction_weight` under `transaction_weight_params()`, allowing up to
/// `max_block_coinbase_count()` coinbase outputs and 1 coinbase kernel, without iterating any of the element vectors.
pub fn check_body_size(body: &grpc::AggregateBody, consensus_constants: &ConsensusConstants) -> Result<(), String> {
    check_body_element_counts(
        body.inputs.len(),
        body.outputs.len(),
        body.kernels.len(),
        consensus_constants,
        consensus_constants.max_block_coinbase_count(),
        1,
    )
}

/// Checks that `(num_inputs, num_outputs, num_kernels)` can fit within `consensus_constants`'s
/// `max_block_transaction_weight` under `transaction_weight_params()`, subtracting up to `max_coinbase_outputs`
/// outputs and `max_coinbase_kernels` kernels (`max_block_coinbase_count()` and `1` for blocks/templates, or `0` and
/// `0` for transactions).
pub fn check_body_element_counts(
    num_inputs: usize,
    num_outputs: usize,
    num_kernels: usize,
    consensus_constants: &ConsensusConstants,
    max_coinbase_outputs: u64,
    max_coinbase_kernels: u64,
) -> Result<(), String> {
    let max_weight = consensus_constants.max_block_transaction_weight();
    let params = consensus_constants.transaction_weight_params().params();

    let num_inputs_u64 = u64::try_from(num_inputs).map_err(|_| "Body input count overflowed u64".to_string())?;
    let num_outputs_u64 = u64::try_from(num_outputs).map_err(|_| "Body output count overflowed u64".to_string())?;
    let num_kernels_u64 = u64::try_from(num_kernels).map_err(|_| "Body kernel count overflowed u64".to_string())?;

    let non_coinbase_outputs = num_outputs_u64.saturating_sub(max_coinbase_outputs);
    let non_coinbase_kernels = num_kernels_u64.saturating_sub(max_coinbase_kernels);

    let input_weight = params
        .input_weight
        .checked_mul(num_inputs_u64)
        .ok_or_else(|| "Body input weight overflowed".to_string())?;
    let output_weight = params
        .output_weight
        .checked_mul(non_coinbase_outputs)
        .ok_or_else(|| "Body output weight overflowed".to_string())?;
    let kernel_weight = params
        .kernel_weight
        .checked_mul(non_coinbase_kernels)
        .ok_or_else(|| "Body kernel weight overflowed".to_string())?;

    let min_weight = input_weight
        .checked_add(output_weight)
        .and_then(|w| w.checked_add(kernel_weight))
        .ok_or_else(|| "Body weight overflowed".to_string())?;

    if min_weight > max_weight {
        return Err(format!(
            "AggregateBody element counts ({num_inputs} inputs, {num_outputs} outputs, {num_kernels} kernels) exceed \
             maximum block transaction weight {max_weight} (minimum weight {min_weight})"
        ));
    }

    Ok(())
}

pub fn grpc_aggregate_body_with_payrefs(
    block: AggregateBody,
    block_hash: Option<BlockHash>,
) -> Result<grpc::AggregateBody, String> {
    let (inputs, outputs, kernels) = block.clone().dissolve();
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
mod test {
    #![allow(clippy::arithmetic_side_effects)]
    use super::*;

    fn mainnet_constants() -> ConsensusConstants {
        ConsensusConstants::mainnet()
            .pop()
            .expect("mainnet consensus constants are non-empty")
    }

    #[test]
    fn check_body_size_enforces_mainnet_weight_bounds_without_iterating() {
        let constants = mainnet_constants();
        let max_weight = constants.max_block_transaction_weight();
        let max_coinbases = constants.max_block_coinbase_count();
        let params = constants.transaction_weight_params().params();

        let max_inputs = usize::try_from(max_weight / params.input_weight).unwrap();
        let max_outputs = usize::try_from(max_weight / params.output_weight + max_coinbases).unwrap();
        let max_kernels = usize::try_from(max_weight / params.kernel_weight + 1).unwrap();

        let at_input_limit = grpc::AggregateBody {
            inputs: vec![grpc::TransactionInput::default(); max_inputs],
            outputs: vec![],
            kernels: vec![],
        };
        assert!(check_body_size(&at_input_limit, &constants).is_ok());

        let over_input_limit = grpc::AggregateBody {
            inputs: vec![grpc::TransactionInput::default(); max_inputs + 1],
            outputs: vec![],
            kernels: vec![],
        };
        assert!(check_body_size(&over_input_limit, &constants).is_err());

        let at_output_limit = grpc::AggregateBody {
            inputs: vec![],
            outputs: vec![grpc::TransactionOutput::default(); max_outputs],
            kernels: vec![],
        };
        assert!(check_body_size(&at_output_limit, &constants).is_ok());

        let over_output_limit = grpc::AggregateBody {
            inputs: vec![],
            outputs: vec![grpc::TransactionOutput::default(); max_outputs + 1],
            kernels: vec![],
        };
        assert!(check_body_size(&over_output_limit, &constants).is_err());

        let at_kernel_limit = grpc::AggregateBody {
            inputs: vec![],
            outputs: vec![],
            kernels: vec![grpc::TransactionKernel::default(); max_kernels],
        };
        assert!(check_body_size(&at_kernel_limit, &constants).is_ok());

        let over_kernel_limit = grpc::AggregateBody {
            inputs: vec![],
            outputs: vec![],
            kernels: vec![grpc::TransactionKernel::default(); max_kernels + 1],
        };
        assert!(check_body_size(&over_kernel_limit, &constants).is_err());

        // Transactions get no coinbase allowance (0 outputs, 0 kernels).
        let tx_max_outputs = usize::try_from(max_weight / params.output_weight).unwrap();
        let tx_max_kernels = usize::try_from(max_weight / params.kernel_weight).unwrap();
        assert!(check_body_element_counts(0, tx_max_outputs, 0, &constants, 0, 0).is_ok());
        assert!(check_body_element_counts(0, tx_max_outputs + 1, 0, &constants, 0, 0).is_err());
        assert!(check_body_element_counts(0, 0, tx_max_kernels, &constants, 0, 0).is_ok());
        assert!(check_body_element_counts(0, 0, tx_max_kernels + 1, &constants, 0, 0).is_err());
    }
}
