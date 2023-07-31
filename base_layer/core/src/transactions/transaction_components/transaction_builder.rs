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

use tari_common_types::types::PrivateKey;

use crate::transactions::{
    aggregated_body::AggregateBody,
    tari_amount::MicroMinotari,
    transaction_components::{Transaction, TransactionError, TransactionInput, TransactionKernel, TransactionOutput},
};

//----------------------------------------  Transaction Builder   ----------------------------------------------------//
pub struct TransactionBuilder {
    body: AggregateBody,
    offset: Option<PrivateKey>,
    script_offset: Option<PrivateKey>,
    reward: Option<MicroMinotari>,
}

impl TransactionBuilder {
    /// Create an new empty TransactionBuilder
    pub fn new() -> Self {
        Self::default()
    }

    /// Update the offset of an existing transaction
    pub fn add_offset(&mut self, offset: PrivateKey) -> &mut Self {
        self.offset = Some(offset);
        self
    }

    /// Update the script offset of an existing transaction
    pub fn add_script_offset(&mut self, script_offset: PrivateKey) -> &mut Self {
        self.script_offset = Some(script_offset);
        self
    }

    /// Add an input to an existing transaction
    pub fn add_input(&mut self, input: TransactionInput) -> &mut Self {
        self.body.add_input(input);
        self
    }

    /// Add an output to an existing transaction
    pub fn add_output(&mut self, output: TransactionOutput) -> &mut Self {
        self.body.add_output(output);
        self
    }

    /// Moves a series of inputs to an existing transaction, leaving `inputs` empty
    pub fn add_inputs<I: IntoIterator<Item = TransactionInput>>(&mut self, inputs: I) -> &mut Self {
        self.body.add_inputs(inputs);
        self
    }

    /// Moves a series of outputs to an existing transaction, leaving `outputs` empty
    pub fn add_outputs<I: IntoIterator<Item = TransactionOutput>>(&mut self, outputs: I) -> &mut Self {
        self.body.add_outputs(outputs);
        self
    }

    /// Set the kernel of a transaction. Currently only one kernel is allowed per transaction
    pub fn with_kernel(&mut self, kernel: TransactionKernel) -> &mut Self {
        self.body.set_kernel(kernel);
        self
    }

    pub fn with_reward(&mut self, reward: MicroMinotari) -> &mut Self {
        self.reward = Some(reward);
        self
    }

    /// Build the transaction.
    pub fn build(self) -> Result<Transaction, TransactionError> {
        if let (Some(script_offset), Some(offset)) = (self.script_offset, self.offset) {
            let (i, o, k) = self.body.dissolve();
            let mut tx = Transaction::new(i, o, k, offset, script_offset);
            tx.body.sort();
            Ok(tx)
        } else {
            Err(TransactionError::ValidationError(
                "Transaction validation failed".into(),
            ))
        }
    }
}

impl Default for TransactionBuilder {
    fn default() -> Self {
        Self {
            offset: None,
            body: AggregateBody::empty(),
            reward: None,
            script_offset: None,
        }
    }
}
