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

/// Checks in `O(1)` that the element counts of a block or transaction body can fit within
/// `consensus_constants.max_block_transaction_weight()` under `consensus_constants.transaction_weight_params()`.
///
/// When `coinbase_allowed` is `true` (blocks and block templates), accounts for up to
/// `consensus_constants.max_block_coinbase_count()` coinbase outputs and exactly `1` coinbase kernel (matching
/// consensus rules in `AggregateBody::check_coinbase_output` and `TransactionWeight::calculate_body`). When `false`
/// (standalone transactions), both coinbase allowances are `0`.
///
/// Rejects oversized bodies before `TryFrom` converts individual elements into domain types or downstream handlers
/// normalise, sort, and hash the body under database locks.
pub fn check_body_counts(
    inputs_len: usize,
    outputs_len: usize,
    kernels_len: usize,
    consensus_constants: &ConsensusConstants,
    coinbase_allowed: bool,
) -> Result<(), String> {
    let params = consensus_constants.transaction_weight_params().params();
    let max_weight = consensus_constants.max_block_transaction_weight();
    let coinbase_output_allowance = if coinbase_allowed {
        consensus_constants.max_block_coinbase_count()
    } else {
        0
    };
    let coinbase_kernel_allowance = u64::from(coinbase_allowed);

    let input_count = u64::try_from(inputs_len).unwrap_or(u64::MAX);
    let non_coinbase_outputs = u64::try_from(outputs_len)
        .unwrap_or(u64::MAX)
        .saturating_sub(coinbase_output_allowance);
    let non_coinbase_kernels = u64::try_from(kernels_len)
        .unwrap_or(u64::MAX)
        .saturating_sub(coinbase_kernel_allowance);

    let min_weight = input_count
        .saturating_mul(params.input_weight)
        .saturating_add(non_coinbase_outputs.saturating_mul(params.output_weight))
        .saturating_add(non_coinbase_kernels.saturating_mul(params.kernel_weight));

    if min_weight > max_weight {
        return Err(format!(
            "AggregateBody minimum weight {min_weight} ({inputs_len} inputs, {outputs_len} outputs, {kernels_len} \
             kernels) exceeds maximum block transaction weight {max_weight}",
        ));
    }

    Ok(())
}

/// Checks in `O(1)` that the repeated-field element counts of a decoded `grpc::AggregateBody` can fit within
/// `consensus_constants.max_block_transaction_weight()` under `consensus_constants.transaction_weight_params()`.
pub fn check_body_size(
    body: &grpc::AggregateBody,
    consensus_constants: &ConsensusConstants,
    coinbase_allowed: bool,
) -> Result<(), String> {
    check_body_counts(
        body.inputs.len(),
        body.outputs.len(),
        body.kernels.len(),
        consensus_constants,
        coinbase_allowed,
    )
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
mod test {
    use tari_common::configuration::Network;
    use tari_transaction_components::consensus::ConsensusConstants;

    use super::check_body_size;
    use crate::tari_rpc as grpc;

    fn mainnet_constants() -> ConsensusConstants {
        ConsensusConstants::for_network_at_height(Network::MainNet, 0)
    }

    fn body_with_counts(inputs: usize, outputs: usize, kernels: usize) -> grpc::AggregateBody {
        grpc::AggregateBody {
            inputs: vec![grpc::TransactionInput::default(); inputs],
            outputs: vec![grpc::TransactionOutput::default(); outputs],
            kernels: vec![grpc::TransactionKernel::default(); kernels],
        }
    }

    #[test]
    fn check_body_size_enforces_input_output_and_kernel_boundaries() {
        let constants = mainnet_constants();
        let params = constants.transaction_weight_params().params();
        let max_weight = constants.max_block_transaction_weight();
        let coinbase_outputs = constants.max_block_coinbase_count();

        assert!(check_body_size(&body_with_counts(0, 0, 0), &constants, true).is_ok());

        let max_inputs = usize::try_from(max_weight.checked_div(params.input_weight).unwrap()).unwrap();
        assert_eq!(max_inputs, 11_250);
        assert!(check_body_size(&body_with_counts(max_inputs, 0, 0), &constants, true).is_ok());
        assert!(
            check_body_size(
                &body_with_counts(max_inputs.checked_add(1).unwrap(), 0, 0),
                &constants,
                true
            )
            .is_err()
        );

        let max_non_coinbase_outputs = usize::try_from(max_weight.checked_div(params.output_weight).unwrap()).unwrap();
        assert_eq!(max_non_coinbase_outputs, 1_698);
        let max_block_outputs = max_non_coinbase_outputs
            .checked_add(usize::try_from(coinbase_outputs).unwrap())
            .unwrap();
        assert!(check_body_size(&body_with_counts(0, max_block_outputs, 0), &constants, true).is_ok());
        assert!(
            check_body_size(
                &body_with_counts(0, max_block_outputs.checked_add(1).unwrap(), 0),
                &constants,
                true,
            )
            .is_err()
        );
        // Transactions carry no coinbases (coinbase_allowed = false).
        assert!(check_body_size(&body_with_counts(0, max_non_coinbase_outputs, 0), &constants, false).is_ok());
        assert!(
            check_body_size(
                &body_with_counts(0, max_non_coinbase_outputs.checked_add(1).unwrap(), 0),
                &constants,
                false,
            )
            .is_err()
        );

        let max_non_coinbase_kernels = usize::try_from(max_weight.checked_div(params.kernel_weight).unwrap()).unwrap();
        assert_eq!(max_non_coinbase_kernels, 9_000);
        // Consensus allows at most 1 coinbase kernel per block (not `max_block_coinbase_count()`).
        let max_block_kernels = max_non_coinbase_kernels.checked_add(1).unwrap();
        assert!(check_body_size(&body_with_counts(0, 0, max_block_kernels), &constants, true).is_ok());
        assert!(
            check_body_size(
                &body_with_counts(0, 0, max_block_kernels.checked_add(1).unwrap()),
                &constants,
                true,
            )
            .is_err()
        );
        assert!(check_body_size(&body_with_counts(0, 0, max_non_coinbase_kernels), &constants, false).is_ok());
        assert!(
            check_body_size(
                &body_with_counts(0, 0, max_non_coinbase_kernels.checked_add(1).unwrap()),
                &constants,
                false,
            )
            .is_err()
        );
    }

    #[test]
    fn check_body_size_rejects_mixed_body_exceeding_combined_weight() {
        let constants = mainnet_constants();

        // After subtracting coinbase allowances (1,000 outputs, 1 kernel), each count fits individually on MainNet
        // (6,000 <= 11,250; 800 <= 1,698; 100 <= 9,000), while combined non-coinbase weight is
        // 6,000*8 + 800*53 + 100*10 = 91,400 > 90,000.
        let mixed_over = body_with_counts(6_000, 1_800, 101);
        assert!(check_body_size(&mixed_over, &constants, true).is_err());

        // Reducing inputs to 5,825 brings combined weight to 5,825*8 + 800*53 + 100*10 = 90,000 == max_weight.
        let mixed_exact = body_with_counts(5_825, 1_800, 101);
        assert!(check_body_size(&mixed_exact, &constants, true).is_ok());
    }
}
