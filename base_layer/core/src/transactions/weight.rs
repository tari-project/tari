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

use crate::transactions::aggregated_body::AggregateBody;

#[derive(Debug, Clone, Copy)]
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
            kernel_weight: 5,
            input_weight: 18,
            output_weight: 43,
            // SAFETY: the value isn't 0. NonZeroU64::new(x).expect(...) is not const so cannot be used in const fn
            features_and_scripts_bytes_per_gram: unsafe { NonZeroU64::new_unchecked(23) },
        }
    }
}

#[derive(Debug, Clone, Copy)]
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
        params.kernel_weight * num_kernels as u64 +
            params.input_weight * num_inputs as u64 +
            params.output_weight * num_outputs as u64 +
            rounded_up_features_and_scripts_byte_size as u64 / params.features_and_scripts_bytes_per_gram.get()
    }

    pub fn calculate_body(&self, body: &AggregateBody) -> std::io::Result<u64> {
        let rounded_up_features_and_scripts_bytes_size =
            self.calculate_normalised_total_features_and_scripts_size(body)?;
        let output_count = body.outputs().len().saturating_sub(body.get_coinbase_outputs().len());
        let kernel_count = if body.get_coinbase_outputs().is_empty() {
            // we dont count coinbase kernels, and there is only ever an allowed max of 1 coinbase kernel
            body.kernels().len()
        } else {
            body.kernels().len().saturating_sub(1)
        };
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
        let rem = features_and_scripts_size % per_gram;
        if rem == 0 {
            features_and_scripts_size
        } else {
            features_and_scripts_size
                .checked_add(per_gram - rem)
                // The maximum rounded value possible is usize::MAX - usize::MAX % per_gram
                .unwrap_or(usize::MAX - usize::MAX % per_gram)
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
    use tari_common::configuration::Network;

    use super::*;
    use crate::{
        consensus::ConsensusManager,
        transactions::transaction_components::{
            transaction_output::{MAX_OUTPUT_SIZE_NO_FEATURES},
            MAX_INPUT_SIZE_AVERAGE_STACK,
            MAX_INPUT_SIZE_LARGE_STACK,
            MAX_KERNEL_SIZE,
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

    // The purpose of this test is to ensure that the weight params are proportional to the size of the individual
    // components that makes up a transaction and ultimately the block size.
    #[test]
    fn weight_params_sanity_chack() {
        let weighting = TransactionWeight::latest();
        let weight_params = weighting.0;
        let esmeralda_max_weight = ConsensusManager::builder(Network::Esmeralda)
            .build()
            .unwrap()
            .consensus_constants(0)
            .max_block_transaction_weight();
        let nextnet_max_weight = ConsensusManager::builder(Network::NextNet)
            .build()
            .unwrap()
            .consensus_constants(0)
            .max_block_transaction_weight();
        let mainnet_max_weight = ConsensusManager::builder(Network::MainNet)
            .build()
            .unwrap()
            .consensus_constants(0)
            .max_block_transaction_weight();

        let output_ratio_bytes_per_gram = MAX_OUTPUT_SIZE_NO_FEATURES as f64 / weight_params.output_weight as f64;
        let input_ratio_bytes_per_gram = MAX_INPUT_SIZE_AVERAGE_STACK as f64 / weight_params.input_weight as f64;
        let kernel_ratio_bytes_per_gram = MAX_KERNEL_SIZE as f64 / weight_params.kernel_weight as f64;
        let features_ratio_bytes_per_gram = weight_params.features_and_scripts_bytes_per_gram.get() as f64;
        let average_ratio_bytes_per_gram = (output_ratio_bytes_per_gram +
            input_ratio_bytes_per_gram +
            kernel_ratio_bytes_per_gram +
            features_ratio_bytes_per_gram) /
            4.0;

        let adjusted_weight_params = WeightParams {
            kernel_weight: (MAX_KERNEL_SIZE as f64 / average_ratio_bytes_per_gram) as u64,
            input_weight: (MAX_INPUT_SIZE_LARGE_STACK as f64 / average_ratio_bytes_per_gram) as u64,
            output_weight: (MAX_OUTPUT_SIZE_NO_FEATURES as f64 / average_ratio_bytes_per_gram) as u64,
            features_and_scripts_bytes_per_gram: unsafe {
                NonZeroU64::new_unchecked(average_ratio_bytes_per_gram as u64)
            },
        };

        let adjusted_weighting = TransactionWeight(adjusted_weight_params);

        // Test case - block on esmeralda that could not be propagated via grpc:
        //  - weight 127770,
        //  - input(s): 6541,
        //  - output(s): 1126,
        //  - kernel(s): 563,
        //  - byte size: 5316326
        //  - average feature byte size: 144
        let inputs = 6541;
        let outputs = 1126;
        let kernels = 563;
        let average_feature_size = 144;

        let inputs_reduced = 4645;
        let outputs_reduced = 800;
        let kernels_reduced = 400;

        let size_1 = kernels * MAX_KERNEL_SIZE +
            inputs * MAX_INPUT_SIZE_AVERAGE_STACK +
            outputs * MAX_OUTPUT_SIZE_NO_FEATURES +
            outputs * average_feature_size;
        let size_2 = kernels * MAX_KERNEL_SIZE +
            inputs * MAX_INPUT_SIZE_LARGE_STACK +
            outputs * MAX_OUTPUT_SIZE_NO_FEATURES +
            outputs * average_feature_size;
        let weight = weighting.calculate(kernels, inputs, outputs, outputs * average_feature_size);
        let adjusted_weight = adjusted_weighting.calculate(kernels, inputs, outputs, outputs * average_feature_size);
        let reduced_weight = adjusted_weighting.calculate(kernels_reduced, inputs_reduced, outputs_reduced, outputs * average_feature_size);

        // output_ratio_bytes_per_gram:   23.26
        // input_ratio_bytes_per_gram:    19.72
        // kernel_ratio_bytes_per_gram:   26.40
        // features_ratio_bytes_per_gram: 23.00
        // average_ratio_bytes_per_gram:  23.09
        // weight_params:                 WeightParams { kernel_weight: 5, input_weight: 18, output_weight: 43,
        //                                               features_and_scripts_bytes_per_gram: 23 }
        // adjusted_weight_params:        WeightParams { kernel_weight: 5, input_weight: 18, output_weight: 43,
        //                                               features_and_scripts_bytes_per_gram: 23 }
        // average_feature_size:          144
        // weight:          176020, size_1: 3684515, size_2: 4207795
        // adjusted_weight: 176020, size_1: 3684515, size_2: 4207795
        // reduced_weight: 127059, size_1: 3684515, size_2: 4207795

        let test = true;
        if test {
            // We allow some margins away from the size proportionate weights, but not much as this will skew the block size
            assert!(
                (weight_params.input_weight.saturating_sub(1)..
                    weight_params.input_weight + 1).contains(&adjusted_weight_params.input_weight)
            );
            assert!(
                (weight_params.output_weight.saturating_sub(1)..
                    weight_params.output_weight + 1).contains(&adjusted_weight_params.output_weight)
            );
            assert!(
                (weight_params.kernel_weight.saturating_sub(1)..
                    weight_params.kernel_weight + 1).contains(&adjusted_weight_params.kernel_weight)
            );
            assert!(
                (weight_params
                    .features_and_scripts_bytes_per_gram
                    .get()
                    .saturating_sub(1)..
                    weight_params.features_and_scripts_bytes_per_gram.get() +
                        1).contains(&adjusted_weight_params.features_and_scripts_bytes_per_gram.get())
            );
            assert!((weight.saturating_sub(100)..weight + 100).contains(&adjusted_weight));
            assert!(reduced_weight < esmeralda_max_weight);
            assert!(reduced_weight < nextnet_max_weight);
            assert!(reduced_weight < mainnet_max_weight);
        } else {
            println!("output_ratio_bytes_per_gram:   {:.2}", output_ratio_bytes_per_gram);
            println!("input_ratio_bytes_per_gram:    {:.2}", input_ratio_bytes_per_gram);
            println!("kernel_ratio_bytes_per_gram:   {:.2}", kernel_ratio_bytes_per_gram);
            println!("features_ratio_bytes_per_gram: {:.2}", features_ratio_bytes_per_gram);
            println!("average_ratio_bytes_per_gram:  {:.2}", average_ratio_bytes_per_gram);
            println!("weight_params:                 {:?}", weight_params);
            println!("adjusted_weight_params:        {:?}", adjusted_weight_params);
            println!("average_feature_size:          {}", average_feature_size);
            println!("weight:          {}, size_1: {}, size_2: {}", weight, size_1, size_2);
            println!(
                "adjusted_weight: {}, size_1: {}, size_2: {}",
                adjusted_weight, size_1, size_2
            );
            println!(
                "reduced_weight:  {}, size_1: {}, size_2: {}",
                reduced_weight, size_1, size_2
            );
        }
    }
}
