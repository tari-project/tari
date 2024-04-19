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
use tari_common_sqlite::{error::SqliteStorageError, sqlite_connection_pool::PooledDbConnection};
use tari_common_types::types::{Commitment, PrivateKey, PublicKey, Signature};
use tari_crypto::keys::{PublicKey as PK, SecretKey};
use tari_key_manager::key_manager_service::{storage::sqlite_db::KeyManagerSqliteDatabase, KeyManagerInterface, KeyId};
use tari_script::{inputs, script, ExecutionStack, TariScript};

use super::transaction_components::{TransactionInputVersion, TransactionOutputVersion};
use crate::{
    borsh::SerializedSize,
    consensus::ConsensusManager,
    covenants::Covenant,
    transactions::{
        crypto_factories::CryptoFactories,
        fee::Fee,
        key_manager::{
            create_memory_db_key_manager,
            MemoryDbKeyManager,
            TariKeyId,
            TransactionKeyManagerBranch,
            TransactionKeyManagerInterface,
            TransactionKeyManagerWrapper,
            TxoStage,
        },
        tari_amount::MicroMinotari,
        transaction_components::{
            KernelBuilder,
            KernelFeatures,
            OutputFeatures,
            RangeProofType,
            Transaction,
            TransactionKernel,
            TransactionKernelVersion,
            TransactionOutput,
            WalletOutput,
            WalletOutputBuilder,
        },
        transaction_protocol::{transaction_initializer::SenderTransactionInitializer, TransactionMetadata},
        weight::TransactionWeight,
        SenderTransactionProtocol,
    },
};

pub async fn create_test_input<
    TKeyManagerDbConnection: PooledDbConnection<Error = SqliteStorageError> + Clone + 'static,
>(
    amount: MicroMinotari,
    maturity: u64,
    key_manager: &TransactionKeyManagerWrapper<KeyManagerSqliteDatabase<TKeyManagerDbConnection>>,
    coinbase_extra: Vec<u8>,
) -> WalletOutput {
    let params = TestParams::new(key_manager).await;
    params
        .create_input(
            UtxoTestParams {
                value: amount,
                features: OutputFeatures {
                    maturity,
                    coinbase_extra,
                    ..Default::default()
                },
                ..Default::default()
            },
            key_manager,
        )
        .await
}

#[derive(Clone)]
pub struct TestParams {
    pub spend_key_id: TariKeyId,
    pub spend_key_pk: PublicKey,
    pub script_key_id: TariKeyId,
    pub script_key_pk: PublicKey,
    pub sender_offset_key_id: TariKeyId,
    pub sender_offset_key_pk: PublicKey,
    pub kernel_nonce_key_id: TariKeyId,
    pub kernel_nonce_key_pk: PublicKey,
    pub public_nonce_key_id: TariKeyId,
    pub public_nonce_key_pk: PublicKey,
    pub ephemeral_public_nonce_key_id: TariKeyId,
    pub ephemeral_public_nonce_key_pk: PublicKey,
    pub transaction_weight: TransactionWeight,
}

impl TestParams {
    pub async fn new<TKeyManagerDbConnection: PooledDbConnection<Error = SqliteStorageError> + Clone + 'static>(
        key_manager: &TransactionKeyManagerWrapper<KeyManagerSqliteDatabase<TKeyManagerDbConnection>>,
    ) -> TestParams {
        let (spend_key_id, spend_key_pk, script_key_id, script_key_pk) =
            key_manager.get_next_spend_and_script_key_ids().await.unwrap();
        let (sender_offset_key_id, sender_offset_key_pk) = key_manager
            .get_next_key(TransactionKeyManagerBranch::SenderOffset.get_branch_key())
            .await
            .unwrap();
        let (kernel_nonce_key_id, kernel_nonce_key_pk) = key_manager
            .get_next_key(TransactionKeyManagerBranch::KernelNonce.get_branch_key())
            .await
            .unwrap();
        let (public_nonce_key_id, public_nonce_key_pk) = key_manager
            .get_next_key(TransactionKeyManagerBranch::Nonce.get_branch_key())
            .await
            .unwrap();
        let (ephemeral_public_nonce_key_id, ephemeral_public_nonce_key_pk) = key_manager
            .get_next_key(TransactionKeyManagerBranch::Nonce.get_branch_key())
            .await
            .unwrap();

        Self {
            spend_key_id,
            spend_key_pk,
            script_key_id,
            script_key_pk,
            sender_offset_key_id,
            sender_offset_key_pk,
            kernel_nonce_key_id,
            kernel_nonce_key_pk,
            public_nonce_key_id,
            public_nonce_key_pk,
            ephemeral_public_nonce_key_id,
            ephemeral_public_nonce_key_pk,
            transaction_weight: TransactionWeight::v1(),
        }
    }

    pub fn fee(&self) -> Fee {
        Fee::new(self.transaction_weight)
    }

    pub async fn create_output<
        TKeyManagerDbConnection: PooledDbConnection<Error = SqliteStorageError> + Clone + 'static,
    >(
        &self,
        params: UtxoTestParams,
        key_manager: &TransactionKeyManagerWrapper<KeyManagerSqliteDatabase<TKeyManagerDbConnection>>,
    ) -> Result<WalletOutput, String> {
        let version = match params.output_version {
            Some(v) => v,
            None => TransactionOutputVersion::get_current_version(),
        };
        let input_data = params.input_data.unwrap_or_else(|| inputs!(self.script_key_pk.clone()));

        let output = WalletOutputBuilder::new(params.value, self.spend_key_id.clone())
            .with_features(params.features)
            .with_script(params.script.clone())
            .encrypt_data_for_recovery(key_manager, None)
            .await
            .unwrap()
            .with_input_data(input_data)
            .with_covenant(params.covenant)
            .with_version(version)
            .with_sender_offset_public_key(self.sender_offset_key_pk.clone())
            .with_script_key(self.script_key_id.clone())
            .with_minimum_value_promise(params.minimum_value_promise)
            .sign_as_sender_and_receiver(key_manager, &self.sender_offset_key_id)
            .await
            .unwrap()
            .try_build(key_manager)
            .await
            .unwrap();

        Ok(output)
    }

    /// Create a random transaction input for the given amount and maturity period. The input's wallet
    /// parameters are returned.
    pub async fn create_input<
        TKeyManagerDbConnection: PooledDbConnection<Error = SqliteStorageError> + Clone + 'static,
    >(
        &self,
        params: UtxoTestParams,
        key_manager: &TransactionKeyManagerWrapper<KeyManagerSqliteDatabase<TKeyManagerDbConnection>>,
    ) -> WalletOutput {
        self.create_output(params, key_manager).await.unwrap()
    }

    pub fn get_size_for_default_features_and_scripts(&self, num_outputs: usize) -> std::io::Result<usize> {
        let output_features = OutputFeatures { ..Default::default() };
        Ok(self.fee().weighting().round_up_features_and_scripts_size(
            script![Nop].get_serialized_size()? + output_features.get_serialized_size()?,
        ) * num_outputs)
    }
}

#[derive(Clone)]
pub struct UtxoTestParams {
    pub value: MicroMinotari,
    pub script: TariScript,
    pub features: OutputFeatures,
    pub input_data: Option<ExecutionStack>,
    pub covenant: Covenant,
    pub output_version: Option<TransactionOutputVersion>,
    pub minimum_value_promise: MicroMinotari,
}

impl UtxoTestParams {
    pub fn with_value(value: MicroMinotari) -> Self {
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
            minimum_value_promise: MicroMinotari::zero(),
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
    let _rng = rand::thread_rng();
    let (k, pk) = PublicKey::random_keypair(&mut OsRng);
    let (r, pr) = PublicKey::random_keypair(&mut OsRng);
    TestKeySet { k, pk, r, pr }
}

/// Generate a random transaction signature, returning the public key (excess) and the signature.
pub fn create_random_signature(
    fee: MicroMinotari,
    lock_height: u64,
    features: KernelFeatures,
) -> (PublicKey, Signature) {
    let (k, p) = PublicKey::random_keypair(&mut OsRng);
    (p, create_signature(k, fee, lock_height, features))
}

/// Generate a random transaction signature, returning the public key (excess) and the signature.
pub fn create_signature(k: PrivateKey, fee: MicroMinotari, lock_height: u64, features: KernelFeatures) -> Signature {
    let r = PrivateKey::random(&mut OsRng);
    let tx_meta = TransactionMetadata::new_with_features(fee, lock_height, features);
    let e = TransactionKernel::build_kernel_challenge_from_tx_meta(
        &TransactionKernelVersion::get_current_version(),
        &PublicKey::from_secret_key(&r),
        &PublicKey::from_secret_key(&k),
        &tx_meta,
    );
    Signature::sign_raw_uniform(&k, r, &e).unwrap()
}

/// Generate a random transaction signature given a key, returning the public key (excess) and the signature.
pub async fn create_random_signature_from_secret_key(
    key_manager: &MemoryDbKeyManager,
    secret_key_id: TariKeyId,
    fee: MicroMinotari,
    lock_height: u64,
    kernel_features: KernelFeatures,
    txo_type: TxoStage,
) -> (PublicKey, Signature) {
    let tx_meta = TransactionMetadata::new_with_features(fee, lock_height, kernel_features);
    let (nonce_id, total_nonce) = key_manager
        .get_next_key(TransactionKeyManagerBranch::KernelNonce.get_branch_key())
        .await
        .unwrap();
    let total_excess = key_manager.get_public_key_at_key_id(&secret_key_id).await.unwrap();
    let kernel_version = TransactionKernelVersion::get_current_version();
    let kernel_message = TransactionKernel::build_kernel_signature_message(
        &kernel_version,
        tx_meta.fee,
        tx_meta.lock_height,
        &tx_meta.kernel_features,
        &tx_meta.burn_commitment,
    );
    let kernel_signature = key_manager
        .get_partial_txo_kernel_signature(
            &secret_key_id,
            &nonce_id,
            &total_nonce,
            &total_excess,
            &kernel_version,
            &kernel_message,
            &kernel_features,
            txo_type,
        )
        .await
        .unwrap();
    (total_excess, kernel_signature)
}

pub fn create_consensus_manager() -> ConsensusManager {
    ConsensusManager::builder(Network::LocalNet).build().unwrap()
}

pub async fn create_coinbase_wallet_output(
    test_params: &TestParams,
    height: u64,
    extra: Option<Vec<u8>>,
    range_proof_type: RangeProofType,
) -> WalletOutput {
    let rules = create_consensus_manager();
    let key_manager = create_memory_db_key_manager();
    let constants = rules.consensus_constants(height);
    test_params
        .create_output(
            UtxoTestParams {
                value: rules.get_block_reward_at(height),
                features: OutputFeatures::create_coinbase(
                    height + constants.coinbase_min_maturity(),
                    extra,
                    range_proof_type,
                ),
                minimum_value_promise: if range_proof_type == RangeProofType::BulletProofPlus {
                    MicroMinotari(0)
                } else {
                    rules.get_block_reward_at(height)
                },
                ..Default::default()
            },
            &key_manager,
        )
        .await
        .unwrap()
}

pub async fn create_wallet_output_with_data<
    TKeyManagerDbConnection: PooledDbConnection<Error = SqliteStorageError> + Clone + 'static,
>(
    script: TariScript,
    output_features: OutputFeatures,
    test_params: &TestParams,
    value: MicroMinotari,
    key_manager: &TransactionKeyManagerWrapper<KeyManagerSqliteDatabase<TKeyManagerDbConnection>>,
) -> Result<WalletOutput, String> {
    test_params
        .create_output(
            UtxoTestParams {
                value,
                script,
                features: output_features,
                ..Default::default()
            },
            key_manager,
        )
        .await
}

/// The tx macro is a convenience wrapper around the [create_tx] function, making the arguments optional and explicit
/// via keywords.
#[macro_export]
macro_rules! tx {
  ($amount:expr, fee: $fee:expr, lock: $lock:expr, inputs: $n_in:expr, maturity: $mat:expr, outputs: $n_out:expr, features: $features:expr, $key_manager:expr) => {{
      use $crate::transactions::test_helpers::create_tx;
      create_tx($amount, $fee, $lock, $n_in, $mat, $n_out, $features, $key_manager).await
  }};
  ($amount:expr, fee: $fee:expr, lock: $lock:expr, inputs: $n_in:expr, maturity: $mat:expr, outputs: $n_out:expr, $key_manager:expr) => {{
    tx!($amount, fee: $fee, lock: $lock, inputs: $n_in, maturity: $mat, outputs: $n_out, features: Default::default(), $key_manager)
  }};

  ($amount:expr, fee: $fee:expr, lock: $lock:expr, inputs: $n_in:expr, outputs: $n_out:expr, $key_manager:expr) => {
    tx!($amount, fee: $fee, lock: $lock, inputs: $n_in, maturity: 0, outputs: $n_out, $key_manager)
  };

  ($amount:expr, fee: $fee:expr, inputs: $n_in:expr, outputs: $n_out:expr, features: $features:expr, $key_manager:expr) => {
    tx!($amount, fee: $fee, lock: 0, inputs: $n_in, maturity: 0, outputs: $n_out, features: $features, $key_manager)
  };

  ($amount:expr, fee: $fee:expr, inputs: $n_in:expr, outputs: $n_out:expr, $key_manager:expr) => {
    tx!($amount, fee: $fee, lock: 0, inputs: $n_in, maturity: 0, outputs: $n_out, $key_manager)
  };

  ($amount:expr, fee: $fee:expr, $key_manager:expr) => {
    tx!($amount, fee: $fee, lock: 0, inputs: 1, maturity: 0, outputs: 2, $key_manager)
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
    (from: $inputs:expr, to: $outputs:expr, fee: $fee:expr, lock: $lock:expr, features: $features:expr, input_version: $input_version:expr, output_version: $output_version:expr) => {{
        $crate::transactions::test_helpers::TransactionSchema {
            from: $inputs.clone(),
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
    pub from: Vec<WalletOutput>,
    pub to: Vec<MicroMinotari>,
    pub to_outputs: Vec<WalletOutput>,
    pub fee: MicroMinotari,
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
pub async fn create_tx(
    amount: MicroMinotari,
    fee_per_gram: MicroMinotari,
    lock_height: u64,
    input_count: usize,
    input_maturity: u64,
    output_count: usize,
    output_features: OutputFeatures,
    key_manager: &MemoryDbKeyManager,
) -> std::io::Result<(Transaction, Vec<WalletOutput>, Vec<WalletOutput>)> {
    let (inputs, outputs) = create_wallet_outputs(
        amount,
        input_count,
        input_maturity,
        output_count,
        fee_per_gram,
        &output_features,
        &script![Nop],
        &Default::default(),
        key_manager,
    )
    .await?;
    let tx = create_transaction_with(lock_height, fee_per_gram, inputs.clone(), outputs.clone(), key_manager).await;
    Ok((tx, inputs, outputs.into_iter().map(|(utxo, _)| utxo).collect()))
}

pub async fn create_wallet_outputs(
    amount: MicroMinotari,
    input_count: usize,
    input_maturity: u64,
    output_count: usize,
    fee_per_gram: MicroMinotari,
    output_features: &OutputFeatures,
    output_script: &TariScript,
    output_covenant: &Covenant,
    key_manager: &MemoryDbKeyManager,
) -> std::io::Result<(Vec<WalletOutput>, Vec<(WalletOutput, TariKeyId)>)> {
    let weighting = TransactionWeight::latest();
    // This is a best guess to not underestimate metadata size
    let output_features_and_scripts_size = weighting.round_up_features_and_scripts_size(
        output_features.get_serialized_size()? +
            output_script.get_serialized_size()? +
            output_covenant.get_serialized_size()?,
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

    let mut outputs = Vec::new();
    for i in 0..output_count {
        let output_amount = if i < output_count - 1 {
            amount_per_output
        } else {
            amount_for_last_output
        };
        let test_params = TestParams::new(key_manager).await;
        let sender_offset_key_id = test_params.sender_offset_key_id.clone();

        let output = test_params
            .create_output(
                UtxoTestParams {
                    value: output_amount,
                    covenant: output_covenant.clone(),
                    script: output_script.clone(),
                    features: output_features.clone(),
                    ..Default::default()
                },
                key_manager,
            )
            .await
            .unwrap();
        outputs.push((output, sender_offset_key_id));
    }

    let amount_per_input = amount / input_count as u64;
    let mut inputs = Vec::new();
    for i in 0..input_count {
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

        let wallet_output = TestParams::new(key_manager)
            .await
            .create_input(params, key_manager)
            .await;
        inputs.push(wallet_output);
    }

    Ok((inputs, outputs))
}
/// Create an unconfirmed transaction for testing with a valid fee, unique excess_sig, random inputs and outputs, the
/// transaction is only partially constructed
pub async fn create_transaction_with(
    lock_height: u64,
    fee_per_gram: MicroMinotari,
    inputs: Vec<WalletOutput>,
    outputs: Vec<(WalletOutput, TariKeyId)>,
    key_manager: &MemoryDbKeyManager,
) -> Transaction {
    let rules = ConsensusManager::builder(Network::LocalNet).build().unwrap();
    let constants = rules.consensus_constants(0).clone();
    let mut stx_builder = SenderTransactionProtocol::builder(constants, key_manager.clone());
    let change = TestParams::new(key_manager).await;
    stx_builder
        .with_lock_height(lock_height)
        .with_fee_per_gram(fee_per_gram)
        .with_kernel_features(KernelFeatures::empty())
        .with_change_data(
            TariScript::default(),
            ExecutionStack::default(),
            change.script_key_id,
            change.spend_key_id,
            Covenant::default(),
        );
    for input in inputs {
        stx_builder.with_input(input).await.unwrap();
    }

    for (output, script_offset_key_id) in outputs {
        stx_builder.with_output(output, script_offset_key_id).await.unwrap();
    }

    let mut stx_protocol = stx_builder.build().await.unwrap();
    stx_protocol.finalize(key_manager).await.unwrap();

    stx_protocol.into_transaction().unwrap()
}

/// Spend the provided UTXOs to the given amounts. Change will be created with any outstanding amount.
/// You only need to provide the wallet outputs to spend. This function will calculate the commitment for you.
/// This is obviously less efficient, but is offered as a convenience.
/// The output features will be applied to every output
pub async fn spend_utxos(
    schema: TransactionSchema,
    key_manager: &MemoryDbKeyManager,
) -> (Transaction, Vec<WalletOutput>) {
    let (mut stx_protocol, outputs) = create_stx_protocol(schema, key_manager).await;
    stx_protocol.finalize(key_manager).await.unwrap();
    let txn = stx_protocol.get_transaction().unwrap().clone();
    (txn, outputs)
}

#[allow(clippy::too_many_lines)]
pub async fn create_stx_protocol(
    schema: TransactionSchema,
    key_manager: &MemoryDbKeyManager,
) -> (SenderTransactionProtocol, Vec<WalletOutput>) {
    let mut outputs = Vec::with_capacity(schema.to.len());
    let stx_builder = create_stx_protocol_internal(schema, key_manager, &mut outputs).await;

    let stx_protocol = stx_builder.build().await.unwrap();
    let change_output = stx_protocol.get_change_output().unwrap().unwrap();

    outputs.push(change_output);
    (stx_protocol, outputs)
}

#[allow(clippy::too_many_lines)]
pub async fn create_stx_protocol_internal(
    schema: TransactionSchema,
    key_manager: &MemoryDbKeyManager,
    outputs: &mut Vec<WalletOutput>,
) -> SenderTransactionInitializer<MemoryDbKeyManager> {
    let constants = ConsensusManager::builder(Network::LocalNet)
        .build()
        .unwrap()
        .consensus_constants(0)
        .clone();
    let mut stx_builder = SenderTransactionProtocol::builder(constants, key_manager.clone());
    let change = TestParams::new(key_manager).await;
    let script_public_key = key_manager
        .get_public_key_at_key_id(&change.script_key_id)
        .await
        .unwrap();
    stx_builder
        .with_lock_height(schema.lock_height)
        .with_fee_per_gram(schema.fee)
        .with_change_data(
            script!(PushPubKey(Box::new(script_public_key))),
            ExecutionStack::default(),
            change.script_key_id,
            change.spend_key_id,
            Covenant::default(),
        );

    for tx_input in &schema.from {
        stx_builder.with_input(tx_input.clone()).await.unwrap();
    }
    for val in schema.to {
        let (spending_key, _) = key_manager
            .get_next_key(TransactionKeyManagerBranch::CommitmentMask.get_branch_key())
            .await
            .unwrap();
        let (sender_offset_key_id, sender_offset_public_key) = key_manager
            .get_next_key(TransactionKeyManagerBranch::SenderOffset.get_branch_key())
            .await
            .unwrap();
        let script_key_id = KeyId::Derived {branch: TransactionKeyManagerBranch::CommitmentMask.get_branch_key(), index:spending_key.managed_index().unwrap() };
        let script_public_key = key_manager.get_public_key_at_key_id(&script_key_id).await.unwrap();
        let input_data = match &schema.input_data {
            Some(data) => data.clone(),
            None => inputs!(script_public_key),
        };
        let version = match schema.output_version {
            Some(data) => data,
            None => TransactionOutputVersion::get_current_version(),
        };
        let output = WalletOutputBuilder::new(val, spending_key)
            .with_features(schema.features.clone())
            .with_script(schema.script.clone())
            .encrypt_data_for_recovery(key_manager, None)
            .await
            .unwrap()
            .with_input_data(input_data)
            .with_covenant(schema.covenant.clone())
            .with_version(version)
            .with_sender_offset_public_key(sender_offset_public_key)
            .with_script_key(script_key_id.clone())
            .sign_as_sender_and_receiver(key_manager, &sender_offset_key_id)
            .await
            .unwrap()
            .try_build(key_manager)
            .await
            .unwrap();

        outputs.push(output.clone());
        stx_builder.with_output(output, sender_offset_key_id).await.unwrap();
    }
    for mut utxo in schema.to_outputs {
        let (sender_offset_key_id, _) = key_manager
            .get_next_key(TransactionKeyManagerBranch::SenderOffset.get_branch_key())
            .await
            .unwrap();
        let metadata_message = TransactionOutput::metadata_signature_message(&utxo);
        utxo.metadata_signature = key_manager
            .get_metadata_signature(
                &utxo.spending_key_id,
                &utxo.value.into(),
                &sender_offset_key_id,
                &utxo.version,
                &metadata_message,
                utxo.features.range_proof_type,
            )
            .await
            .unwrap();

        stx_builder.with_output(utxo, sender_offset_key_id).await.unwrap();
    }

    stx_builder
}

pub async fn create_coinbase_kernel(
    spending_key_id: &TariKeyId,
    key_manager: &MemoryDbKeyManager,
) -> TransactionKernel {
    let kernel_version = TransactionKernelVersion::get_current_version();
    let kernel_features = KernelFeatures::COINBASE_KERNEL;
    let kernel_message =
        TransactionKernel::build_kernel_signature_message(&kernel_version, 0.into(), 0, &kernel_features, &None);
    let (public_nonce_id, public_nonce) = key_manager
        .get_next_key(TransactionKeyManagerBranch::KernelNonce.get_branch_key())
        .await
        .unwrap();
    let public_spend_key = key_manager.get_public_key_at_key_id(spending_key_id).await.unwrap();

    let kernel_signature = key_manager
        .get_partial_txo_kernel_signature(
            spending_key_id,
            &public_nonce_id,
            &public_nonce,
            &public_spend_key,
            &kernel_version,
            &kernel_message,
            &kernel_features,
            TxoStage::Output,
        )
        .await
        .unwrap();

    KernelBuilder::new()
        .with_features(kernel_features)
        .with_excess(&Commitment::from_public_key(&public_spend_key))
        .with_signature(kernel_signature)
        .build()
        .unwrap()
}

/// Create a transaction kernel with the given fee, using random keys to generate the signature
pub fn create_test_kernel(fee: MicroMinotari, lock_height: u64, features: KernelFeatures) -> TransactionKernel {
    let (excess, s) = create_random_signature(fee, lock_height, features);
    KernelBuilder::new()
        .with_fee(fee)
        .with_lock_height(lock_height)
        .with_features(features)
        .with_excess(&Commitment::from_public_key(&excess))
        .with_signature(s)
        .build()
        .unwrap()
}

/// Create a new UTXO for the specified value and return the output and spending key
pub async fn create_utxo(
    value: MicroMinotari,
    key_manager: &MemoryDbKeyManager,
    features: &OutputFeatures,
    script: &TariScript,
    covenant: &Covenant,
    minimum_value_promise: MicroMinotari,
) -> (TransactionOutput, TariKeyId, TariKeyId) {
    let (spending_key_id, _) = key_manager
        .get_next_key(TransactionKeyManagerBranch::CommitmentMask.get_branch_key())
        .await
        .unwrap();
    let encrypted_data = key_manager
        .encrypt_data_for_recovery(&spending_key_id, None, value.into())
        .await
        .unwrap();
    let (sender_offset_key_id, sender_offset_public_key) = key_manager
        .get_next_key(TransactionKeyManagerBranch::SenderOffset.get_branch_key())
        .await
        .unwrap();
    let metadata_message = TransactionOutput::metadata_signature_message_from_parts(
        &TransactionOutputVersion::get_current_version(),
        script,
        features,
        covenant,
        &encrypted_data,
        &minimum_value_promise,
    );
    let metadata_sig = key_manager
        .get_metadata_signature(
            &spending_key_id,
            &value.into(),
            &sender_offset_key_id,
            &TransactionOutputVersion::get_current_version(),
            &metadata_message,
            features.range_proof_type,
        )
        .await
        .unwrap();
    let commitment = key_manager
        .get_commitment(&spending_key_id, &value.into())
        .await
        .unwrap();
    let proof = if features.range_proof_type == RangeProofType::BulletProofPlus {
        Some(
            key_manager
                .construct_range_proof(&spending_key_id, value.into(), minimum_value_promise.into())
                .await
                .unwrap(),
        )
    } else {
        None
    };

    let utxo = TransactionOutput::new_current_version(
        features.clone(),
        commitment,
        proof,
        script.clone(),
        sender_offset_public_key,
        metadata_sig,
        covenant.clone(),
        encrypted_data,
        minimum_value_promise,
    );
    utxo.verify_range_proof(&CryptoFactories::default().range_proof)
        .unwrap();
    (utxo, spending_key_id, sender_offset_key_id)
}

pub async fn schema_to_transaction(
    txns: &[TransactionSchema],
    key_manager: &MemoryDbKeyManager,
) -> (Vec<Arc<Transaction>>, Vec<WalletOutput>) {
    let mut txs = Vec::new();
    let mut utxos = Vec::new();
    for schema in txns {
        let (txn, mut output) = spend_utxos(schema.clone(), key_manager).await;
        txs.push(Arc::new(txn));
        utxos.append(&mut output);
    }

    (txs, utxos)
}
