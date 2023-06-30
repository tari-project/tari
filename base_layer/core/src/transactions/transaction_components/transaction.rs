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
    fmt::{Display, Formatter},
    ops::Add,
};

use serde::{Deserialize, Serialize};
use tari_common_types::types::{PrivateKey, Signature};
use tari_utilities::hex::Hex;

use crate::transactions::{
    aggregated_body::AggregateBody,
    transaction_components::{TransactionError, TransactionInput, TransactionKernel, TransactionOutput},
    weight::TransactionWeight,
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
    pub offset: PrivateKey,
    /// The constituents of a transaction which has the same structure as the body of a block.
    pub body: AggregateBody,
    /// A scalar offset that links outputs and inputs to prevent cut-through, enforcing the correct application of
    /// the output script.
    pub script_offset: PrivateKey,
}

impl Transaction {
    /// Create a new transaction from the provided inputs, outputs, kernels and offset
    pub fn new(
        inputs: Vec<TransactionInput>,
        outputs: Vec<TransactionOutput>,
        kernels: Vec<TransactionKernel>,
        offset: PrivateKey,
        script_offset: PrivateKey,
    ) -> Self {
        Self {
            offset,
            body: AggregateBody::new(inputs, outputs, kernels),
            script_offset,
        }
    }

    pub fn body(&self) -> &AggregateBody {
        &self.body
    }

    /// Returns the byte size or weight of a transaction
    pub fn calculate_weight(&self, transaction_weight: &TransactionWeight) -> std::io::Result<u64> {
        self.body.calculate_weight(transaction_weight)
    }

    /// Returns the maximum maturity of the input UTXOs
    pub fn max_input_maturity(&self) -> Result<u64, TransactionError> {
        self.body.max_input_maturity()
    }

    /// Returns the maximum time lock of the kernels inside of the transaction
    pub fn max_kernel_timelock(&self) -> u64 {
        self.body.max_kernel_timelock()
    }

    /// Returns the height of the minimum height where the transaction is spendable. This is calculated from the
    /// transaction kernel lock_heights and the maturity of the input UTXOs.
    pub fn min_spendable_height(&self) -> Result<u64, TransactionError> {
        self.body.min_spendable_height()
    }

    pub fn first_kernel_excess_sig(&self) -> Option<&Signature> {
        Some(&self.body.kernels().first()?.excess_sig)
    }
}

impl Add for Transaction {
    type Output = Self;

    /// This function adds two transactions together by summing up the offset, script offset and
    /// extending inputs, outputs and kernels.
    fn add(mut self, other: Self) -> Self {
        self.offset = self.offset + other.offset;
        self.script_offset = self.script_offset + other.script_offset;
        let (inputs, outputs, kernels) = other.body.dissolve();
        self.body.add_inputs(inputs);
        self.body.add_outputs(outputs);
        self.body.add_kernels(kernels);
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
