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

use std::num::NonZeroU64;

use crate::transactions::aggregated_body::AggregateBody;

#[derive(Debug, Clone, Copy)]
pub struct WeightParams {
    /// Weight in grams per kernel
    pub kernel_weight: u64,
    /// Weight in grams per input
    pub input_weight: u64,
    /// Weight in grams per output, excl. TariScript and OutputFeatures
    pub output_weight: u64,
    /// Metadata per byte weight
    pub metadata_bytes_per_gram: Option<NonZeroU64>,
}

impl WeightParams {
    pub const fn v1() -> Self {
        Self {
            kernel_weight: 3,
            input_weight: 1,
            output_weight: 13,
            metadata_bytes_per_gram: None,
        }
    }

    pub const fn v2() -> Self {
        Self {
            kernel_weight: 10, // adj. +2
            input_weight: 8,   // adj. -3
            output_weight: 53,
            // SAFETY: the value isn't 0. NonZeroU64::new(x).expect(...) is not const so cannot be used in const fn
            metadata_bytes_per_gram: Some(unsafe { NonZeroU64::new_unchecked(16) }),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TransactionWeight(WeightParams);

impl TransactionWeight {
    /// Creates a new `TransactionWeight` with latest weight params
    pub fn latest() -> Self {
        Self(WeightParams::v2())
    }

    /// Creates a new `TransactionWeight` with v1 weight params
    pub fn v1() -> Self {
        Self(WeightParams::v1())
    }

    /// Creates a new `TransactionWeight` with v2 weight params
    pub fn v2() -> Self {
        Self(WeightParams::v2())
    }

    /// Calculate the weight in grams of a transaction based on the number of kernels, inputs, outputs and rounded up
    /// metadata size. A warning to ensure that the _per output_ rounded up metadata size must be used or the
    /// calculation will be incorrect. If possible, use calculate_body instead to ensure correctness.
    pub fn calculate(
        &self,
        num_kernels: usize,
        num_inputs: usize,
        num_outputs: usize,
        rounded_up_metadata_byte_size: usize,
    ) -> u64 {
        let params = self.params();
        params.kernel_weight * num_kernels as u64 +
            params.input_weight * num_inputs as u64 +
            params.output_weight * num_outputs as u64 +
            params
                .metadata_bytes_per_gram
                .map(|per_gram| rounded_up_metadata_byte_size as u64 / per_gram.get())
                .unwrap_or(0)
    }

    pub fn calculate_body(&self, body: &AggregateBody) -> u64 {
        let rounded_up_metadata_size = self.calculate_normalised_total_metadata_size(body);
        self.calculate(
            body.kernels().len(),
            body.inputs().len(),
            body.outputs().len(),
            rounded_up_metadata_size,
        )
    }

    fn calculate_normalised_total_metadata_size(&self, body: &AggregateBody) -> usize {
        // When calculating the total block size vs each individual transaction the div operator in `calculate` above
        // will yield a different result due to integer rounding.
        // Where s_n is the metadata size for the nth output, p is per_gram
        // (∑s_i) / p != (s_1/p) + (s_2/p) +....(s_n / p)
        // We round up each output to the nearest p here to account for this

        body.outputs()
            .iter()
            .map(|o| {
                let actual_size = o.get_metadata_size();
                // round up each output to nearest multiple of metadata_byte_per_gram
                self.round_up_metadata_size(actual_size)
            })
            .sum()
    }

    pub fn round_up_metadata_size(&self, metadata_size: usize) -> usize {
        self.params()
            .metadata_bytes_per_gram
            .and_then(|per_gram| {
                let per_gram = per_gram.get() as usize;
                let rem = metadata_size % per_gram;
                if rem == 0 {
                    Some(metadata_size)
                } else {
                    metadata_size.checked_add(per_gram - rem)
                }
            })
            .unwrap_or(metadata_size)
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

    #[test]
    fn round_up_metadata_size() {
        let weighting = TransactionWeight::latest();
        assert_eq!(weighting.round_up_metadata_size(0), 0);
        assert_eq!(weighting.round_up_metadata_size(1), 16);
        assert_eq!(weighting.round_up_metadata_size(16), 16);
        assert_eq!(weighting.round_up_metadata_size(17), 32);
        assert_eq!(weighting.round_up_metadata_size(usize::MAX), usize::MAX);
    }
}
