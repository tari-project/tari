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
    transaction::{
        KernelBuilder,
        KernelFeatures,
        OutputFeatures,
        Transaction,
        TransactionInput,
        TransactionKernel,
        TransactionOutput,
        UnblindedOutput,
    },
    transaction_protocol::{build_challenge, TransactionMetadata},
    types::{Commitment, CommitmentFactory, CryptoFactories, PrivateKey, PublicKey, Signature},
    SenderTransactionProtocol,
};
use num::pow;
use rand::rngs::OsRng;
use std::sync::Arc;
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    common::Blake256,
    inputs,
    keys::{PublicKey as PK, SecretKey},
    range_proof::RangeProofService,
    script,
    script::{ExecutionStack, TariScript},
};

pub fn create_test_input(
    amount: MicroTari,
    maturity: u64,
    factory: &CommitmentFactory,
) -> (TransactionInput, UnblindedOutput) {
    let mut params = TestParams::new();
    params.commitment_factory = factory.clone();
    params.create_input(UtxoTestParams {
        value: amount,
        output_features: OutputFeatures::with_maturity(maturity),
        ..Default::default()
    })
}

#[derive(Default, Clone)]
pub struct TestParams {
    pub spend_key: PrivateKey,
    pub change_spend_key: PrivateKey,
    pub offset: PrivateKey,
    pub nonce: PrivateKey,
    pub public_nonce: PublicKey,
    pub script_private_key: PrivateKey,
    pub sender_offset_public_key: PublicKey,
    pub sender_offset_private_key: PrivateKey,
    pub sender_sig_private_nonce: PrivateKey,
    pub sender_sig_public_nonce: PublicKey,
    pub sender_private_commitment_nonce: PrivateKey,
    pub sender_public_commitment_nonce: PublicKey,
    pub commitment_factory: CommitmentFactory,
}

#[derive(Clone)]
pub struct UtxoTestParams {
    pub value: MicroTari,
    pub script: TariScript,
    pub output_features: OutputFeatures,
    pub input_data: Option<ExecutionStack>,
}

impl Default for UtxoTestParams {
    fn default() -> Self {
        Self {
            value: 10.into(),
            script: script![Nop],
            output_features: Default::default(),
            input_data: None,
        }
    }
}

impl TestParams {
    pub fn new() -> TestParams {
        let r = PrivateKey::random(&mut OsRng);
        let sender_offset_private_key = PrivateKey::random(&mut OsRng);
        let sender_sig_pvt_nonce = PrivateKey::random(&mut OsRng);
        let script_private_key = PrivateKey::random(&mut OsRng);
        TestParams {
            spend_key: PrivateKey::random(&mut OsRng),
            change_spend_key: PrivateKey::random(&mut OsRng),
            offset: PrivateKey::random(&mut OsRng),
            public_nonce: PublicKey::from_secret_key(&r),
            nonce: r,
            script_private_key,
            sender_offset_public_key: PublicKey::from_secret_key(&sender_offset_private_key),
            sender_offset_private_key,
            sender_sig_private_nonce: sender_sig_pvt_nonce.clone(),
            sender_sig_public_nonce: PublicKey::from_secret_key(&sender_sig_pvt_nonce),
            sender_private_commitment_nonce: sender_sig_pvt_nonce.clone(),
            sender_public_commitment_nonce: PublicKey::from_secret_key(&sender_sig_pvt_nonce),
            commitment_factory: CommitmentFactory::default(),
        }
    }

    pub fn create_unblinded_output(&self, params: UtxoTestParams) -> UnblindedOutput {
        let metadata_signature = TransactionOutput::create_final_metadata_signature(
            &params.value,
            &self.spend_key,
            &params.script,
            &params.output_features,
            &self.sender_offset_private_key,
        )
        .unwrap();

        UnblindedOutput::new(
            params.value,
            self.spend_key.clone(),
            Some(params.output_features.clone()),
            params.script.clone(),
            params
                .input_data
                .unwrap_or_else(|| inputs!(self.get_script_public_key())),
            self.script_private_key.clone(),
            self.sender_offset_public_key.clone(),
            metadata_signature,
        )
    }

    pub fn get_script_public_key(&self) -> PublicKey {
        PublicKey::from_secret_key(&self.script_private_key)
    }

    pub fn get_script_keypair(&self) -> (PrivateKey, PublicKey) {
        (self.script_private_key.clone(), self.get_script_public_key())
    }

    /// Create a random transaction input for the given amount and maturity period. The input ,its unblinded
    /// parameters are returned.
    pub fn create_input(&self, params: UtxoTestParams) -> (TransactionInput, UnblindedOutput) {
        let unblinded = self.create_unblinded_output(params);
        let input = unblinded.as_transaction_input(&self.commitment_factory).unwrap();
        (input, unblinded)
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
    let _rng = rand::thread_rng();
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

/// Generate a random transaction signature given a key, returning the public key (excess) and the signature.
pub fn create_random_signature_from_s_key(
    s_key: PrivateKey,
    fee: MicroTari,
    lock_height: u64,
) -> (PublicKey, Signature) {
    let _rng = rand::thread_rng();
    let r = PrivateKey::random(&mut OsRng);
    let p = PK::from_secret_key(&s_key);
    let tx_meta = TransactionMetadata { fee, lock_height };
    let e = build_challenge(&PublicKey::from_secret_key(&r), &tx_meta);
    (p, Signature::sign(s_key, r, &e).unwrap())
}

pub fn create_unblinded_output(
    script: TariScript,
    output_features: OutputFeatures,
    test_params: TestParams,
    value: MicroTari,
) -> UnblindedOutput {
    test_params.create_unblinded_output(UtxoTestParams {
        value,
        script,
        output_features,
        ..Default::default()
    })
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
    (from: $input:expr, to: $outputs:expr, fee: $fee:expr, lock: $lock:expr, features: $features:expr) => {{
        $crate::transactions::helpers::TransactionSchema {
            from: $input.clone(),
            to: $outputs.clone(),
            fee: $fee,
            lock_height: $lock,
            features: $features,
            script: tari_crypto::script![Nop],
            input_data: None,
        }
    }};

    (from: $input:expr, to: $outputs:expr, fee: $fee:expr, lock: $lock:expr, features: $features:expr) => {{
        txn_schema!(
            from: $input,
            to:$outputs,
            fee:$fee,
            lock:$lock,
            features: $features
        )
    }};

    (from: $input:expr, to: $outputs:expr, fee: $fee:expr) => {
        txn_schema!(
            from: $input,
            to:$outputs,
            fee:$fee,
            lock:0,
            features: $crate::transactions::transaction::OutputFeatures::default()
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
    pub script: TariScript,
    pub input_data: Option<ExecutionStack>,
}

/// Create an unconfirmed transaction for testing with a valid fee, unique access_sig, random inputs and outputs, the
/// transaction is only partially constructed
pub fn create_tx(
    amount: MicroTari,
    fee_per_gram: MicroTari,
    lock_height: u64,
    input_count: usize,
    input_maturity: u64,
    output_count: usize,
) -> (Transaction, Vec<UnblindedOutput>, Vec<UnblindedOutput>) {
    let (inputs, outputs) = create_unblinded_txos(amount, input_count, input_maturity, output_count, fee_per_gram);
    let tx = create_transaction_with(lock_height, fee_per_gram, inputs.clone(), outputs.clone());
    (tx, inputs, outputs.into_iter().map(|(utxo, _)| utxo).collect())
}

pub fn create_unblinded_txos(
    amount: MicroTari,
    input_count: usize,
    input_maturity: u64,
    output_count: usize,
    fee_per_gram: MicroTari,
) -> (Vec<UnblindedOutput>, Vec<(UnblindedOutput, PrivateKey)>) {
    let estimated_fee = Fee::calculate(fee_per_gram, 1, input_count, output_count);
    let amount_per_output = (amount - estimated_fee) / output_count as u64;
    let amount_for_last_output = (amount - estimated_fee) - amount_per_output * (output_count as u64 - 1);

    let outputs = (0..output_count)
        .map(|i| {
            let output_amount = if i < output_count - 1 {
                amount_per_output
            } else {
                amount_for_last_output
            };
            let test_params = TestParams::new();
            let script_offset_pvt_key = test_params.sender_offset_private_key.clone();

            (
                test_params.create_unblinded_output(UtxoTestParams {
                    value: output_amount,
                    ..Default::default()
                }),
                script_offset_pvt_key,
            )
        })
        .collect();

    let amount_per_input = amount / input_count as u64;
    let inputs = (0..input_count)
        .map(|i| {
            let mut params = UtxoTestParams {
                output_features: OutputFeatures::with_maturity(input_maturity),
                ..Default::default()
            };
            if i == input_count - 1 {
                params.value = amount - amount_per_input * (input_count as u64 - 1);
            } else {
                params.value = amount_per_input;
            }

            let (_, unblinded) = TestParams::new().create_input(params);
            unblinded
        })
        .collect();

    (inputs, outputs)
}

/// Create an unconfirmed transaction for testing with a valid fee, unique access_sig, random inputs and outputs, the
/// transaction is only partially constructed
pub fn create_transaction_with(
    lock_height: u64,
    fee_per_gram: MicroTari,
    inputs: Vec<UnblindedOutput>,
    outputs: Vec<(UnblindedOutput, PrivateKey)>,
) -> Transaction {
    let factories = CryptoFactories::default();
    let test_params = TestParams::new();
    let mut stx_builder = SenderTransactionProtocol::builder(0);
    stx_builder
        .with_lock_height(lock_height)
        .with_fee_per_gram(fee_per_gram)
        .with_offset(test_params.offset.clone())
        .with_private_nonce(test_params.nonce.clone())
        .with_change_secret(test_params.change_spend_key);

    inputs.into_iter().for_each(|input| {
        stx_builder.with_input(
            input.as_transaction_input(&CommitmentFactory::default()).unwrap(),
            input,
        );
    });

    outputs.into_iter().for_each(|(utxo, script_offset_pvt_key)| {
        stx_builder.with_output(utxo, script_offset_pvt_key).unwrap();
    });

    let mut stx_protocol = stx_builder.build::<Blake256>(&factories).unwrap();
    stx_protocol.finalize(KernelFeatures::empty(), &factories).unwrap();
    stx_protocol.take_transaction().unwrap()
}

/// Spend the provided UTXOs by to the given amounts. Change will be created with any outstanding amount.
/// You only need to provide the unblinded outputs to spend. This function will calculate the commitment for you.
/// This is obviously less efficient, but is offered as a convenience.
/// The output features will be applied to every output
pub fn spend_utxos(schema: TransactionSchema) -> (Transaction, Vec<UnblindedOutput>, TestParams) {
    let factories = CryptoFactories::default();
    let test_params_change_and_txn = TestParams::new();
    let mut stx_builder = SenderTransactionProtocol::builder(0);
    stx_builder
        .with_lock_height(schema.lock_height)
        .with_fee_per_gram(schema.fee)
        .with_offset(test_params_change_and_txn.offset.clone())
        .with_private_nonce(test_params_change_and_txn.nonce.clone())
        .with_change_secret(test_params_change_and_txn.change_spend_key.clone())
        .with_change_script(
            script!(Nop),
            inputs!(PublicKey::from_secret_key(
                &test_params_change_and_txn.script_private_key
            )),
            test_params_change_and_txn.script_private_key.clone(),
        );

    for tx_input in &schema.from {
        let input = tx_input.clone();
        let utxo = input
            .as_transaction_input(&factories.commitment)
            .expect("Should be able to make a transaction input");
        stx_builder.with_input(utxo, input.clone());
    }
    let mut outputs = Vec::with_capacity(schema.to.len());
    for val in schema.to {
        let test_params = TestParams::new();
        let utxo = test_params.create_unblinded_output(UtxoTestParams {
            value: val,
            output_features: schema.features.clone(),
            script: schema.script.clone(),
            input_data: schema.input_data.clone(),
        });
        outputs.push(utxo.clone());
        stx_builder
            .with_output(utxo, test_params.sender_offset_private_key.clone())
            .unwrap();
    }

    let mut stx_protocol = stx_builder.build::<Blake256>(&factories).unwrap();
    let change = stx_protocol.get_change_amount().unwrap();
    // The change output is assigned its own random script offset private key
    let change_sender_offset_public_key = stx_protocol.get_change_sender_offset_public_key().unwrap().unwrap();

    let script = script!(Nop);
    let metadata_sig = TransactionOutput::create_final_metadata_signature(
        &change,
        &test_params_change_and_txn.change_spend_key,
        &script,
        &schema.features,
        &test_params_change_and_txn.sender_offset_private_key,
    )
    .unwrap();

    let change_output = UnblindedOutput::new(
        change,
        test_params_change_and_txn.change_spend_key.clone(),
        Some(schema.features),
        script,
        inputs!(PublicKey::from_secret_key(
            &test_params_change_and_txn.script_private_key
        )),
        test_params_change_and_txn.script_private_key.clone(),
        change_sender_offset_public_key,
        metadata_sig,
    );
    outputs.push(change_output);
    match stx_protocol.finalize(KernelFeatures::empty(), &factories) {
        Ok(_) => (),
        Err(e) => panic!("{:?}", e),
    }
    let txn = stx_protocol.get_transaction().unwrap().clone();
    (txn, outputs, test_params_change_and_txn)
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

/// Create a new UTXO for the specified value and return the output and spending key
pub fn create_utxo(
    value: MicroTari,
    factories: &CryptoFactories,
    features: Option<OutputFeatures>,
    script: &TariScript,
) -> (TransactionOutput, PrivateKey, PrivateKey) {
    let keys = generate_keys();
    let offset_keys = generate_keys();
    let features = features.unwrap_or_default();
    let commitment = factories.commitment.commit_value(&keys.k, value.into());
    let proof = factories.range_proof.construct_proof(&keys.k, value.into()).unwrap();
    let metadata_sig =
        TransactionOutput::create_final_metadata_signature(&value, &keys.k, &script, &features, &offset_keys.k)
            .unwrap();

    let utxo = TransactionOutput::new(
        features,
        commitment,
        proof.into(),
        script.clone(),
        offset_keys.pk,
        metadata_sig,
    );
    (utxo, keys.k, offset_keys.k)
}

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
#[allow(clippy::excessive_precision)]
mod test {
    #[test]
    fn display_currency() {
        assert_eq!(String::from("0.00"), super::display_currency(0.0f64, 2, ","));
        assert_eq!(String::from("0.000000000000"), super::display_currency(0.0f64, 12, ","));
        assert_eq!(
            String::from("123,456.123456789"),
            super::display_currency(123_456.123_456_789_012_f64, 9, ",")
        );
        assert_eq!(
            String::from("123,456"),
            super::display_currency(123_456.123_456_789_012_f64, 0, ",")
        );
        assert_eq!(String::from("1,234"), super::display_currency(1234.1f64, 0, ","));
    }
}
