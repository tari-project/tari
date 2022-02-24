// Copyright 2018 The Tari Project
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
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE
//
// Portions of this file were originally copyrighted (c) 2018 The Grin Developers, issued under the Apache License,
// Version 2.0, available at http://www.apache.org/licenses/LICENSE-2.0.

use std::{
    cmp::{max, min},
    fmt::{Display, Formatter},
    ops::Add,
};

use serde::{Deserialize, Serialize};
use tari_common_types::types::{BlindingFactor, HashOutput, Signature};
use tari_crypto::tari_utilities::hex::Hex;

use crate::transactions::{
    aggregated_body::AggregateBody,
    tari_amount::{uT, MicroTari},
    transaction_components::{
        OutputFeatures,
        TransactionError,
        TransactionInput,
        TransactionKernel,
        TransactionOutput,
    },
    weight::TransactionWeight,
    CryptoFactories,
};

/// A transaction which consists of a kernel offset and an aggregate body made up of inputs, outputs and kernels.
/// This struct is used to describe single transactions only. The common part between transactions and Tari blocks is
/// accessible via the `body` field, but single transactions also need to carry the public offset around with them so
/// that these can be aggregated into block offsets.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transaction {
    /// This kernel offset will be accumulated when transactions are aggregated to prevent the "subset" problem where
    /// kernels can be linked to inputs and outputs by testing a series of subsets and see which produce valid
    /// transactions.
    pub offset: BlindingFactor,
    /// The constituents of a transaction which has the same structure as the body of a block.
    pub body: AggregateBody,
    /// A scalar offset that links outputs and inputs to prevent cut-through, enforcing the correct application of
    /// the output script.
    pub script_offset: BlindingFactor,
}

impl Transaction {
    /// Create a new transaction from the provided inputs, outputs, kernels and offset
    pub fn new(
        inputs: Vec<TransactionInput>,
        outputs: Vec<TransactionOutput>,
        kernels: Vec<TransactionKernel>,
        offset: BlindingFactor,
        script_offset: BlindingFactor,
    ) -> Self {
        Self {
            offset,
            body: AggregateBody::new(inputs, outputs, kernels),
            script_offset,
        }
    }

    /// Validate this transaction by checking the following:
    /// 1. The sum of inputs, outputs and fees equal the (public excess value + offset)
    /// 1. The signature signs the canonical message with the private excess
    /// 1. Range proofs of the outputs are valid
    ///
    /// This function does NOT check that inputs come from the UTXO set
    #[allow(clippy::erasing_op)] // This is for 0 * uT
    pub fn validate_internal_consistency(
        &self,
        bypass_range_proof_verification: bool,
        factories: &CryptoFactories,
        reward: Option<MicroTari>,
        prev_header: Option<HashOutput>,
        height: u64,
    ) -> Result<(), TransactionError> {
        let reward = reward.unwrap_or_else(|| 0 * uT);
        self.body.validate_internal_consistency(
            &self.offset,
            &self.script_offset,
            bypass_range_proof_verification,
            reward,
            factories,
            prev_header,
            height,
        )
    }

    pub fn body(&self) -> &AggregateBody {
        &self.body
    }

    /// Returns the byte size or weight of a transaction
    pub fn calculate_weight(&self, transaction_weight: &TransactionWeight) -> u64 {
        self.body.calculate_weight(transaction_weight)
    }

    /// Returns the minimum maturity of the input UTXOs
    pub fn min_input_maturity(&self) -> u64 {
        self.body.inputs().iter().fold(u64::MAX, |min_maturity, input| {
            min(
                min_maturity,
                input
                    .features()
                    .unwrap_or(&OutputFeatures::with_maturity(std::u64::MAX))
                    .maturity,
            )
        })
    }

    /// Returns the maximum maturity of the input UTXOs
    pub fn max_input_maturity(&self) -> u64 {
        self.body.inputs().iter().fold(0, |max_maturity, input| {
            max(
                max_maturity,
                input.features().unwrap_or(&OutputFeatures::with_maturity(0)).maturity,
            )
        })
    }

    /// Returns the maximum time lock of the kernels inside of the transaction
    pub fn max_kernel_timelock(&self) -> u64 {
        self.body.max_kernel_timelock()
    }

    /// Returns the height of the minimum height where the transaction is spendable. This is calculated from the
    /// transaction kernel lock_heights and the maturity of the input UTXOs.
    pub fn min_spendable_height(&self) -> u64 {
        max(self.max_kernel_timelock(), self.max_input_maturity())
    }

    /// This function adds two transactions together. It does not do cut-through. Calling Tx1 + Tx2 will result in
    /// vut-through being applied.
    pub fn add_no_cut_through(mut self, other: Self) -> Self {
        self.offset = self.offset + other.offset;
        self.script_offset = self.script_offset + other.script_offset;
        let (mut inputs, mut outputs, mut kernels) = other.body.dissolve();
        self.body.add_inputs(&mut inputs);
        self.body.add_outputs(&mut outputs);
        self.body.add_kernels(&mut kernels);
        self
    }

    pub fn first_kernel_excess_sig(&self) -> Option<&Signature> {
        Some(&self.body.kernels().first()?.excess_sig)
    }
}

impl Add for Transaction {
    type Output = Self;

    fn add(mut self, other: Self) -> Self {
        self = self.add_no_cut_through(other);
        self
    }
}

impl Display for Transaction {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        writeln!(fmt, "-------------- Transaction --------------")?;
        writeln!(fmt, "--- Offset ---")?;
        writeln!(fmt, "{}", self.offset.to_hex())?;
        writeln!(fmt, "--- Script Offset ---")?;
        writeln!(fmt, "{}", self.script_offset.to_hex())?;
        writeln!(fmt, "---  Body  ---")?;
        writeln!(fmt, "{}", self.body)
    }
}
