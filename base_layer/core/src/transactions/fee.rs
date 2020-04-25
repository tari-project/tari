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

use crate::transactions::{tari_amount::*, transaction::MINIMUM_TRANSACTION_FEE};

pub struct Fee {}

pub const WEIGHT_PER_INPUT: u64 = 1;
pub const WEIGHT_PER_OUTPUT: u64 = 13;
pub const KERNEL_WEIGHT: u64 = 3; // Constant weight per transaction; covers kernel and part of header.

impl Fee {
    /// Computes the absolute transaction fee given the fee-per-gram, and the size of the transaction
    pub fn calculate(fee_per_gram: MicroTari, num_kernels: usize, num_inputs: usize, num_outputs: usize) -> MicroTari {
        (Fee::calculate_weight(num_kernels, num_inputs, num_outputs) * u64::from(fee_per_gram)).into()
    }

    /// Computes the absolute transaction fee using `calculate`, but the resulting fee will always be at least the
    /// minimum network transaction fee.
    pub fn calculate_with_minimum(
        fee_per_gram: MicroTari,
        num_kernels: usize,
        num_inputs: usize,
        num_outputs: usize,
    ) -> MicroTari
    {
        let fee = Fee::calculate(fee_per_gram, num_kernels, num_inputs, num_outputs);
        if fee < MINIMUM_TRANSACTION_FEE {
            MINIMUM_TRANSACTION_FEE
        } else {
            fee
        }
    }

    /// Calculate the weight of a transaction based on the number of inputs and outputs
    pub fn calculate_weight(num_kernels: usize, num_inputs: usize, num_outputs: usize) -> u64 {
        KERNEL_WEIGHT * num_kernels as u64 +
            WEIGHT_PER_INPUT * num_inputs as u64 +
            WEIGHT_PER_OUTPUT * num_outputs as u64
    }
}
