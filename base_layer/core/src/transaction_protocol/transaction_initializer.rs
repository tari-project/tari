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

use crate::{
    fee::Fee,
    transaction::{
        TransactionInput,
        TransactionOutput,
        UnblindedOutput,
        MAX_TRANSACTION_INPUTS,
        MINIMUM_TRANSACTION_FEE,
    },
    transaction_protocol::{
        receiver::RecipientInfo,
        sender::{calculate_tx_id, RawTransactionInfo, SenderState, SenderTransactionProtocol},
        TransactionMetadata,
    },
    types::{BlindingFactor, PublicKey, SecretKey},
};
use digest::Digest;
use std::{
    collections::HashMap,
    fmt::{Debug, Error, Formatter},
};
use tari_crypto::keys::PublicKey as PublicKeyTrait;
use tari_utilities::fixed_set::FixedSet;

/// The SenderTransactionInitializer is a Builder that helps set up the initial state for the Sender party of a new
/// transaction Typically you don't instantiate this object directly. Rather use
/// ```ignore
/// # use tari_core::SenderTransactionProtocol;
/// SenderTransactionProtocol::new(1);
/// ```
/// which returns an instance of this builder. Once all the sender's information has been added via the builder
/// methods, you can call `build()` which will return a
#[derive(Debug)]
pub struct SenderTransactionInitializer {
    num_recipients: usize,
    amounts: FixedSet<u64>,
    lock_height: Option<u64>,
    fee_per_gram: Option<u64>,
    inputs: Vec<TransactionInput>,
    unblinded_inputs: Vec<UnblindedOutput>,
    outputs: Vec<UnblindedOutput>,
    change_secret: Option<BlindingFactor>,
    offset: Option<BlindingFactor>,
    excess_blinding_factor: BlindingFactor,
    private_nonce: Option<SecretKey>,
}

pub struct BuildError {
    pub builder: SenderTransactionInitializer,
    pub message: String,
}

impl Debug for BuildError {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        f.write_str(&self.message)
    }
}

impl SenderTransactionInitializer {
    pub fn new(num_recipients: usize) -> Self {
        Self {
            num_recipients,
            amounts: FixedSet::new(num_recipients),
            lock_height: None,
            fee_per_gram: None,
            inputs: Vec::new(),
            unblinded_inputs: Vec::new(),
            outputs: Vec::new(),
            change_secret: None,
            offset: None,
            private_nonce: None,
            excess_blinding_factor: BlindingFactor::default(),
        }
    }

    /// Set the fee per weight for the transaction. See (Fee::calculate)[Struct.Fee.html#calculate] for how the
    /// absolute fee is calculated from the fee-per-gram value.
    pub fn with_fee_per_gram(&mut self, fee_per_gram: u64) -> &mut Self {
        self.fee_per_gram = Some(fee_per_gram);
        self
    }

    /// Set the amount to pay to the ith recipient. This method will silently fail if `receiver_index` >= num_receivers.
    pub fn with_amount(&mut self, receiver_index: usize, amount: u64) -> &mut Self {
        self.amounts.set_item(receiver_index, amount);
        self
    }

    /// Sets the minimum block height that this transaction will be mined.
    pub fn with_lock_height(&mut self, lock_height: u64) -> &mut Self {
        self.lock_height = Some(lock_height);
        self
    }

    /// Manually sets the offset value. If this is not called, a random offset will be used when `build()` is called.
    pub fn with_offset(&mut self, offset: BlindingFactor) -> &mut Self {
        self.offset = Some(offset);
        self
    }

    /// Adds an input to the transaction. The sender must provide the blinding factor that was used when the input
    /// was first set as an output. We don't check that the input and commitments match at this point.
    pub fn with_input(&mut self, utxo: TransactionInput, input: UnblindedOutput) -> &mut Self {
        self.inputs.push(utxo);
        self.excess_blinding_factor = &self.excess_blinding_factor - &input.spending_key;
        self.unblinded_inputs.push(input);
        self
    }

    /// Adds an output to the transaction. This can be called multiple times
    pub fn with_output(&mut self, output: UnblindedOutput) -> &mut Self {
        self.excess_blinding_factor = &self.excess_blinding_factor + &output.spending_key;
        self.outputs.push(output);
        self
    }

    /// Provide a blinding factor for the change output. The amount of change will automatically be calculated when
    /// the transaction is built.
    pub fn with_change_secret(&mut self, blinding_factor: BlindingFactor) -> &mut Self {
        self.change_secret = Some(blinding_factor);
        self
    }

    /// Provide the private nonce that will be used for the sender's partial signature for the transaction.
    pub fn with_private_nonce(&mut self, nonce: SecretKey) -> &mut Self {
        self.private_nonce = Some(nonce);
        self
    }

    /// Tries to make a change output with the given transaction parameters and add it to the set of outputs. The total
    /// fee, including the additional change output (if any) is returned
    fn add_change_if_required(&mut self) -> Result<u64, String> {
        // The number of outputs excluding a possible residual change output
        let num_outputs = self.outputs.len() + self.num_recipients;
        let num_inputs = self.inputs.len();
        let total_being_spent = self.unblinded_inputs.iter().map(|i| i.value).sum::<u64>();
        let total_to_self = self.outputs.iter().map(|o| o.value).sum::<u64>();

        let total_amount = self.amounts.sum().ok_or("Not all amounts have been provided")?;
        let fee_per_gram = self.fee_per_gram.ok_or("Fee per gram was not provided")?;
        let fee_without_change = Fee::calculate(fee_per_gram, num_inputs, num_outputs);
        let fee_with_change = Fee::calculate(fee_per_gram, num_inputs, num_outputs + 1);
        let extra_fee = fee_with_change - fee_without_change;
        // Subtract with a check on going negative
        let change_amount = total_being_spent.checked_sub(total_to_self + total_amount + fee_without_change);
        match change_amount {
            None => Err("You are spending more than you're providing".into()),
            Some(0) => Ok(fee_without_change),
            Some(v) => {
                let change_amount = v.checked_sub(extra_fee);
                match change_amount {
                    // You can't win. Just add the change to the fee (which is less than the cost of adding another
                    // output and go without a change output
                    None | Some(0) => Ok(fee_without_change + v),
                    Some(v) => {
                        let change_key = self
                            .change_secret
                            .as_ref()
                            .ok_or("Change spending key was not provided")?;
                        self.with_output(UnblindedOutput::new(v, change_key.clone(), None));
                        Ok(fee_with_change)
                    },
                }
            },
        }
    }

    fn check_value<T>(name: &str, val: &Option<T>, vec: &mut Vec<String>) {
        if val.is_none() {
            vec.push(name.to_string());
        }
    }

    fn build_err<T>(self, msg: &str) -> Result<T, BuildError> {
        Err(BuildError {
            builder: self,
            message: msg.to_string(),
        })
    }

    /// Construct a `SenderTransactionProtocol` instance in and appropriate state. The data stored
    /// in the struct is _moved_ into the new struct. If any data is missing, the `self` instance is returned in the
    /// error (so that you can continue building) along with a string listing the missing fields.
    /// If all the input data is present, but one or more fields are invalid, the function will return a
    /// `SenderTransactionProtocol` instance in the Failed state.
    pub fn build<D: Digest>(mut self) -> Result<SenderTransactionProtocol, BuildError> {
        // Compile a list of all data that is missing
        let mut message = Vec::new();
        Self::check_value("Missing Lock Height", &self.lock_height, &mut message);
        Self::check_value("Missing Fee per gram", &self.fee_per_gram, &mut message);
        Self::check_value("Missing Offset", &self.offset, &mut message);
        Self::check_value("Missing Private nonce", &self.private_nonce, &mut message);
        if !self.amounts.is_full() {
            message.push(format!("Missing all {} amounts", self.amounts.size()));
        }
        if self.inputs.len() < 1 {
            message.push("Missing Input".to_string());
        }
        // Prevent overflow attacks by imposing sane limits on some key parameters
        if self.inputs.len() > MAX_TRANSACTION_INPUTS {
            message.push("Too many inputs".into());
        }
        if message.len() > 0 {
            return self.build_err(&message.join(","));
        }
        // Everything is here. Let's send some Tari!
        // Calculate the fee based on whether we need to add a residual change output or not
        let total_fee = match self.add_change_if_required() {
            Ok(fee) => fee,
            Err(e) => return self.build_err(&e),
        };
        // Some checks on the fee
        if total_fee < MINIMUM_TRANSACTION_FEE {
            return self.build_err("Fee is less than the minimum");
        }
        let nonce = self.private_nonce.unwrap();
        let public_nonce = PublicKey::from_secret_key(&nonce);
        let offset = self.offset.unwrap();
        let excess_blinding_factor = self.excess_blinding_factor;
        let offset_blinding_factor = &excess_blinding_factor - &offset;
        let excess = PublicKey::from_secret_key(&offset_blinding_factor);
        let amount_to_self = self.outputs.iter().fold(0u64, |sum, o| sum + o.value);
        let outputs = self.outputs.iter().map(|o| TransactionOutput::from(o)).collect();
        let recipient_info = match self.num_recipients {
            0 => RecipientInfo::None,
            1 => RecipientInfo::Single(None),
            _ => RecipientInfo::Multiple(HashMap::new()),
        };
        let mut ids = Vec::with_capacity(self.num_recipients);
        for i in 0..self.num_recipients {
            ids.push(calculate_tx_id::<D>(&public_nonce, i));
        }
        let sender_info = RawTransactionInfo {
            num_recipients: self.num_recipients,
            amount_to_self,
            ids,
            amounts: self.amounts.into_vec(),
            metadata: TransactionMetadata {
                fee: total_fee,
                lock_height: self.lock_height.unwrap(),
            },
            inputs: self.inputs,
            outputs,
            offset,
            offset_blinding_factor,
            public_excess: excess,
            private_nonce: nonce,
            public_nonce: public_nonce.clone(),
            public_nonce_sum: public_nonce,
            recipient_info,
            signatures: Vec::new(),
        };
        let state = SenderState::Initializing(sender_info);
        let state = state
            .initialize()
            .expect("It should be possible to call initialize from Initializing state");
        Ok(SenderTransactionProtocol { state })
    }
}

//----------------------------------------         Tests          ----------------------------------------------------//

#[cfg(test)]
mod test {
    use crate::{
        fee::{Fee, BASE_COST, COST_PER_INPUT, COST_PER_OUTPUT},
        transaction::{UnblindedOutput, MAX_TRANSACTION_INPUTS},
        transaction_protocol::{
            sender::SenderState,
            test_common::{make_input, TestParams},
            transaction_initializer::SenderTransactionInitializer,
            TransactionProtocolError,
        },
    };
    use rand::OsRng;
    use tari_crypto::common::Blake256;

    /// One input, 2 outputs
    #[test]
    fn no_receivers() {
        // Create some inputs
        let mut rng = OsRng::new().unwrap();
        let p = TestParams::new(&mut rng);
        // Start the builder
        let builder = SenderTransactionInitializer::new(0);
        let err = builder.build::<Blake256>().unwrap_err();
        // We should have a bunch of fields missing still, but we can recover and continue
        assert_eq!(
            err.message,
            "Missing Lock Height,Missing Fee per gram,Missing Offset,Missing Private nonce,Missing Input"
        );
        let mut builder = err.builder;
        builder
            .with_lock_height(100)
            .with_offset(p.offset)
            .with_private_nonce(p.nonce);
        builder.with_output(UnblindedOutput::new(100, p.spend_key, None));
        let (utxo, input) = make_input(&mut rng, 500);
        builder.with_input(utxo, input);
        builder.with_fee_per_gram(20);
        let expected_fee = Fee::calculate(20, 1, 2);
        // We needed a change input, so this should fail
        let err = builder.build::<Blake256>().unwrap_err();
        assert_eq!(err.message, "Change spending key was not provided");
        // Ok, give them a change output
        let mut builder = err.builder;
        builder.with_change_secret(p.change_key.clone());
        let result = builder.build::<Blake256>().unwrap();
        // Peek inside and check the results
        if let SenderState::Finalizing(info) = result.state {
            assert_eq!(info.num_recipients, 0, "Number of receivers");
            assert_eq!(info.signatures.len(), 0, "Number of signatures");
            assert_eq!(info.ids.len(), 0, "Number of tx_ids");
            assert_eq!(info.amounts.len(), 0, "Number of external payment amounts");
            assert_eq!(info.metadata.lock_height, 100, "Lock height");
            assert_eq!(info.metadata.fee, expected_fee, "Fee");
            assert_eq!(info.outputs.len(), 2, "There should be 2 outputs");
            assert_eq!(info.inputs.len(), 1, "There should be 1 input");
        } else {
            panic!("There were no recipients, so we should be finalizing");
        }
    }

    /// One output, one input
    #[test]
    fn no_change_or_receivers() {
        // Create some inputs
        let mut rng = OsRng::new().unwrap();
        let p = TestParams::new(&mut rng);
        let (utxo, input) = make_input(&mut rng, 500);
        let expected_fee = Fee::calculate(20, 1, 1);
        let output = UnblindedOutput::new(500 - expected_fee, p.spend_key, None);
        // Start the builder
        let mut builder = SenderTransactionInitializer::new(0);
        builder
            .with_lock_height(0)
            .with_offset(p.offset)
            .with_private_nonce(p.nonce)
            .with_output(output)
            .with_input(utxo, input)
            .with_fee_per_gram(20);
        let result = builder.build::<Blake256>().unwrap();
        // Peek inside and check the results
        if let SenderState::Finalizing(info) = result.state {
            assert_eq!(info.num_recipients, 0, "Number of receivers");
            assert_eq!(info.signatures.len(), 0, "Number of signatures");
            assert_eq!(info.ids.len(), 0, "Number of tx_ids");
            assert_eq!(info.amounts.len(), 0, "Number of external payment amounts");
            assert_eq!(info.metadata.lock_height, 0, "Lock height");
            assert_eq!(info.metadata.fee, expected_fee, "Fee");
            assert_eq!(info.outputs.len(), 1, "There should be 1 output");
            assert_eq!(info.inputs.len(), 1, "There should be 1 input");
        } else {
            panic!("There were no recipients, so we should be finalizing");
        }
    }

    /// Hit the edge case where our change isn't enough to cover the cost of an extra output
    #[test]
    fn change_edge_case() {
        // Create some inputs
        let mut rng = OsRng::new().unwrap();
        let p = TestParams::new(&mut rng);
        let (utxo, input) = make_input(&mut rng, 500);
        let expected_fee = BASE_COST + (COST_PER_INPUT + 1 * COST_PER_OUTPUT) * 20; // 101, output = 80
                                                                                    // Pay out so that I should get change, but not enough to pay for the output
        let output = UnblindedOutput::new(500 - expected_fee - 50, p.spend_key, None);
        // Start the builder
        let mut builder = SenderTransactionInitializer::new(0);
        builder
            .with_lock_height(0)
            .with_offset(p.offset)
            .with_private_nonce(p.nonce)
            .with_output(output)
            .with_input(utxo, input)
            .with_fee_per_gram(20);
        let result = builder.build::<Blake256>().unwrap();
        // Peek inside and check the results
        if let SenderState::Finalizing(info) = result.state {
            assert_eq!(info.num_recipients, 0, "Number of receivers");
            assert_eq!(info.signatures.len(), 0, "Number of signatures");
            assert_eq!(info.ids.len(), 0, "Number of tx_ids");
            assert_eq!(info.amounts.len(), 0, "Number of external payment amounts");
            assert_eq!(info.metadata.lock_height, 0, "Lock height");
            assert_eq!(info.metadata.fee, expected_fee + 50, "Fee");
            assert_eq!(info.outputs.len(), 1, "There should be 1 output");
            assert_eq!(info.inputs.len(), 1, "There should be 1 input");
        } else {
            panic!("There were no recipients, so we should be finalizing");
        }
    }

    #[test]
    fn too_many_inputs() {
        // Create some inputs
        let mut rng = OsRng::new().unwrap();
        let p = TestParams::new(&mut rng);
        let output = UnblindedOutput::new(500, p.spend_key, None);
        // Start the builder
        let mut builder = SenderTransactionInitializer::new(0);
        builder
            .with_lock_height(0)
            .with_offset(p.offset)
            .with_private_nonce(p.nonce)
            .with_output(output)
            .with_fee_per_gram(2);
        for _ in 0..MAX_TRANSACTION_INPUTS + 1 {
            let (utxo, input) = make_input(&mut rng, 50);
            builder.with_input(utxo, input);
        }
        let err = builder.build::<Blake256>().unwrap_err();
        assert_eq!(err.message, "Too many inputs");
    }

    #[test]
    fn fee_too_low() {
        // Create some inputs
        let mut rng = OsRng::new().unwrap();
        let p = TestParams::new(&mut rng);
        let (utxo, input) = make_input(&mut rng, 500);
        let output = UnblindedOutput::new(400, p.spend_key, None);
        // Start the builder
        let mut builder = SenderTransactionInitializer::new(0);
        builder
            .with_lock_height(0)
            .with_offset(p.offset)
            .with_private_nonce(p.nonce)
            .with_input(utxo, input)
            .with_output(output)
            .with_change_secret(p.change_key)
            .with_fee_per_gram(1);
        let err = builder.build::<Blake256>().unwrap_err();
        assert_eq!(err.message, "Fee is less than the minimum");
    }

    #[test]
    fn not_enough_funds() {
        // Create some inputs
        let mut rng = OsRng::new().unwrap();
        let p = TestParams::new(&mut rng);
        let (utxo, input) = make_input(&mut rng, 400);
        let output = UnblindedOutput::new(400, p.spend_key, None);
        // Start the builder
        let mut builder = SenderTransactionInitializer::new(0);
        builder
            .with_lock_height(0)
            .with_offset(p.offset)
            .with_private_nonce(p.nonce)
            .with_input(utxo, input)
            .with_output(output)
            .with_change_secret(p.change_key)
            .with_fee_per_gram(1);
        let err = builder.build::<Blake256>().unwrap_err();
        assert_eq!(err.message, "You are spending more than you're providing");
    }

    #[test]
    fn multi_recipients() {
        // Create some inputs
        let mut rng = OsRng::new().unwrap();
        let p = TestParams::new(&mut rng);
        let (utxo, input) = make_input(&mut rng, 1000);
        let output = UnblindedOutput::new(150, p.spend_key, None);
        // Start the builder
        let mut builder = SenderTransactionInitializer::new(2);
        builder
            .with_lock_height(0)
            .with_offset(p.offset)
            .with_amount(0, 120)
            .with_amount(1, 110)
            .with_private_nonce(p.nonce)
            .with_input(utxo, input)
            .with_output(output)
            .with_change_secret(p.change_key)
            .with_fee_per_gram(20);
        let result = builder.build::<Blake256>().unwrap();
        // Peek inside and check the results
        if let SenderState::Failed(TransactionProtocolError::UnsupportedError(s)) = result.state {
            assert_eq!(s, "Multiple recipients are not supported yet")
        } else {
            panic!("We should not allow multiple recipients at this time");
        }
    }

    #[test]
    fn single_recipient() {
        // Create some inputs
        let mut rng = OsRng::new().unwrap();
        let p = TestParams::new(&mut rng);
        let (utxo1, input1) = make_input(&mut rng, 2000);
        let (utxo2, input2) = make_input(&mut rng, 3000);
        let weight = 30;
        let expected_fee = Fee::calculate(weight, 2, 3);
        let output = UnblindedOutput::new(1500 - expected_fee, p.spend_key, None);
        // Start the builder
        let mut builder = SenderTransactionInitializer::new(1);
        builder
            .with_lock_height(1234)
            .with_offset(p.offset)
            .with_private_nonce(p.nonce)
            .with_output(output)
            .with_input(utxo1, input1)
            .with_input(utxo2, input2)
            .with_amount(0, 2500)
            .with_change_secret(p.change_key)
            .with_fee_per_gram(weight);
        let result = builder.build::<Blake256>().unwrap();
        // Peek inside and check the results
        if let SenderState::SingleRoundMessageReady(info) = result.state {
            assert_eq!(info.num_recipients, 1, "Number of receivers");
            assert_eq!(info.signatures.len(), 0, "Number of signatures");
            assert_eq!(info.ids.len(), 1, "Number of tx_ids");
            assert_eq!(info.amounts.len(), 1, "Number of external payment amounts");
            assert_eq!(info.metadata.lock_height, 1234, "Lock height");
            assert_eq!(info.metadata.fee, expected_fee, "Fee");
            assert_eq!(info.outputs.len(), 2, "There should be 2 outputs");
            assert_eq!(info.inputs.len(), 2, "There should be 2 input");
        } else {
            panic!("There was a recipient, we should be ready to send a message");
        }
    }

}
