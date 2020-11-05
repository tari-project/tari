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

use crate::transactions::{
    fee::Fee,
    tari_amount::MicroTari,
    transaction::{KernelBuilder, KernelFeatures, Transaction, TransactionKernel},
    transaction_protocol::{
        build_challenge,
        transaction_initializer::SenderTransactionInitializer,
        TransactionMetadata,
    },
    types::{Commitment, CommitmentFactory, CryptoFactories, PrivateKey, PublicKey, Signature},
    OutputBuilder,
    OutputFeatures,
    SenderTransactionProtocol,
    TransactionInput,
    UnblindedOutput,
};
use num::pow;
use rand::rngs::OsRng;
use std::sync::Arc;
use tari_crypto::{
    common::Blake256,
    keys::{PublicKey as PK, SecretKey},
};

pub fn make_input(val: MicroTari, factory: &CommitmentFactory) -> (TransactionInput, UnblindedOutput) {
    let output = OutputBuilder::new().with_value(val).build(factory).unwrap();
    let input = output.as_transaction_input();
    (input, output)
}

#[derive(Default)]
pub struct TestParams {
    pub spend_key: PrivateKey,
    pub change_key: PrivateKey,
    pub offset: PrivateKey,
    pub nonce: PrivateKey,
    pub public_nonce: PublicKey,
}

impl TestParams {
    pub fn new() -> TestParams {
        let r = PrivateKey::random(&mut OsRng);
        TestParams {
            spend_key: PrivateKey::random(&mut OsRng),
            change_key: PrivateKey::random(&mut OsRng),
            offset: PrivateKey::random(&mut OsRng),
            public_nonce: PublicKey::from_secret_key(&r),
            nonce: r,
        }
    }
}

/// A convenience struct for a set of public-private keys and a public-private nonce
pub struct TestKeySet {
    pub k: PrivateKey,
    pub pk: PublicKey,
    pub r: PrivateKey,
    pub pr: PublicKey,
}

/// Generate a new random key set. The key set includes
/// * a public-private keypair (k, pk)
/// * a public-private nonce keypair (r, pr)
pub fn generate_keys() -> TestKeySet {
    let (k, pk) = PublicKey::random_keypair(&mut OsRng);
    let (r, pr) = PublicKey::random_keypair(&mut OsRng);
    TestKeySet { k, pk, r, pr }
}

/// Generate a random transaction signature, returning the public key (excess) and the signature.
pub fn create_random_signature(fee: MicroTari, lock_height: u64) -> (PublicKey, Signature) {
    let (k, p) = PublicKey::random_keypair(&mut OsRng);
    (p, create_signature(k, fee, lock_height))
}

/// Generate a random transaction signature, returning the public key (excess) and the signature.
pub fn create_signature(k: PrivateKey, fee: MicroTari, lock_height: u64) -> Signature {
    let r = PrivateKey::random(&mut OsRng);
    let tx_meta = TransactionMetadata { fee, lock_height };
    let e = build_challenge(&PublicKey::from_secret_key(&r), &tx_meta);
    Signature::sign(k, r, &e).unwrap()
}

/// The sign macro is a convenience utility to generate a valid signature for a transaction. The most verbose version
/// of the macro requires an unblinded output, the nonce, and transaction metadata. If the caller omits any parameters
/// we try to use sane defaults (including a new, random nonce).
#[macro_export]
macro_rules! sign {
    ($output:expr, $metadata:expr, nonce: $nonce:expr, pub_nonce: $pub_nonce:expr) => {{
        use $crate::transactions::{transaction_protocol::build_challenge, types::Signature};
        let e = build_challenge(&$pub_nonce, &$metadata);
        Signature::sign($output.blinding_factor().clone(), $nonce, &e)
    }};

    ($output:expr, $metadata:expr, nonce: $nonce:expr) => {{
        use tari_crypto::keys::PublicKey as PK;
        use $crate::transactions::types::PublicKey;
        let p = PublicKey::from_secret_key(&$nonce);
        sign!($output, $metadata, nonce: $nonce, pub_nonce: p)
    }};

    ($output:expr, $metadata:expr) => {{
        use tari_crypto::keys::SecretKey;
        use $crate::transactions::types::PrivateKey;
        let nonce = PrivateKey::random(&mut rand::rngs::OsRng);
        sign!($output, $metadata, nonce: nonce)
    }};

    ($output:expr) => {{
        use $crate::transactions::transaction_protocol::TransactionMetadata;
        let meta = TransactionMetadata::default();
        sign!($output, meta)
    }};
}
/// Generate a random transaction signature given a key, returning the public key (excess) and the signature.
pub fn create_random_signature_from_s_key(
    s_key: PrivateKey,
    fee: MicroTari,
    lock_height: u64,
) -> (PublicKey, Signature)
{
    let _rng = rand::thread_rng();
    let r = PrivateKey::random(&mut OsRng);
    let p = PK::from_secret_key(&s_key);
    let tx_meta = TransactionMetadata { fee, lock_height };
    let e = build_challenge(&PublicKey::from_secret_key(&r), &tx_meta);
    (p, Signature::sign(s_key, r, &e).unwrap())
}

/// The tx macro is a convenience wrapper around the [create_tx] function, making the arguments optional and explicit
/// via keywords.
#[macro_export]
macro_rules! tx {
  ($amount:expr, fee: $fee:expr, lock: $lock:expr, inputs: $n_in:expr, maturity: $mat:expr, outputs: $n_out:expr) => {{
    use $crate::transactions::helpers::create_tx;
    create_tx($amount, $fee, $lock, $n_in, $mat, $n_out)
  }};

  ($amount:expr, fee: $fee:expr, lock: $lock:expr, inputs: $n_in:expr, outputs: $n_out:expr) => {
    tx!($amount, fee: $fee, lock: $lock, inputs: $n_in, maturity: 0, outputs: $n_out)
  };

  ($amount:expr, fee: $fee:expr, inputs: $n_in:expr, outputs: $n_out:expr) => {
    tx!($amount, fee: $fee, lock: 0, inputs: $n_in, maturity: 0, outputs: $n_out)
  };

  ($amount:expr, fee: $fee:expr) => {
    tx!($amount, fee: $fee, lock: 0, inputs: 1, maturity: 0, outputs: 2)
  }
}

/// A utility macro to help make it easy to build transactions.
///
/// The full syntax allows maximum flexibility, but most arguments are optional with sane defaults
/// ```ignore
///   txn_schema!(from: inputs, to: outputs, fee: 50*uT, lock: 1250, OutputFeatures::with_maturity(1320));
///   txn_schema!(from: inputs, to: outputs, fee: 50*uT); // Uses default features and zero lock height
///   txn_schema!(from: inputs, to: outputs); // min fee of 25µT, zero lock height and default features
///   // as above, and transaction splits the first input in roughly half, returning remainder as change
///   txn_schema!(from: inputs);
/// ```
/// The output of this macro is intended to be used in [spend_utxos].
#[macro_export]
macro_rules! txn_schema {
    (from: $input:expr, to: $outputs:expr, fee: $fee:expr, lock: $lock:expr, $features:expr) => {{
        $crate::transactions::helpers::TransactionSchema {
            from: $input.clone(),
            to: $outputs.clone(),
            fee: $fee,
            lock_height: $lock,
            features: $features
        }
    }};

    (from: $input:expr, to: $outputs:expr, fee: $fee:expr) => {
        txn_schema!(
            from: $input,
            to:$outputs,
            fee:$fee,
            lock:0,
            $crate::transactions::OutputFeatures::default()
        )
    };

    (from: $input:expr, to: $outputs:expr) => {
        txn_schema!(from: $input, to:$outputs, fee: 25.into())
    };

    // Spend inputs to ± half the first input value, with default fee and lock height
    (from: $input:expr) => {{
        let out_val = $input[0].value / 2u64;
        txn_schema!(from: $input, to: vec![out_val])
    }};
}

/// A convenience struct that holds plaintext versions of transactions
#[derive(Clone, Debug)]
pub struct TransactionSchema {
    pub from: Vec<UnblindedOutput>,
    pub to: Vec<MicroTari>,
    pub fee: MicroTari,
    pub lock_height: u64,
    pub features: OutputFeatures,
}

/// Create a random transaction input for the given amount and maturity period. The input and its unblinded
/// parameters are returned.
pub fn create_test_input(
    amount: MicroTari,
    maturity: u64,
    factory: &CommitmentFactory,
) -> (TransactionInput, UnblindedOutput)
{
    let spending_key = PrivateKey::random(&mut OsRng);
    let features = OutputFeatures::with_maturity(maturity);
    let unblinded_output = OutputBuilder::new()
        .with_value(amount)
        .with_spending_key(spending_key)
        .with_features(features)
        .build(factory)
        .unwrap();
    let input = unblinded_output.as_transaction_input();
    (input, unblinded_output)
}

/// Create an unconfirmed transaction for testing with a valid fee, unique access_sig, random inputs and outputs, the
/// transaction is only partially constructed
pub fn create_tx(
    amount: MicroTari,
    fee_per_gram: MicroTari,
    lock_height: u64,
    input_count: u64,
    input_maturity: u64,
    output_count: u64,
) -> (Transaction, Vec<UnblindedOutput>, Vec<UnblindedOutput>)
{
    let factories = CryptoFactories::default();
    let test_params = TestParams::new();
    let mut stx_builder: SenderTransactionInitializer = SenderTransactionProtocol::builder(0);
    stx_builder
        .with_lock_height(lock_height)
        .with_fee_per_gram(fee_per_gram)
        .with_offset(test_params.offset.clone())
        .with_private_nonce(test_params.nonce.clone())
        .with_change_secret(test_params.change_key.clone());

    let mut unblinded_inputs = Vec::with_capacity(input_count as usize);
    let mut unblinded_outputs = Vec::with_capacity(output_count as usize);
    let amount_per_input = amount / input_count;
    for _ in 0..input_count - 1 {
        let (utxo, input) = create_test_input(amount_per_input, input_maturity, &factories.commitment);
        unblinded_inputs.push(input.clone());
        stx_builder.with_input(utxo, input);
    }
    let amount_for_last_input = amount - amount_per_input * (input_count - 1);
    let (utxo, input) = create_test_input(amount_for_last_input, input_maturity, &factories.commitment);
    unblinded_inputs.push(input.clone());
    stx_builder.with_input(utxo, input);

    let estimated_fee = Fee::calculate(fee_per_gram, 1, input_count as usize, output_count as usize);
    let amount_per_output = (amount - estimated_fee) / output_count;
    let amount_for_last_output = (amount - estimated_fee) - amount_per_output * (output_count - 1);
    for i in 0..output_count {
        let output_amount = if i < output_count - 1 {
            amount_per_output
        } else {
            amount_for_last_output
        };
        let utxo = OutputBuilder::new()
            .with_value(output_amount)
            .with_spending_key(test_params.spend_key.clone())
            .build(&factories.commitment)
            .unwrap();
        unblinded_outputs.push(utxo.clone());
        stx_builder.with_output(utxo);
    }

    let mut stx_protocol = stx_builder.build::<Blake256>(&factories).unwrap();
    match stx_protocol.finalize(KernelFeatures::empty(), &factories) {
        Ok(_) => (),
        Err(e) => panic!("{:?}", e),
    }
    (
        stx_protocol.get_transaction().unwrap().clone(),
        unblinded_inputs,
        unblinded_outputs,
    )
}

/// Spend the provided UTXOs by to the given amounts. Change will be created with any outstanding amount.
/// You only need to provide the unblinded outputs to spend. This function will calculate the commitment for you.
/// This is obviously less efficient, but is offered as a convenience.
/// The output features will be applied to every output
pub fn spend_utxos(schema: TransactionSchema) -> (Transaction, Vec<UnblindedOutput>, TestParams) {
    let factories = CryptoFactories::default();
    let test_params = TestParams::new();
    let mut stx_builder = SenderTransactionProtocol::builder(0);
    stx_builder
        .with_lock_height(schema.lock_height)
        .with_fee_per_gram(schema.fee)
        .with_offset(test_params.offset.clone())
        .with_private_nonce(test_params.nonce.clone())
        .with_change_secret(test_params.change_key.clone());

    for input in &schema.from {
        let utxo = input.as_transaction_input();
        stx_builder.with_input(utxo, input.clone());
    }
    let mut outputs = Vec::with_capacity(schema.to.len());
    for val in schema.to {
        let utxo = OutputBuilder::new()
            .with_value(val)
            .with_features(schema.features.clone())
            .build(&factories.commitment)
            .unwrap();
        outputs.push(utxo.clone());
        stx_builder.with_output(utxo);
    }

    let mut stx_protocol = stx_builder.build::<Blake256>(&factories).unwrap();
    let change = stx_protocol.get_change_amount().unwrap();
    let change_output = OutputBuilder::new()
        .with_value(change)
        .with_spending_key(test_params.change_key.clone())
        .with_features(schema.features)
        .build(&factories.commitment)
        .unwrap();
    outputs.push(change_output);
    match stx_protocol.finalize(KernelFeatures::empty(), &factories) {
        Ok(_) => (),
        Err(e) => panic!("{:?}", e),
    }
    let txn = stx_protocol.get_transaction().unwrap().clone();
    (txn, outputs, test_params)
}

/// Create a transaction kernel with the given fee, using random keys to generate the signature
pub fn create_test_kernel(fee: MicroTari, lock_height: u64) -> TransactionKernel {
    let (excess, s) = create_random_signature(fee, lock_height);
    KernelBuilder::new()
        .with_fee(fee)
        .with_lock_height(lock_height)
        .with_excess(&Commitment::from_public_key(&excess))
        .with_signature(&s)
        .build()
        .unwrap()
}

/// Create a set of [Transaction]s and the associated UTXO data from the given transaction schema objects.
pub fn schema_to_transaction(txns: &[TransactionSchema]) -> (Vec<Arc<Transaction>>, Vec<UnblindedOutput>) {
    let mut tx = Vec::new();
    let mut utxos = Vec::new();
    txns.iter().for_each(|schema| {
        let (txn, mut output, _) = spend_utxos(schema.clone());
        tx.push(Arc::new(txn));
        utxos.append(&mut output);
    });
    (tx, utxos)
}

/// Return a currency styled `String`
/// # Examples
///
/// ```
/// use tari_core::transactions::helpers::display_currency;
/// assert_eq!(String::from("12,345.12"), display_currency(12345.12, 2, ","));
/// assert_eq!(String::from("12,345"), display_currency(12345.12, 0, ","));
/// ```
pub fn display_currency(value: f64, precision: usize, separator: &str) -> String {
    let whole = value as usize;
    let decimal = ((value - whole as f64) * pow(10_f64, precision)).round() as usize;
    let formatted_whole_value = whole
        .to_string()
        .chars()
        .rev()
        .enumerate()
        .fold(String::new(), |acc, (i, c)| {
            if i != 0 && i % 3 == 0 {
                format!("{}{}{}", acc, separator, c)
            } else {
                format!("{}{}", acc, c)
            }
        })
        .chars()
        .rev()
        .collect::<String>();

    if precision > 0 {
        format!("{}.{:0>2$}", formatted_whole_value, decimal, precision)
    } else {
        formatted_whole_value
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn display_currency() {
        assert_eq!(String::from("0.00"), super::display_currency(0.0f64, 2, ","));
        assert_eq!(String::from("0.000000000000"), super::display_currency(0.0f64, 12, ","));
        assert_eq!(
            String::from("123,456.123456789"),
            super::display_currency(123_456.123_456_789_f64, 9, ",")
        );
        assert_eq!(
            String::from("123,456"),
            super::display_currency(123_456.123_456_789_f64, 0, ",")
        );
        assert_eq!(String::from("1,234"), super::display_currency(1234.1f64, 0, ","));
    }
}
