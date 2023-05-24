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

use std::sync::Arc;

use rand::rngs::OsRng;
use tari_common::configuration::Network;
use tari_common_types::types::{ComAndPubSignature, Commitment, CommitmentFactory, PrivateKey, PublicKey, Signature};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    keys::{PublicKey as PK, SecretKey},
    range_proof::RangeProofService,
};
use tari_script::{inputs, script, ExecutionStack, TariScript};

use super::transaction_components::{TransactionInputVersion, TransactionOutputVersion};
use crate::{
    borsh::SerializedSize,
    consensus::ConsensusManager,
    covenants::Covenant,
    transactions::{
        crypto_factories::CryptoFactories,
        fee::Fee,
        tari_amount::MicroTari,
        transaction_components::{
            EncryptedData,
            KernelBuilder,
            KernelFeatures,
            OutputFeatures,
            OutputType,
            RangeProofType,
            SpentOutput,
            Transaction,
            TransactionInput,
            TransactionKernel,
            TransactionKernelVersion,
            TransactionOutput,
            UnblindedOutput,
        },
        transaction_protocol::{RecoveryData, TransactionMetadata, TransactionProtocolError},
        weight::TransactionWeight,
        SenderTransactionProtocol,
    },
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
        features: OutputFeatures {
            maturity,
            ..Default::default()
        },
        ..Default::default()
    })
}

#[derive(Clone)]
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
    pub ephemeral_private_nonce: PrivateKey,
    pub sender_ephemeral_public_nonce: PublicKey,
    pub commitment_factory: CommitmentFactory,
    pub transaction_weight: TransactionWeight,
    pub recovery_data: RecoveryData,
}

#[derive(Clone)]
pub struct UtxoTestParams {
    pub value: MicroTari,
    pub script: TariScript,
    pub features: OutputFeatures,
    pub input_data: Option<ExecutionStack>,
    pub covenant: Covenant,
    pub output_version: Option<TransactionOutputVersion>,
    pub minimum_value_promise: MicroTari,
}

impl UtxoTestParams {
    pub fn with_value(value: MicroTari) -> Self {
        Self {
            value,
            ..Default::default()
        }
    }
}

impl Default for UtxoTestParams {
    fn default() -> Self {
        Self {
            value: 10.into(),
            script: script![Nop],
            features: OutputFeatures::default(),
            input_data: None,
            covenant: Covenant::default(),
            output_version: None,
            minimum_value_promise: MicroTari::zero(),
        }
    }
}

impl TestParams {
    pub fn new() -> TestParams {
        let r = PrivateKey::random(&mut OsRng);
        let sender_offset_private_key = PrivateKey::random(&mut OsRng);
        let sender_sig_pvt_nonce = PrivateKey::random(&mut OsRng);
        let script_private_key = PrivateKey::random(&mut OsRng);
        Self {
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
            ephemeral_private_nonce: sender_sig_pvt_nonce.clone(),
            sender_ephemeral_public_nonce: PublicKey::from_secret_key(&sender_sig_pvt_nonce),
            commitment_factory: CommitmentFactory::default(),
            transaction_weight: TransactionWeight::v1(),
            recovery_data: RecoveryData {
                encryption_key: PrivateKey::random(&mut OsRng),
            },
        }
    }

    pub fn fee(&self) -> Fee {
        Fee::new(self.transaction_weight)
    }

    pub fn create_unblinded_output_not_recoverable(&self, params: UtxoTestParams) -> Result<UnblindedOutput, String> {
        self.create_output(params, None)
    }

    pub fn create_unblinded_output_with_recovery_data(
        &self,
        params: UtxoTestParams,
    ) -> Result<UnblindedOutput, String> {
        self.create_output(params, Some(&self.recovery_data))
    }

    fn create_output(
        &self,
        params: UtxoTestParams,
        recovery_data: Option<&RecoveryData>,
    ) -> Result<UnblindedOutput, String> {
        let commitment = self
            .commitment_factory
            .commit_value(&self.spend_key, params.value.as_u64());

        let encrypted_data = if let Some(recovery_data) = recovery_data {
            EncryptedData::encrypt_data(
                &recovery_data.encryption_key,
                &commitment,
                params.value,
                &self.spend_key,
            )
            .map_err(|e| format!("{:?}", e))?
        } else {
            EncryptedData::default()
        };
        let version = match params.output_version {
            Some(v) => v,
            None => TransactionOutputVersion::get_current_version(),
        };
        let metadata_signature = TransactionOutput::create_metadata_signature(
            version,
            params.value,
            &self.spend_key,
            &params.script,
            &params.features,
            &self.sender_offset_private_key,
            &params.covenant,
            &encrypted_data,
            params.minimum_value_promise,
        )
        .map_err(|e| format!("{:?}", e))?;

        Ok(UnblindedOutput::new(
            version,
            params.value,
            self.spend_key.clone(),
            params.features,
            params.script.clone(),
            params
                .input_data
                .unwrap_or_else(|| inputs!(self.get_script_public_key())),
            self.script_private_key.clone(),
            self.sender_offset_public_key.clone(),
            metadata_signature,
            0,
            params.covenant,
            encrypted_data,
            params.minimum_value_promise,
        ))
    }

    pub fn get_script_public_key(&self) -> PublicKey {
        PublicKey::from_secret_key(&self.script_private_key)
    }

    pub fn get_script_keypair(&self) -> (PrivateKey, PublicKey) {
        (self.script_private_key.clone(), self.get_script_public_key())
    }

    /// Create a random transaction input for the given amount and maturity period. The input's unblinded
    /// parameters are returned.
    pub fn create_input(&self, params: UtxoTestParams) -> (TransactionInput, UnblindedOutput) {
        let unblinded = self.create_unblinded_output_not_recoverable(params).unwrap();
        let input = unblinded.as_transaction_input(&self.commitment_factory).unwrap();
        (input, unblinded)
    }

    pub fn get_size_for_default_features_and_scripts(&self, num_outputs: usize) -> usize {
        let output_features = OutputFeatures { ..Default::default() };
        self.fee().weighting().round_up_features_and_scripts_size(
            script![Nop].get_serialized_size() + output_features.get_serialized_size(),
        ) * num_outputs
    }

    pub fn commit_value(&self, value: MicroTari) -> Commitment {
        self.commitment_factory.commit_value(&self.spend_key, value.as_u64())
    }
}

impl Default for TestParams {
    fn default() -> Self {
        Self::new()
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
pub fn create_random_signature(fee: MicroTari, lock_height: u64, features: KernelFeatures) -> (PublicKey, Signature) {
    let (k, p) = PublicKey::random_keypair(&mut OsRng);
    (p, create_signature(k, fee, lock_height, features))
}

/// Generate a random transaction signature, returning the public key (excess) and the signature.
pub fn create_signature(k: PrivateKey, fee: MicroTari, lock_height: u64, features: KernelFeatures) -> Signature {
    let r = PrivateKey::random(&mut OsRng);
    let tx_meta = TransactionMetadata::new_with_features(fee, lock_height, features);
    let e = TransactionKernel::build_kernel_challenge_from_tx_meta(
        &TransactionKernelVersion::get_current_version(),
        &PublicKey::from_secret_key(&r),
        &PublicKey::from_secret_key(&k),
        &tx_meta,
    );
    Signature::sign_raw(&k, r, &e).unwrap()
}

/// Generate a random transaction signature given a key, returning the public key (excess) and the signature.
pub fn create_random_signature_from_s_key(
    s_key: PrivateKey,
    fee: MicroTari,
    lock_height: u64,
    features: KernelFeatures,
) -> (PublicKey, Signature) {
    let _rng = rand::thread_rng();
    let r = PrivateKey::random(&mut OsRng);
    let p = PK::from_secret_key(&s_key);
    let tx_meta = TransactionMetadata::new_with_features(fee, lock_height, features);
    let e = TransactionKernel::build_kernel_challenge_from_tx_meta(
        &TransactionKernelVersion::get_current_version(),
        &PublicKey::from_secret_key(&r),
        &p,
        &tx_meta,
    );
    (p, Signature::sign_raw(&s_key, r, &e).unwrap())
}

pub fn create_consensus_manager() -> ConsensusManager {
    ConsensusManager::builder(Network::LocalNet).build()
}

pub fn create_unblinded_coinbase(test_params: &TestParams, height: u64, extra: Option<Vec<u8>>) -> UnblindedOutput {
    let rules = create_consensus_manager();
    let constants = rules.consensus_constants(height);
    test_params
        .create_unblinded_output_not_recoverable(UtxoTestParams {
            value: rules.get_block_reward_at(height),
            features: OutputFeatures::create_coinbase(height + constants.coinbase_lock_height(), extra),
            ..Default::default()
        })
        .unwrap()
}

pub fn create_non_recoverable_unblinded_output(
    script: TariScript,
    output_features: OutputFeatures,
    test_params: &TestParams,
    value: MicroTari,
) -> Result<UnblindedOutput, String> {
    test_params.create_unblinded_output_not_recoverable(UtxoTestParams {
        value,
        script,
        features: output_features,
        ..Default::default()
    })
}

pub fn create_unblinded_output_with_recovery_data(
    script: TariScript,
    output_features: OutputFeatures,
    test_params: &TestParams,
    value: MicroTari,
) -> Result<UnblindedOutput, String> {
    test_params.create_unblinded_output_with_recovery_data(UtxoTestParams {
        value,
        script,
        features: output_features,
        ..Default::default()
    })
}

/// The tx macro is a convenience wrapper around the [create_tx] function, making the arguments optional and explicit
/// via keywords.
#[macro_export]
macro_rules! tx {
  ($amount:expr, fee: $fee:expr, lock: $lock:expr, inputs: $n_in:expr, maturity: $mat:expr, outputs: $n_out:expr, features: $features:expr) => {{
      use $crate::transactions::test_helpers::create_tx;
      create_tx($amount, $fee, $lock, $n_in, $mat, $n_out, $features)
  }};
  ($amount:expr, fee: $fee:expr, lock: $lock:expr, inputs: $n_in:expr, maturity: $mat:expr, outputs: $n_out:expr) => {{
    tx!($amount, fee: $fee, lock: $lock, inputs: $n_in, maturity: $mat, outputs: $n_out, features: Default::default())
  }};

  ($amount:expr, fee: $fee:expr, lock: $lock:expr, inputs: $n_in:expr, outputs: $n_out:expr) => {
    tx!($amount, fee: $fee, lock: $lock, inputs: $n_in, maturity: 0, outputs: $n_out)
  };

  ($amount:expr, fee: $fee:expr, inputs: $n_in:expr, outputs: $n_out:expr, features: $features:expr) => {
    tx!($amount, fee: $fee, lock: 0, inputs: $n_in, maturity: 0, outputs: $n_out, features: $features)
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
///   txn_schema!(from: inputs, to: outputs, fee: 50*uT, lock: 1250,
///     features: OutputFeatures { maturity: 1320, ..Default::default() },
///     input_version: TransactioInputVersion::get_current_version(),
///     output_version: TransactionOutputVersion::get_current_version()
///   );
///   txn_schema!(from: inputs, to: outputs, fee: 50*uT); // Uses default features, default versions and zero lock height
///   txn_schema!(from: inputs, to: outputs); // min fee of 25µT, zero lock height, default features and default versions
///   // as above, and transaction splits the first input in roughly half, returning remainder as change
///   txn_schema!(from: inputs);
/// ```
/// The output of this macro is intended to be used in [spend_utxos].
#[macro_export]
macro_rules! txn_schema {
    (from: $input:expr, to: $outputs:expr, fee: $fee:expr, lock: $lock:expr, features: $features:expr, input_version: $input_version:expr, output_version: $output_version:expr) => {{
        $crate::transactions::test_helpers::TransactionSchema {
            from: $input.clone(),
            to: $outputs.clone(),
            to_outputs: vec![],
            fee: $fee,
            lock_height: $lock,
            features: $features.clone(),
            script: tari_script::script![Nop],
            covenant: Default::default(),
            input_data: None,
            input_version: $input_version.clone(),
            output_version: $output_version.clone()
        }
    }};

    (from: $input:expr, to: $outputs:expr, fee: $fee:expr, lock: $lock:expr, features: $features:expr) => {{
        txn_schema!(
            from: $input,
            to:$outputs,
            fee:$fee,
            lock:$lock,
            features: $features.clone(),
            input_version: None,
            output_version: None
        )
    }};

    (from: $input:expr, to: $outputs:expr, features: $features:expr) => {{
        txn_schema!(
            from: $input,
            to:$outputs,
            fee: 5.into(),
            lock: 0,
            features: $features,
            input_version: None,
            output_version: None
        )
    }};

    (from: $input:expr, to: $outputs:expr, fee: $fee:expr) => {
        txn_schema!(
            from: $input,
            to:$outputs,
            fee:$fee,
            lock:0,
            features: $crate::transactions::transaction_components::OutputFeatures::default(),
            input_version: None,
            output_version: None
        )
    };

    (from: $input:expr, to: $outputs:expr) => {
        txn_schema!(from: $input, to:$outputs, fee: 5.into())
    };

    (from: $input:expr, to: $outputs:expr, input_version: $input_version:expr, output_version: $output_version:expr) => {
        txn_schema!(
            from: $input,
            to:$outputs,
            fee: 5.into(),
            lock:0,
            features: $crate::transactions::transaction_components::OutputFeatures::default(),
            input_version: Some($input_version),
            output_version: Some($output_version)
        )
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
    pub to_outputs: Vec<UnblindedOutput>,
    pub fee: MicroTari,
    pub lock_height: u64,
    pub features: OutputFeatures,
    pub script: TariScript,
    pub input_data: Option<ExecutionStack>,
    pub covenant: Covenant,
    pub input_version: Option<TransactionInputVersion>,
    pub output_version: Option<TransactionOutputVersion>,
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
    output_features: OutputFeatures,
) -> (Transaction, Vec<UnblindedOutput>, Vec<UnblindedOutput>) {
    let (inputs, outputs) = create_unblinded_txos(
        amount,
        input_count,
        input_maturity,
        output_count,
        fee_per_gram,
        &output_features,
        &script![Nop],
        &Default::default(),
    );
    let tx = create_transaction_with(lock_height, fee_per_gram, inputs.clone(), outputs.clone());
    (tx, inputs, outputs.into_iter().map(|(utxo, _)| utxo).collect())
}

pub fn create_unblinded_txos(
    amount: MicroTari,
    input_count: usize,
    input_maturity: u64,
    output_count: usize,
    fee_per_gram: MicroTari,
    output_features: &OutputFeatures,
    output_script: &TariScript,
    output_covenant: &Covenant,
) -> (Vec<UnblindedOutput>, Vec<(UnblindedOutput, PrivateKey)>) {
    let weighting = TransactionWeight::latest();
    // This is a best guess to not underestimate metadata size
    let output_features_and_scripts_size = weighting.round_up_features_and_scripts_size(
        output_features.get_serialized_size() +
            output_script.get_serialized_size() +
            output_covenant.get_serialized_size(),
    ) * output_count;
    let estimated_fee = Fee::new(weighting).calculate(
        fee_per_gram,
        1,
        input_count,
        output_count,
        output_features_and_scripts_size,
    );
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
            let script_offset_pvt_key = if output_features.output_type == OutputType::Coinbase {
                PrivateKey::default()
            } else {
                test_params.sender_offset_private_key.clone()
            };

            (
                test_params
                    .create_unblinded_output_not_recoverable(UtxoTestParams {
                        value: output_amount,
                        covenant: output_covenant.clone(),
                        script: output_script.clone(),
                        features: output_features.clone(),
                        ..Default::default()
                    })
                    .unwrap(),
                script_offset_pvt_key,
            )
        })
        .collect();

    let amount_per_input = amount / input_count as u64;
    let inputs = (0..input_count)
        .map(|i| {
            let mut params = UtxoTestParams {
                features: OutputFeatures {
                    maturity: input_maturity,
                    ..OutputFeatures::default()
                },
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
/// Create an unconfirmed transaction for testing with a valid fee, unique excess_sig, random inputs and outputs, the
/// transaction is only partially constructed
pub fn create_transaction_with(
    lock_height: u64,
    fee_per_gram: MicroTari,
    inputs: Vec<UnblindedOutput>,
    outputs: Vec<(UnblindedOutput, PrivateKey)>,
) -> Transaction {
    let stx_protocol = create_sender_transaction_protocol_with(lock_height, fee_per_gram, inputs, outputs).unwrap();
    stx_protocol.take_transaction().unwrap()
}

pub fn create_sender_transaction_protocol_with(
    lock_height: u64,
    fee_per_gram: MicroTari,
    inputs: Vec<UnblindedOutput>,
    outputs: Vec<(UnblindedOutput, PrivateKey)>,
) -> Result<SenderTransactionProtocol, TransactionProtocolError> {
    let factories = CryptoFactories::default();
    let test_params = TestParams::new();
    let rules = ConsensusManager::builder(Network::LocalNet).build();
    let constants = rules.consensus_constants(0).clone();
    let mut stx_builder = SenderTransactionProtocol::builder(0, constants);
    stx_builder
        .with_lock_height(lock_height)
        .with_fee_per_gram(fee_per_gram)
        .with_offset(test_params.offset.clone())
        .with_private_nonce(test_params.nonce.clone())
        .with_kernel_features(KernelFeatures::empty())
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

    let mut stx_protocol = stx_builder.build(&factories, None, u64::MAX).unwrap();
    stx_protocol.finalize()?;

    Ok(stx_protocol)
}

/// Spend the provided UTXOs to the given amounts. Change will be created with any outstanding amount.
/// You only need to provide the unblinded outputs to spend. This function will calculate the commitment for you.
/// This is obviously less efficient, but is offered as a convenience.
/// The output features will be applied to every output
pub fn spend_utxos(schema: TransactionSchema) -> (Transaction, Vec<UnblindedOutput>) {
    let (mut stx_protocol, outputs) = create_stx_protocol(schema);
    stx_protocol.finalize().unwrap();
    let txn = stx_protocol.get_transaction().unwrap().clone();
    (txn, outputs)
}

#[allow(clippy::too_many_lines)]
pub fn create_stx_protocol(schema: TransactionSchema) -> (SenderTransactionProtocol, Vec<UnblindedOutput>) {
    let factories = CryptoFactories::default();
    let test_params_change_and_txn = TestParams::new();
    let output_version = schema
        .output_version
        .unwrap_or_else(TransactionOutputVersion::get_current_version);
    let constants = ConsensusManager::builder(Network::LocalNet)
        .build()
        .consensus_constants(0)
        .clone();
    let mut stx_builder = SenderTransactionProtocol::builder(0, constants);
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
        // We cant use as_transaction_input as we might want to change the version, so lets create a custom input here
        let version = schema
            .input_version
            .unwrap_or_else(TransactionInputVersion::get_current_version);
        let commitment = factories.commitment.commit(&input.spending_key, &input.value.into());
        let r_a = PrivateKey::random(&mut OsRng);
        let r_x = PrivateKey::random(&mut OsRng);
        let r_y = PrivateKey::random(&mut OsRng);
        let ephemeral_commitment = factories.commitment.commit(&r_x, &r_a);
        let ephemeral_pubkey = PublicKey::from_secret_key(&r_y);

        let challenge = TransactionInput::build_script_signature_challenge(
            &version,
            &ephemeral_commitment,
            &ephemeral_pubkey,
            &input.script,
            &input.input_data,
            &PublicKey::from_secret_key(&input.script_private_key),
            &commitment,
        );
        let script_signature = ComAndPubSignature::sign(
            &input.value.into(),
            &input.spending_key,
            &input.script_private_key,
            &r_a,
            &r_x,
            &r_y,
            &challenge,
            &*factories.commitment,
        )
        .unwrap();

        let utxo = TransactionInput::new(
            version,
            SpentOutput::OutputData {
                features: input.features.clone(),
                commitment,
                script: input.script.clone(),
                sender_offset_public_key: input.sender_offset_public_key.clone(),
                covenant: input.covenant.clone(),
                version: input.version,
                encrypted_data: input.encrypted_data,
                minimum_value_promise: input.minimum_value_promise,
            },
            input.input_data.clone(),
            script_signature,
        );

        stx_builder.with_input(utxo, input.clone());
    }
    let mut outputs = Vec::with_capacity(schema.to.len());
    for val in schema.to {
        let test_params = TestParams::new();
        let utxo = test_params
            .create_unblinded_output_not_recoverable(UtxoTestParams {
                value: val,
                features: schema.features.clone(),
                script: schema.script.clone(),
                input_data: schema.input_data.clone(),
                covenant: schema.covenant.clone(),
                output_version: schema.output_version,
                minimum_value_promise: MicroTari::zero(),
            })
            .unwrap();
        outputs.push(utxo.clone());
        stx_builder
            .with_output(utxo, test_params.sender_offset_private_key)
            .unwrap();
    }
    for mut utxo in schema.to_outputs {
        let test_params = TestParams::new();
        utxo.metadata_signature = TransactionOutput::create_metadata_signature(
            output_version,
            utxo.value,
            &utxo.spending_key,
            &utxo.script,
            &utxo.features,
            &test_params.sender_offset_private_key,
            &utxo.covenant,
            &utxo.encrypted_data,
            utxo.minimum_value_promise,
        )
        .unwrap();
        utxo.sender_offset_public_key = test_params.sender_offset_public_key;
        outputs.push(utxo.clone());
        stx_builder
            .with_output(utxo, test_params.sender_offset_private_key)
            .unwrap();
    }

    let stx_protocol = stx_builder.build(&factories, None, u64::MAX).unwrap();
    let change = stx_protocol.get_change_amount().unwrap();
    // The change output is assigned its own random script offset private key
    let change_sender_offset_public_key = stx_protocol.get_change_sender_offset_public_key().unwrap().unwrap();

    let script = script!(Nop);
    let covenant = Covenant::default();
    let change_features = OutputFeatures::default();

    let encrypted_data = EncryptedData::default();

    let minimum_value_promise = MicroTari::zero();

    let change_metadata_sig = TransactionOutput::create_metadata_signature(
        output_version,
        change,
        &test_params_change_and_txn.change_spend_key,
        &script,
        &change_features,
        &test_params_change_and_txn.sender_offset_private_key,
        &covenant,
        &encrypted_data,
        minimum_value_promise,
    )
    .unwrap();

    let change_output = UnblindedOutput::new_current_version(
        change,
        test_params_change_and_txn.change_spend_key.clone(),
        change_features,
        script,
        inputs!(PublicKey::from_secret_key(
            &test_params_change_and_txn.script_private_key
        )),
        test_params_change_and_txn.script_private_key,
        change_sender_offset_public_key,
        change_metadata_sig,
        0,
        covenant,
        encrypted_data,
        minimum_value_promise,
    );
    outputs.push(change_output);
    (stx_protocol, outputs)
}

pub fn create_coinbase_kernel(excess: &PrivateKey) -> TransactionKernel {
    let public_excess = PublicKey::from_secret_key(excess);
    let s = create_signature(excess.clone(), 0.into(), 0, KernelFeatures::COINBASE_KERNEL);
    KernelBuilder::new()
        .with_features(KernelFeatures::COINBASE_KERNEL)
        .with_excess(&Commitment::from_public_key(&public_excess))
        .with_signature(&s)
        .build()
        .unwrap()
}

/// Create a transaction kernel with the given fee, using random keys to generate the signature
pub fn create_test_kernel(fee: MicroTari, lock_height: u64, features: KernelFeatures) -> TransactionKernel {
    let (excess, s) = create_random_signature(fee, lock_height, features);
    KernelBuilder::new()
        .with_fee(fee)
        .with_lock_height(lock_height)
        .with_features(features)
        .with_excess(&Commitment::from_public_key(&excess))
        .with_signature(&s)
        .build()
        .unwrap()
}

/// Create a new UTXO for the specified value and return the output and spending key
pub fn create_utxo(
    value: MicroTari,
    factories: &CryptoFactories,
    features: &OutputFeatures,
    script: &TariScript,
    covenant: &Covenant,
    minimum_value_promise: MicroTari,
) -> (TransactionOutput, PrivateKey, PrivateKey) {
    let keys = generate_keys();
    let offset_keys = generate_keys();
    let commitment = factories.commitment.commit_value(&keys.k, value.into());
    let proof = if features.range_proof_type == RangeProofType::BulletProofPlus {
        Some(
            factories
                .range_proof
                .construct_proof(&keys.k, value.into())
                .unwrap()
                .into(),
        )
    } else {
        None
    };

    let metadata_sig = TransactionOutput::create_metadata_signature(
        TransactionOutputVersion::get_current_version(),
        value,
        &keys.k,
        script,
        features,
        &offset_keys.k,
        covenant,
        &EncryptedData::default(),
        minimum_value_promise,
    )
    .unwrap();

    let utxo = TransactionOutput::new_current_version(
        features.clone(),
        commitment,
        proof,
        script.clone(),
        offset_keys.pk,
        metadata_sig,
        covenant.clone(),
        EncryptedData::default(),
        minimum_value_promise,
    );
    utxo.verify_range_proof(&CryptoFactories::default().range_proof)
        .unwrap();
    (utxo, keys.k, offset_keys.k)
}

pub fn schema_to_transaction(txns: &[TransactionSchema]) -> (Vec<Arc<Transaction>>, Vec<UnblindedOutput>) {
    let mut tx = Vec::new();
    let mut utxos = Vec::new();
    txns.iter().for_each(|schema| {
        let (txn, mut output) = spend_utxos(schema.clone());
        tx.push(Arc::new(txn));
        utxos.append(&mut output);
    });
    (tx, utxos)
}
