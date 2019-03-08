// Copyright 2019 The Tari Project
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

use crate::{
    block::AggregateBody,
    transaction::{Transaction, TransactionError, TransactionInput, TransactionKernel, TransactionOutput},
    types::{Base, BlindingFactor},
};

pub struct TransactionBuilder {
    base: &'static Base,
    body: AggregateBody,
    offset: Option<BlindingFactor>,
}

impl TransactionBuilder {
    /// Create an new empty TransactionBuilder
    pub fn new(base: &'static Base) -> Self {
        Self { base, offset: None, body: AggregateBody::empty() }
    }

    /// Update the offset of an existing transaction
    pub fn add_offset(mut self, offset: BlindingFactor) -> Self {
        self.offset = Some(offset);
        self
    }

    /// Add an input to an existing transaction
    pub fn add_input(mut self, input: TransactionInput) -> Self {
        self.body = self.body.add_input(input);
        self
    }

    /// Add an output to an existing transaction
    pub fn add_output(mut self, output: TransactionOutput) -> Self {
        self.body = self.body.add_output(output);
        self
    }

    /// Add a series of inputs to an existing transaction
    pub fn add_inputs(mut self, inputs: Vec<TransactionInput>) -> Self {
        self.body = self.body.add_inputs(inputs);
        self
    }

    /// Add a series of outputs to an existing transaction
    pub fn add_outputs(mut self, outputs: Vec<TransactionOutput>) -> Self {
        self.body = self.body.add_outputs(outputs);
        self
    }

    /// Set the kernel of a transaction. Currently only one kernel is allowed per transaction
    pub fn with_kernel(mut self, kernel: TransactionKernel) -> Self {
        self.body = self.body.set_kernel(kernel);
        self
    }

    pub fn build(&self) -> Result<Transaction, TransactionError> {
        if let Some(offset) = self.offset {
            let tx = Transaction::new(
                self.base,
                self.body.inputs.clone(),
                self.body.outputs.clone(),
                self.body.kernels.clone(),
                offset,
            );
            tx.validate_kernel_sum()?;
            Ok(tx)
        } else {
            return Err(TransactionError::ValidationError);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        range_proof::RangeProof,
        transaction::{KernelFeatures, OutputFeatures, TransactionInput, TransactionKernel, TransactionOutput},
        types::Commitment,
    };
    use crypto::{
        commitment::HomomorphicCommitment,
        keys::SecretKeyFactory,
        ristretto::{pedersen::DEFAULT_RISTRETTO_PEDERSON_BASE, RistrettoSecretKey},
    };
    use curve25519_dalek::scalar::Scalar;
    use rand;

    #[test]
    fn build_transaction_test_kernel_sum_validation() {
        let mut rng = rand::OsRng::new().unwrap();
        let base = &DEFAULT_RISTRETTO_PEDERSON_BASE;

        let mut inputs = Vec::new();
        // Create an input
        inputs.push(TransactionInput::new(
            OutputFeatures::empty(),
            Commitment::new(&RistrettoSecretKey::random(&mut rng).into(), &Scalar::from(12u64), &base),
        ));
        // Create an output
        let mut outputs = Vec::new();
        outputs.push(TransactionOutput::new(
            OutputFeatures::empty(),
            Commitment::new(&RistrettoSecretKey::random(&mut rng).into(), &Scalar::from(12u64), &base),
            RangeProof([0; 1]),
        ));

        let sender_secret_key = RistrettoSecretKey::random(&mut rng);
        let offset: BlindingFactor = BlindingFactor::random(&mut rng).into();

        // Create a transaction
        let tx_builder = TransactionBuilder::new(&base)
            .add_inputs(inputs.clone())
            .add_outputs(outputs.clone())
            .add_offset(offset.clone());

        // Create a second input
        let input2 = TransactionInput::new(
            OutputFeatures::empty(),
            Commitment::new(&sender_secret_key.into(), &Scalar::from(10u64), &base),
        );

        // Create a second output
        let output2 = TransactionOutput::new(
            OutputFeatures::empty(),
            Commitment::new(&RistrettoSecretKey::random(&mut rng).into(), &Scalar::from(9u64), &base),
            RangeProof([0; 1]),
        );
        // Add the second input, output and kernel to the transaction using the builder methods
        let tx_builder = tx_builder.add_input(input2.clone()).add_output(output2.clone());

        // Will fail the validation because there is no kernel yet.
        let tx = tx_builder.build();
        assert!(tx.is_err());

        // Manually calculate the excess
        let mut manual_excess = &outputs[0].commitment + &output2.commitment;
        manual_excess = &manual_excess - &inputs[0].commitment;
        manual_excess = &manual_excess - &input2.commitment;
        manual_excess = &manual_excess + &Commitment::new(&Scalar::zero(), &Scalar::from(1u64), &base); // add fee
        manual_excess = &manual_excess - &Commitment::new(&offset.into(), &Scalar::zero(), &base); // Subtract Offset

        // Create a kernel with a fee (taken into account in the creation of the inputs and outputs
        let kernel = TransactionKernel {
            features: KernelFeatures::empty(),
            fee: 1,
            lock_height: 0,
            excess: Some(manual_excess),
            excess_sig: None,
        };

        let tx = tx_builder.with_kernel(kernel).build().unwrap();

        // Validate the transaction
        tx.validate_kernel_sum().unwrap();
    }
}
