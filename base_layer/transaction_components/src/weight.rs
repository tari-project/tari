// Copyright 2019. The Tari Project
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

use std::{convert::TryFrom, num::NonZeroU64};

use serde::{Deserialize, Serialize};

use crate::aggregated_body::AggregateBody;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WeightParams {
    /// Weight in grams per kernel
    pub kernel_weight: u64,
    /// Weight in grams per input
    pub input_weight: u64,
    /// Weight in grams per output, excl. TariScript and OutputFeatures
    pub output_weight: u64,
    /// Features and scripts per byte weight
    pub features_and_scripts_bytes_per_gram: NonZeroU64,
}

impl WeightParams {
    pub const fn v1() -> Self {
        Self {
            kernel_weight: 10,
            input_weight: 8,
            output_weight: 53,
            // SAFETY: the value isn't 0. NonZeroU64::new(x).expect(...) is not const so cannot be used in const fn
            features_and_scripts_bytes_per_gram: unsafe { NonZeroU64::new_unchecked(16) },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TransactionWeight(WeightParams);

impl TransactionWeight {
    /// Constructor
    pub fn new(weight_params: WeightParams) -> Self {
        Self(weight_params)
    }

    /// Creates a new `TransactionWeight` with latest weight params
    pub fn latest() -> Self {
        Self(WeightParams::v1())
    }

    /// Creates a new `TransactionWeight` with v1 weight params
    pub fn v1() -> Self {
        Self(WeightParams::v1())
    }

    /// Calculate the weight in grams of a transaction based on the number of kernels, inputs, outputs and rounded up
    /// features_and_scripts size. A warning to ensure that the _per output_ rounded up features_and_scripts size must
    /// be used or the calculation will be incorrect. If possible, use calculate_body instead to ensure correctness.
    pub fn calculate(
        &self,
        num_kernels: usize,
        num_inputs: usize,
        num_outputs: usize,
        rounded_up_features_and_scripts_byte_size: usize,
    ) -> u64 {
        let params = self.params();
        params
            .kernel_weight
            .saturating_mul(num_kernels as u64)
            .saturating_add(params.input_weight.saturating_mul(num_inputs as u64))
            .saturating_add(params.output_weight.saturating_mul(num_outputs as u64))
            .saturating_add(
                // `features_and_scripts_bytes_per_gram` is a `NonZero`, so this cannot divide by zero.
                (rounded_up_features_and_scripts_byte_size as u64)
                    .checked_div(params.features_and_scripts_bytes_per_gram.get())
                    .unwrap_or(0),
            )
    }

    pub fn calculate_body(&self, body: &AggregateBody) -> std::io::Result<u64> {
        let rounded_up_features_and_scripts_bytes_size =
            self.calculate_normalised_total_features_and_scripts_size(body)?;
        let output_count = body.outputs().len().saturating_sub(body.get_coinbase_outputs().len());
        // Coinbase kernels are not counted towards the weight.
        let coinbase_kernel_count = body.kernels().iter().filter(|k| k.is_coinbase()).count();
        let kernel_count = body.kernels().len().saturating_sub(coinbase_kernel_count);
        Ok(self.calculate(
            kernel_count,
            body.inputs().len(),
            output_count,
            rounded_up_features_and_scripts_bytes_size,
        ))
    }

    fn calculate_normalised_total_features_and_scripts_size(&self, body: &AggregateBody) -> std::io::Result<usize> {
        // When calculating the total block size vs each individual transaction the div operator in `calculate` above
        // will yield a different result due to integer rounding.
        // Where s_n is the features_and_scripts size for the nth output, p is per_gram
        // (∑s_i) / p != (s_1/p) + (s_2/p) +....(s_n / p)
        // We round up each output to the nearest p here to account for this

        Ok(body
            .outputs()
            .iter()
            .filter(|o| !o.is_coinbase())
            .map(|o| o.get_features_and_scripts_size())
            .collect::<Result<Vec<_>, _>>()?
            .iter()
            .map(
                |actual_size| // round up each output to nearest multiple of features_and_scripts_byte_per_gram
                self.round_up_features_and_scripts_size(*actual_size),
            )
            .sum())
    }

    pub fn round_up_features_and_scripts_size(&self, features_and_scripts_size: usize) -> usize {
        // EXPECT: consensus constant should not be set incorrectly
        let per_gram = usize::try_from(self.params().features_and_scripts_bytes_per_gram.get())
            .expect("features_and_scripts_bytes_per_gram exceeds usize::MAX");
        // `per_gram` is derived from a `NonZero`, so these cannot divide by zero.
        let rem = features_and_scripts_size.checked_rem(per_gram).unwrap_or(0);
        if rem == 0 {
            features_and_scripts_size
        } else {
            features_and_scripts_size
                .checked_add(per_gram.saturating_sub(rem))
                // The maximum rounded value possible is usize::MAX - usize::MAX % per_gram
                .unwrap_or_else(|| usize::MAX.saturating_sub(usize::MAX.checked_rem(per_gram).unwrap_or(0)))
        }
    }

    pub fn params(&self) -> &WeightParams {
        &self.0
    }
}

impl From<WeightParams> for TransactionWeight {
    fn from(params: WeightParams) -> Self {
        Self(params)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        key_manager::KeyManager,
        test_helpers::{self, TestParams},
        transaction_components::{
            KernelFeatures,
            OutputFeatures,
            RangeProofType,
            TransactionKernel,
            TransactionOutput,
        },
    };

    #[test]
    fn round_up_features_and_scripts_size() {
        let weighting = TransactionWeight::latest();
        let features_and_scripts_bytes_per_gram =
            usize::try_from(weighting.params().features_and_scripts_bytes_per_gram.get()).unwrap();
        assert_eq!(weighting.round_up_features_and_scripts_size(0), 0);
        assert_eq!(weighting.round_up_features_and_scripts_size(1), 16);
        assert_eq!(weighting.round_up_features_and_scripts_size(16), 16);
        assert_eq!(weighting.round_up_features_and_scripts_size(17), 32);
        if usize::MAX % features_and_scripts_bytes_per_gram == 0 {
            assert_eq!(weighting.round_up_features_and_scripts_size(usize::MAX), usize::MAX);
        } else {
            assert_eq!(
                weighting.round_up_features_and_scripts_size(usize::MAX) % features_and_scripts_bytes_per_gram,
                0
            );
        }
    }

    #[test]
    fn empty_body_weight() {
        let weighting = TransactionWeight::latest();
        let body = AggregateBody::empty();
        assert_eq!(weighting.calculate_body(&body).unwrap(), 0);
    }

    fn coinbase_output() -> TransactionOutput {
        TransactionOutput {
            features: OutputFeatures::create_coinbase(0, None, RangeProofType::RevealedValue),
            ..Default::default()
        }
    }

    fn kernel(features: KernelFeatures) -> TransactionKernel {
        test_helpers::create_test_kernel(0.into(), 0, features)
    }

    #[test]
    fn coinbase_output_without_coinbase_kernel_counts_every_kernel() {
        let weighting = TransactionWeight::latest();
        let body = AggregateBody::new_unsorted(vec![], vec![coinbase_output()], vec![kernel(KernelFeatures::empty())]);
        assert_eq!(
            weighting.calculate_body(&body).unwrap(),
            weighting.params().kernel_weight
        );
    }

    #[test]
    fn coinbase_kernel_is_left_out() {
        let weighting = TransactionWeight::latest();
        let kernels = vec![kernel(KernelFeatures::COINBASE_KERNEL), kernel(KernelFeatures::empty())];
        let body = AggregateBody::new_unsorted(vec![], vec![coinbase_output()], kernels.clone());
        assert_eq!(
            weighting.calculate_body(&body).unwrap(),
            weighting.params().kernel_weight
        );

        let body = AggregateBody::new_unsorted(vec![], vec![], kernels);
        assert_eq!(
            weighting.calculate_body(&body).unwrap(),
            weighting.params().kernel_weight
        );
    }

    #[test]
    fn every_normal_kernel_is_counted() {
        let weighting = TransactionWeight::latest();
        let kernels = (0..3).map(|_| kernel(KernelFeatures::empty())).collect();
        let body = AggregateBody::new_unsorted(vec![], vec![], kernels);
        assert_eq!(
            weighting.calculate_body(&body).unwrap(),
            3 * weighting.params().kernel_weight
        );
    }

    #[test]
    fn block_body_weight_matches_one_coinbase_kernel_discount() {
        let weighting = TransactionWeight::latest();
        let params = weighting.params();
        let key_manager = KeyManager::new_random().unwrap();
        let coinbase = test_helpers::create_coinbase_wallet_output(
            &TestParams::new(&key_manager),
            1,
            None,
            RangeProofType::RevealedValue,
            &key_manager,
        );
        let coinbase_kernel = test_helpers::create_coinbase_kernel(coinbase.commitment_mask_key_id(), &key_manager);
        let (tx, _, _) =
            test_helpers::create_tx(50000000.into(), 3.into(), 0, 2, 0, 2, Default::default(), &key_manager).unwrap();
        let mut body = tx.body;
        body.add_output(coinbase.to_transaction_output().unwrap());
        body.add_kernel(coinbase_kernel);

        let non_coinbase_outputs = body.outputs().iter().filter(|o| !o.is_coinbase()).collect::<Vec<_>>();
        let features_and_scripts = non_coinbase_outputs
            .iter()
            .map(|o| weighting.round_up_features_and_scripts_size(o.get_features_and_scripts_size().unwrap()) as u64)
            .sum::<u64>() /
            params.features_and_scripts_bytes_per_gram;
        let expected = (body.kernels().len() as u64 - 1) * params.kernel_weight +
            body.inputs().len() as u64 * params.input_weight +
            non_coinbase_outputs.len() as u64 * params.output_weight +
            features_and_scripts;
        assert_eq!(weighting.calculate_body(&body).unwrap(), expected);
    }
}
