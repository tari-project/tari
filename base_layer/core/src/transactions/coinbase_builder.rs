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
//

use chacha20poly1305::aead::OsRng;
use log::*;
use tari_common_types::{
    tari_address::TariAddress,
    types::{Commitment, PrivateKey, PublicKey},
};
use tari_crypto::keys::PublicKey as PK;
use tari_key_manager::key_manager_service::{KeyManagerInterface, KeyManagerServiceError};
use tari_script::{one_sided_payment_script, stealth_payment_script, ExecutionStack, TariScript};
use tari_utilities::ByteArrayError;
use thiserror::Error;

use crate::{
    consensus::{
        emission::{Emission, EmissionSchedule},
        ConsensusConstants,
    },
    covenants::Covenant,
    one_sided::{
        diffie_hellman_stealth_domain_hasher,
        shared_secret_to_output_encryption_key,
        shared_secret_to_output_spending_key,
        stealth_address_script_spending_key,
    },
    transactions::{
        key_manager::{
            CoreKeyManagerError,
            MemoryDbKeyManager,
            TariKeyId,
            TransactionKeyManagerBranch,
            TransactionKeyManagerInterface,
            TxoStage,
        },
        tari_amount::{uT, MicroMinotari},
        transaction_components::{
            KernelBuilder,
            KernelFeatures,
            OutputFeatures,
            RangeProofType,
            Transaction,
            TransactionBuilder,
            TransactionError,
            TransactionKernel,
            TransactionKernelVersion,
            TransactionOutput,
            TransactionOutputVersion,
            WalletOutput,
        },
        transaction_protocol::TransactionMetadata,
    },
};

pub const LOG_TARGET: &str = "c::tx::coinbase_builder";

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum CoinbaseBuildError {
    #[error("The block height for this coinbase transaction wasn't provided")]
    MissingBlockHeight,
    #[error("The value for the coinbase transaction is missing")]
    MissingFees,
    #[error("The private nonce for this coinbase transaction wasn't provided")]
    MissingNonce,
    #[error("The spend key for this coinbase transaction wasn't provided")]
    MissingSpendKey,
    #[error("The script key for this coinbase transaction wasn't provided")]
    MissingScriptKey,
    #[error("The script for this coinbase transaction wasn't provided")]
    MissingScript,
    #[error("The range proof type for this coinbase transaction wasn't provided")]
    MissingRangeProofType,
    #[error("The wallet public key for this coinbase transaction wasn't provided")]
    MissingWalletPublicKey,
    #[error("The encryption key for this coinbase transaction wasn't provided")]
    MissingEncryptionKey,
    #[error("The sender offset key for this coinbase transaction wasn't provided")]
    MissingSenderOffsetKey,
    #[error("The value encryption was not succeed")]
    ValueEncryptionFailed,
    #[error("An error occurred building the final transaction: `{0}`")]
    BuildError(String),
    #[error("Some inconsistent data was given to the builder. This transaction is not valid")]
    InvalidTransaction,
    #[error("Unable to produce a spender offset key from spend key hash")]
    InvalidSenderOffsetKey,
    #[error("An invalid transaction has been encountered: {0}")]
    TransactionError(#[from] TransactionError),
    #[error("Key manager error: {0}")]
    CoreKeyManagerError(String),
    #[error("Key manager service error: `{0}`")]
    KeyManagerServiceError(String),
    #[error("Conversion error: {0}")]
    ByteArrayError(String),
}

impl From<ByteArrayError> for CoinbaseBuildError {
    fn from(err: ByteArrayError) -> Self {
        CoinbaseBuildError::ByteArrayError(err.to_string())
    }
}

impl From<CoreKeyManagerError> for CoinbaseBuildError {
    fn from(err: CoreKeyManagerError) -> Self {
        CoinbaseBuildError::CoreKeyManagerError(err.to_string())
    }
}

impl From<KeyManagerServiceError> for CoinbaseBuildError {
    fn from(err: KeyManagerServiceError) -> Self {
        CoinbaseBuildError::KeyManagerServiceError(err.to_string())
    }
}

pub struct CoinbaseBuilder<TKeyManagerInterface> {
    key_manager: TKeyManagerInterface,
    block_height: Option<u64>,
    fees: Option<MicroMinotari>,
    spend_key_id: Option<TariKeyId>,
    script_key_id: Option<TariKeyId>,
    encryption_key_id: Option<TariKeyId>,
    sender_offset_key_id: Option<TariKeyId>,
    script: Option<TariScript>,
    covenant: Covenant,
    extra: Option<Vec<u8>>,
    range_proof_type: Option<RangeProofType>,
}

impl<TKeyManagerInterface> CoinbaseBuilder<TKeyManagerInterface>
where TKeyManagerInterface: TransactionKeyManagerInterface
{
    /// Start building a new Coinbase transaction. From here you can build the transaction piecemeal with the builder
    /// methods.
    pub fn new(key_manager: TKeyManagerInterface) -> Self {
        CoinbaseBuilder {
            key_manager,
            block_height: None,
            fees: None,
            spend_key_id: None,
            script_key_id: None,
            encryption_key_id: None,
            sender_offset_key_id: None,
            script: None,
            covenant: Covenant::default(),
            extra: None,
            range_proof_type: None,
        }
    }

    /// Assign the block height. This is used to determine the coinbase maturity and reward.
    pub fn with_block_height(mut self, height: u64) -> Self {
        self.block_height = Some(height);
        self
    }

    /// Indicates the sum total of all fees that the coinbase transaction earns, over and above the block reward
    pub fn with_fees(mut self, value: MicroMinotari) -> Self {
        self.fees = Some(value);
        self
    }

    /// Provides the spend key ID for this transaction. This will usually be provided by a miner's wallet instance.
    pub fn with_spend_key_id(mut self, key: TariKeyId) -> Self {
        self.spend_key_id = Some(key);
        self
    }

    /// Provides the script key ID for this transaction. This will usually be provided by a miner's wallet
    /// instance.
    pub fn with_script_key_id(mut self, key: TariKeyId) -> Self {
        self.script_key_id = Some(key);
        self
    }

    /// Provides the encryption key ID for this transaction. This will usually be provided by a Diffie-Hellman shared
    /// secret.
    pub fn with_encryption_key_id(mut self, key: TariKeyId) -> Self {
        self.encryption_key_id = Some(key);
        self
    }

    /// Provides the sender offset key ID for this transaction. This will usually be provided by a miner's wallet
    /// instance.
    pub fn with_sender_offset_key_id(mut self, key: TariKeyId) -> Self {
        self.sender_offset_key_id = Some(key);
        self
    }

    /// Provides the script for this transaction, usually by a miner's wallet instance.
    pub fn with_script(mut self, script: TariScript) -> Self {
        self.script = Some(script);
        self
    }

    /// Set the covenant for this transaction.
    pub fn with_covenant(mut self, covenant: Covenant) -> Self {
        self.covenant = covenant;
        self
    }

    /// Provide some arbitrary additional information that will be stored in the coinbase output's `coinbase_extra`
    /// field.
    pub fn with_extra(mut self, extra: Vec<u8>) -> Self {
        self.extra = Some(extra);
        self
    }

    /// Provide some arbitrary additional information that will be stored in the coinbase output's `coinbase_extra`
    /// field.
    pub fn with_range_proof_type(mut self, range_proof_type: RangeProofType) -> Self {
        self.range_proof_type = Some(range_proof_type);
        self
    }

    /// Try and construct a Coinbase Transaction. The block reward is taken from the emission curve for the current
    /// block height. The other parameters (keys, nonces etc.) are provided by the caller. Other data is
    /// automatically set: Coinbase transactions have an offset of zero, no fees, the `COINBASE_OUTPUT` flags are set
    /// on the output and kernel, and the maturity schedule is set from the consensus rules.
    pub async fn build(
        self,
        constants: &ConsensusConstants,
        emission_schedule: &EmissionSchedule,
    ) -> Result<(Transaction, WalletOutput), CoinbaseBuildError> {
        let height = self.block_height.ok_or(CoinbaseBuildError::MissingBlockHeight)?;
        let reward = emission_schedule.block_reward(height);
        self.build_with_reward(constants, reward).await
    }

    /// Try and construct a Coinbase Transaction while specifying the block reward. The other parameters (keys, nonces
    /// etc.) are provided by the caller. Other data is automatically set: Coinbase transactions have an offset of
    /// zero, no fees, the `COINBASE_OUTPUT` flags are set on the output and kernel, and the maturity schedule is
    /// set from the consensus rules.
    #[allow(clippy::too_many_lines)]
    #[allow(clippy::erasing_op)] // This is for 0 * uT
    pub async fn build_with_reward(
        self,
        constants: &ConsensusConstants,
        block_reward: MicroMinotari,
    ) -> Result<(Transaction, WalletOutput), CoinbaseBuildError> {
        // gets tx details
        let height = self.block_height.ok_or(CoinbaseBuildError::MissingBlockHeight)?;
        let total_reward = block_reward + self.fees.ok_or(CoinbaseBuildError::MissingFees)?;
        let spending_key_id = self.spend_key_id.ok_or(CoinbaseBuildError::MissingSpendKey)?;
        let script_key_id = self.script_key_id.ok_or(CoinbaseBuildError::MissingScriptKey)?;
        let encryption_key_id = self.encryption_key_id.ok_or(CoinbaseBuildError::MissingEncryptionKey)?;
        let sender_offset_key_id = self
            .sender_offset_key_id
            .ok_or(CoinbaseBuildError::MissingSenderOffsetKey)?;
        let covenant = self.covenant;
        let script = self.script.ok_or(CoinbaseBuildError::MissingScript)?;
        let range_proof_type = self.range_proof_type.ok_or(CoinbaseBuildError::MissingRangeProofType)?;

        let kernel_features = KernelFeatures::create_coinbase();
        let metadata = TransactionMetadata::new_with_features(0.into(), 0, kernel_features);
        // generate kernel signature
        let kernel_version = TransactionKernelVersion::get_current_version();
        let kernel_message = TransactionKernel::build_kernel_signature_message(
            &kernel_version,
            metadata.fee,
            metadata.lock_height,
            &metadata.kernel_features,
            &metadata.burn_commitment,
        );
        let (public_nonce_id, public_nonce) = self
            .key_manager
            .get_next_key(TransactionKeyManagerBranch::KernelNonce.get_branch_key())
            .await?;

        let public_spend_key = self.key_manager.get_public_key_at_key_id(&spending_key_id).await?;

        let kernel_signature = self
            .key_manager
            .get_partial_txo_kernel_signature(
                &spending_key_id,
                &public_nonce_id,
                &public_nonce,
                &public_spend_key,
                &kernel_version,
                &kernel_message,
                &metadata.kernel_features,
                TxoStage::Output,
            )
            .await?;

        let excess = Commitment::from_public_key(&public_spend_key);
        // generate tx details
        let value: u64 = total_reward.into();
        let output_features =
            OutputFeatures::create_coinbase(height + constants.coinbase_min_maturity(), self.extra, range_proof_type);
        let encrypted_data = self
            .key_manager
            .encrypt_data_for_recovery(&spending_key_id, Some(&encryption_key_id), total_reward.into())
            .await?;
        let minimum_value_promise = match range_proof_type {
            RangeProofType::BulletProofPlus => MicroMinotari::zero(),
            RangeProofType::RevealedValue => MicroMinotari(value),
        };

        let output_version = TransactionOutputVersion::get_current_version();
        let metadata_message = TransactionOutput::metadata_signature_message_from_parts(
            &output_version,
            &script,
            &output_features,
            &covenant,
            &encrypted_data,
            &minimum_value_promise,
        );

        let sender_offset_public_key = self.key_manager.get_public_key_at_key_id(&sender_offset_key_id).await?;

        let metadata_sig = self
            .key_manager
            .get_metadata_signature(
                &spending_key_id,
                &value.into(),
                &sender_offset_key_id,
                &output_version,
                &metadata_message,
                output_features.range_proof_type,
            )
            .await?;

        let wallet_output = WalletOutput::new(
            output_version,
            total_reward,
            spending_key_id,
            output_features,
            script,
            ExecutionStack::default(),
            script_key_id,
            sender_offset_public_key,
            metadata_sig,
            0,
            covenant,
            encrypted_data,
            minimum_value_promise,
            &self.key_manager,
        )
        .await?;
        let output = wallet_output
            .to_transaction_output(&self.key_manager)
            .await
            .map_err(|e| CoinbaseBuildError::BuildError(e.to_string()))?;
        let kernel = KernelBuilder::new()
            .with_fee(0 * uT)
            .with_features(kernel_features)
            .with_lock_height(0)
            .with_excess(&excess)
            .with_signature(kernel_signature)
            .build()
            .map_err(|e| CoinbaseBuildError::BuildError(e.to_string()))?;

        let mut builder = TransactionBuilder::new();
        builder
            .add_output(output)
            // A coinbase must have 0 offset or the reward balance check will fail.
            .add_offset(PrivateKey::default())
            // Coinbase has no script offset https://rfc.tari.com/RFC-0201_TariScript.html#script-offset
            .add_script_offset(PrivateKey::default())
            .with_reward(total_reward)
            .with_kernel(kernel);
        let tx = builder
            .build()
            .map_err(|e| CoinbaseBuildError::BuildError(e.to_string()))?;
        Ok((tx, wallet_output))
    }
}

/// Clients that do not need to spend the wallet output must call this function to generate a coinbase transaction,
/// so that the only way to get access to the funds will be via the Diffie-Hellman shared secret.
pub async fn generate_coinbase(
    fee: MicroMinotari,
    reward: MicroMinotari,
    height: u64,
    extra: &[u8],
    key_manager: &MemoryDbKeyManager,
    wallet_payment_address: &TariAddress,
    stealth_payment: bool,
    consensus_constants: &ConsensusConstants,
    range_proof_type: RangeProofType,
) -> Result<(TransactionOutput, TransactionKernel), CoinbaseBuildError> {
    // The script key is not used in the Diffie-Hellmann protocol, so we assign default.
    let script_key_id = TariKeyId::default();
    let (_, coinbase_output, coinbase_kernel, _) = generate_coinbase_with_wallet_output(
        fee,
        reward,
        height,
        extra,
        key_manager,
        &script_key_id,
        wallet_payment_address,
        stealth_payment,
        consensus_constants,
        range_proof_type,
    )
    .await?;
    Ok((coinbase_output, coinbase_kernel))
}

/// Clients that need to spend the wallet output must call this function to generate a coinbase transaction,
/// so that the only way to get access to the funds will be via the Diffie-Hellman shared secret.
pub async fn generate_coinbase_with_wallet_output(
    fee: MicroMinotari,
    reward: MicroMinotari,
    height: u64,
    extra: &[u8],
    key_manager: &MemoryDbKeyManager,
    script_key_id: &TariKeyId,
    wallet_payment_address: &TariAddress,
    stealth_payment: bool,
    consensus_constants: &ConsensusConstants,
    range_proof_type: RangeProofType,
) -> Result<(Transaction, TransactionOutput, TransactionKernel, WalletOutput), CoinbaseBuildError> {
    let (sender_offset_key_id, _) = key_manager
        .get_next_key(TransactionKeyManagerBranch::SenderOffset.get_branch_key())
        .await?;
    let shared_secret = key_manager
        .get_diffie_hellman_shared_secret(&sender_offset_key_id, wallet_payment_address.public_key())
        .await?;
    let spending_key = shared_secret_to_output_spending_key(&shared_secret)?;

    let encryption_private_key = shared_secret_to_output_encryption_key(&shared_secret)?;
    let encryption_key_id = key_manager.import_key(encryption_private_key).await?;

    let spending_key_id = key_manager.import_key(spending_key).await?;

    let script = if stealth_payment {
        let (nonce_private_key, nonce_public_key) = PublicKey::random_keypair(&mut OsRng);
        let c = diffie_hellman_stealth_domain_hasher(&nonce_private_key, wallet_payment_address.public_key());
        let script_spending_key = stealth_address_script_spending_key(&c, wallet_payment_address.public_key());
        stealth_payment_script(&nonce_public_key, &script_spending_key)
    } else {
        one_sided_payment_script(wallet_payment_address.public_key())
    };

    let (transaction, wallet_output) = CoinbaseBuilder::new(key_manager.clone())
        .with_block_height(height)
        .with_fees(fee)
        .with_spend_key_id(spending_key_id)
        .with_encryption_key_id(encryption_key_id)
        .with_sender_offset_key_id(sender_offset_key_id)
        .with_script_key_id(script_key_id.clone())
        .with_script(script)
        .with_extra(extra.to_vec())
        .with_range_proof_type(range_proof_type)
        .build_with_reward(consensus_constants, reward)
        .await?;

    let output = transaction
        .body()
        .outputs()
        .first()
        .ok_or(CoinbaseBuildError::BuildError("No output found".to_string()))?;
    let kernel = transaction
        .body()
        .kernels()
        .first()
        .ok_or(CoinbaseBuildError::BuildError("No kernel found".to_string()))?;

    debug!(target: LOG_TARGET, "Coinbase kernel: {}", kernel.clone());
    debug!(target: LOG_TARGET, "Coinbase output: {}", output.clone());
    Ok((transaction.clone(), output.clone(), kernel.clone(), wallet_output))
}

#[cfg(test)]
mod test {
    use tari_common::configuration::Network;
    use tari_common_types::{tari_address::TariAddress, types::Commitment};

    use crate::{
        consensus::{emission::Emission, ConsensusManager, ConsensusManagerBuilder},
        transactions::{
            coinbase_builder::CoinbaseBuildError,
            crypto_factories::CryptoFactories,
            tari_amount::uT,
            test_helpers::TestParams,
            transaction_components::{KernelFeatures, OutputFeatures, OutputType, TransactionError, TransactionKernel},
            CoinbaseBuilder,
        },
        validation::aggregate_body::AggregateBodyInternalConsistencyValidator,
    };

    fn get_builder() -> (
        CoinbaseBuilder<MemoryDbKeyManager>,
        ConsensusManager,
        CryptoFactories,
        MemoryDbKeyManager,
    ) {
        let network = Network::LocalNet;
        let rules = ConsensusManagerBuilder::new(network).build().unwrap();
        let key_manager = create_memory_db_key_manager();
        let factories = CryptoFactories::default();
        (CoinbaseBuilder::new(key_manager.clone()), rules, factories, key_manager)
    }

    #[tokio::test]
    async fn missing_height() {
        let (builder, rules, _, _) = get_builder();

        assert_eq!(
            builder
                .build(rules.consensus_constants(0), rules.emission_schedule())
                .await
                .unwrap_err(),
            CoinbaseBuildError::MissingBlockHeight
        );
    }

    #[tokio::test]
    async fn missing_fees() {
        let (builder, rules, _, _) = get_builder();
        let builder = builder.with_block_height(42);
        assert_eq!(
            builder
                .build(rules.consensus_constants(42), rules.emission_schedule(),)
                .await
                .unwrap_err(),
            CoinbaseBuildError::MissingFees
        );
    }

    #[tokio::test]
    #[allow(clippy::erasing_op)]
    async fn missing_spend_key() {
        let (builder, rules, _, _) = get_builder();
        let fees = 0 * uT;
        let builder = builder.with_block_height(42).with_fees(fees);
        assert_eq!(
            builder
                .build(rules.consensus_constants(42), rules.emission_schedule(),)
                .await
                .unwrap_err(),
            CoinbaseBuildError::MissingSpendKey
        );
    }

    #[tokio::test]
    async fn valid_coinbase() {
        let (builder, rules, factories, key_manager) = get_builder();
        let p = TestParams::new(&key_manager).await;
        let wallet_payment_address = TariAddress::default();
        let builder = builder
            .with_block_height(42)
            .with_fees(145 * uT)
            .with_spend_key_id(p.spend_key_id.clone())
            .with_encryption_key_id(TariKeyId::default())
            .with_sender_offset_key_id(p.sender_offset_key_id)
            .with_script_key_id(p.script_key_id)
            .with_script(one_sided_payment_script(wallet_payment_address.public_key()))
            .with_range_proof_type(RangeProofType::RevealedValue);
        let (tx, _unblinded_output) = builder
            .build(rules.consensus_constants(42), rules.emission_schedule())
            .await
            .unwrap();
        let utxo = &tx.body.outputs()[0];
        let block_reward = rules.emission_schedule().block_reward(42) + 145 * uT;

        let commitment = key_manager
            .get_commitment(&p.spend_key_id, &block_reward.into())
            .await
            .unwrap();
        assert_eq!(&commitment, utxo.commitment());
        utxo.verify_range_proof(&factories.range_proof).unwrap();
        assert_eq!(utxo.features.output_type, OutputType::Coinbase);
        tx.body
            .check_coinbase_output(
                block_reward,
                rules.consensus_constants(0).coinbase_min_maturity(),
                &factories,
                42,
            )
            .unwrap();

        let body_validator = AggregateBodyInternalConsistencyValidator::new(false, rules, factories);
        body_validator
            .validate(
                tx.body(),
                &tx.offset,
                &tx.script_offset,
                Some(block_reward),
                None,
                u64::MAX,
            )
            .unwrap();
    }

    #[tokio::test]
    async fn invalid_coinbase_maturity() {
        let (builder, rules, factories, key_manager) = get_builder();
        let p = TestParams::new(&key_manager).await;
        let block_reward = rules.emission_schedule().block_reward(42) + 145 * uT;
        let wallet_payment_address = TariAddress::default();
        let builder = builder
            .with_block_height(42)
            .with_fees(145 * uT)
            .with_spend_key_id(p.spend_key_id)
            .with_encryption_key_id(TariKeyId::default())
            .with_sender_offset_key_id(p.sender_offset_key_id)
            .with_script_key_id(p.script_key_id)
            .with_script(one_sided_payment_script(wallet_payment_address.public_key()))
            .with_range_proof_type(RangeProofType::BulletProofPlus);
        let (mut tx, _) = builder
            .build(rules.consensus_constants(42), rules.emission_schedule())
            .await
            .unwrap();
        let mut outputs = tx.body.outputs().clone();
        outputs[0].features.maturity = 1;
        tx.body = AggregateBody::new(tx.body().inputs().clone(), outputs, tx.body().kernels().clone());
        assert!(matches!(
            tx.body.check_coinbase_output(
                block_reward,
                rules.consensus_constants(0).coinbase_min_maturity(),
                &factories,
                42
            ),
            Err(TransactionError::InvalidCoinbaseMaturity)
        ));
    }

    #[tokio::test]
    #[allow(clippy::identity_op)]
    async fn invalid_coinbase_value() {
        let (builder, rules, factories, key_manager) = get_builder();
        let p = TestParams::new(&key_manager).await;
        // We just want some small amount here.
        let missing_fee = rules.emission_schedule().block_reward(4200000) + (2 * uT);
        let wallet_payment_address = TariAddress::default();
        let builder = builder
            .with_block_height(42)
            .with_fees(1 * uT)
            .with_spend_key_id(p.spend_key_id.clone())
            .with_encryption_key_id(TariKeyId::default())
            .with_sender_offset_key_id(p.sender_offset_key_id.clone())
            .with_script_key_id(p.script_key_id.clone())
            .with_script(one_sided_payment_script(wallet_payment_address.public_key()))
            .with_range_proof_type(RangeProofType::BulletProofPlus);
        let (mut tx, _) = builder
            .build(rules.consensus_constants(0), rules.emission_schedule())
            .await
            .unwrap();
        let block_reward = rules.emission_schedule().block_reward(42) + missing_fee;
        let builder = CoinbaseBuilder::new(key_manager.clone());
        let builder = builder
            .with_block_height(4_200_000)
            .with_fees(1 * uT)
            .with_spend_key_id(p.spend_key_id.clone())
            .with_encryption_key_id(TariKeyId::default())
            .with_sender_offset_key_id(p.sender_offset_key_id.clone())
            .with_script_key_id(p.script_key_id.clone())
            .with_script(one_sided_payment_script(wallet_payment_address.public_key()))
            .with_range_proof_type(RangeProofType::BulletProofPlus);
        let (tx2, _) = builder
            .build(rules.consensus_constants(0), rules.emission_schedule())
            .await
            .unwrap();
        let mut coinbase2 = tx2.body.outputs()[0].clone();
        let mut coinbase_kernel2 = tx2.body.kernels()[0].clone();
        coinbase2.features = OutputFeatures::default();
        coinbase_kernel2.features = KernelFeatures::empty();
        tx.body.add_output(coinbase2);
        tx.body.add_kernel(coinbase_kernel2);

        // test catches that coinbase amount is wrong
        assert!(matches!(
            tx.body.check_coinbase_output(
                block_reward,
                rules.consensus_constants(0).coinbase_min_maturity(),
                &factories,
                42
            ),
            Err(TransactionError::InvalidCoinbase)
        ));
        // lets construct a correct one now, with the correct amount.
        let builder = CoinbaseBuilder::new(key_manager.clone());
        let builder = builder
            .with_block_height(42)
            .with_fees(missing_fee)
            .with_spend_key_id(p.spend_key_id)
            .with_encryption_key_id(TariKeyId::default())
            .with_sender_offset_key_id(p.sender_offset_key_id)
            .with_script_key_id(p.script_key_id)
            .with_script(one_sided_payment_script(wallet_payment_address.public_key()))
            .with_range_proof_type(RangeProofType::BulletProofPlus);
        let (tx3, _) = builder
            .build(rules.consensus_constants(0), rules.emission_schedule())
            .await
            .unwrap();
        assert!(tx3
            .body
            .check_coinbase_output(
                block_reward,
                rules.consensus_constants(0).coinbase_min_maturity(),
                &factories,
                42
            )
            .is_ok());
    }
    use tari_key_manager::key_manager_service::KeyManagerInterface;
    use tari_script::one_sided_payment_script;

    use crate::transactions::{
        aggregated_body::AggregateBody,
        key_manager::{
            create_memory_db_key_manager,
            MemoryDbKeyManager,
            TariKeyId,
            TransactionKeyManagerBranch,
            TransactionKeyManagerInterface,
            TxoStage,
        },
        transaction_components::{KernelBuilder, RangeProofType, TransactionKernelVersion},
    };

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    #[allow(clippy::identity_op)]
    async fn invalid_coinbase_amount() {
        // We construct two txs both valid with a single coinbase. We then add a duplicate coinbase utxo to the one, and
        // a duplicate coinbase kernel to the other one.
        let (builder, rules, factories, key_manager) = get_builder();
        let p = TestParams::new(&key_manager).await;
        // We just want some small amount here.
        let missing_fee = rules.emission_schedule().block_reward(4200000) + (2 * uT);
        let wallet_payment_address = TariAddress::default();
        let builder = builder
            .with_block_height(42)
            .with_fees(1 * uT)
            .with_spend_key_id(p.spend_key_id.clone())
            .with_encryption_key_id(TariKeyId::default())
            .with_sender_offset_key_id(p.sender_offset_key_id.clone())
            .with_script_key_id(p.script_key_id.clone())
            .with_script(one_sided_payment_script(wallet_payment_address.public_key()))
            .with_range_proof_type(RangeProofType::RevealedValue);
        let (mut tx, _) = builder
            .build(rules.consensus_constants(0), rules.emission_schedule())
            .await
            .unwrap();

        // we calculate a duplicate tx here so that we can have a coinbase with the correct fee amount
        let block_reward = rules.emission_schedule().block_reward(42) + missing_fee;
        let builder = CoinbaseBuilder::new(key_manager.clone());
        let builder = builder
            .with_block_height(4200000)
            .with_fees(1 * uT)
            .with_spend_key_id(p.spend_key_id.clone())
            .with_encryption_key_id(TariKeyId::default())
            .with_sender_offset_key_id(p.sender_offset_key_id)
            .with_script_key_id(p.script_key_id)
            .with_script(one_sided_payment_script(wallet_payment_address.public_key()))
            .with_range_proof_type(RangeProofType::RevealedValue);
        let (tx2, output) = builder
            .build(rules.consensus_constants(0), rules.emission_schedule())
            .await
            .unwrap();
        let mut tx_kernel_test = tx.clone();

        // let add duplicate coinbase flagged utxo with missing amount
        let coinbase2 = tx2.body.outputs()[0].clone();
        assert!(coinbase2.is_coinbase());
        let mut coinbase_kernel2 = tx2.body.kernels()[0].clone();
        assert!(coinbase_kernel2.is_coinbase());
        coinbase_kernel2.features = KernelFeatures::empty();
        let (new_nonce, nonce) = key_manager
            .get_next_key(TransactionKeyManagerBranch::KernelNonce.get_branch_key())
            .await
            .unwrap();
        let kernel_message = TransactionKernel::build_kernel_signature_message(
            &TransactionKernelVersion::get_current_version(),
            coinbase_kernel2.fee,
            coinbase_kernel2.lock_height,
            &coinbase_kernel2.features,
            &None,
        );
        let excess = key_manager
            .get_txo_kernel_signature_excess_with_offset(&output.spending_key_id, &new_nonce)
            .await
            .unwrap();
        let sig = key_manager
            .get_partial_txo_kernel_signature(
                &output.spending_key_id,
                &new_nonce,
                &nonce,
                &excess,
                &TransactionKernelVersion::get_current_version(),
                &kernel_message,
                &coinbase_kernel2.features,
                TxoStage::Output,
            )
            .await
            .unwrap();
        // we verify that the created signature is correct
        let offset = key_manager
            .get_txo_private_kernel_offset(&output.spending_key_id, &new_nonce)
            .await
            .unwrap();
        let sig_challenge = TransactionKernel::finalize_kernel_signature_challenge(
            &TransactionKernelVersion::get_current_version(),
            &nonce,
            &excess,
            &kernel_message,
        );
        assert!(sig.verify_raw_uniform(&excess, &sig_challenge));

        // we fix the signature and the excess with the now included offset.
        coinbase_kernel2.excess_sig = sig;
        coinbase_kernel2.excess = Commitment::from_public_key(&excess);

        tx.body.add_output(coinbase2);
        tx.body.add_kernel(coinbase_kernel2);
        tx.offset = tx.offset + offset;
        tx.body.sort();

        // lets add duplicate coinbase kernel
        let mut coinbase2 = tx2.body.outputs()[0].clone();
        coinbase2.features = OutputFeatures::default();
        let coinbase_kernel2 = tx2.body.kernels()[0].clone();
        tx_kernel_test.body.add_output(coinbase2);
        tx_kernel_test.body.add_kernel(coinbase_kernel2);

        tx_kernel_test.body.sort();

        // test catches that coinbase count on the kernel is wrong
        assert!(matches!(
            tx_kernel_test.body.check_coinbase_output(
                block_reward,
                rules.consensus_constants(0).coinbase_min_maturity(),
                &factories,
                42
            ),
            Err(TransactionError::MoreThanOneCoinbaseKernel)
        ));
        // testing that "block" is still valid
        let body_validator = AggregateBodyInternalConsistencyValidator::new(false, rules, factories);
        body_validator
            .validate(
                tx.body(),
                &tx.offset,
                &tx.script_offset,
                Some(block_reward),
                None,
                u64::MAX,
            )
            .unwrap();
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    #[allow(clippy::identity_op)]
    async fn multi_coinbase_amount() {
        // We construct two txs both valid with a single coinbase. We then add a duplicate coinbase utxo to the one, and
        // a duplicate coinbase kernel to the other one.
        let (builder, rules, factories, key_manager) = get_builder();
        let p = TestParams::new(&key_manager).await;
        // We just want some small amount here.
        let missing_fee = rules.emission_schedule().block_reward(4200000) + (2 * uT);
        let wallet_payment_address = TariAddress::default();
        let builder = builder
            .with_block_height(42)
            .with_fees(1 * uT)
            .with_spend_key_id(p.spend_key_id.clone())
            .with_encryption_key_id(TariKeyId::default())
            .with_sender_offset_key_id(p.sender_offset_key_id.clone())
            .with_script_key_id(p.script_key_id.clone())
            .with_script(one_sided_payment_script(wallet_payment_address.public_key()))
            .with_range_proof_type(RangeProofType::RevealedValue);
        let (tx1, wo1) = builder
            .build(rules.consensus_constants(0), rules.emission_schedule())
            .await
            .unwrap();

        // we calculate a duplicate tx here so that we can have a coinbase with the correct fee amount
        let block_reward = rules.emission_schedule().block_reward(42) + missing_fee;
        let builder = CoinbaseBuilder::new(key_manager.clone());
        let builder = builder
            .with_block_height(4200000)
            .with_fees(1 * uT)
            .with_spend_key_id(p.spend_key_id.clone())
            .with_encryption_key_id(TariKeyId::default())
            .with_sender_offset_key_id(p.sender_offset_key_id)
            .with_script_key_id(p.script_key_id)
            .with_script(one_sided_payment_script(wallet_payment_address.public_key()))
            .with_range_proof_type(RangeProofType::RevealedValue);
        let (tx2, wo2) = builder
            .build(rules.consensus_constants(0), rules.emission_schedule())
            .await
            .unwrap();

        let coinbase1 = tx1.body.outputs()[0].clone();
        let coinbase2 = tx2.body.outputs()[0].clone();
        let mut kernel_1 = tx1.body.kernels()[0].clone();
        let kernel_2 = tx2.body.kernels()[0].clone();
        let excess = &kernel_1.excess + &kernel_2.excess;
        kernel_1.excess = &kernel_1.excess + &kernel_2.excess;
        kernel_1.excess_sig = &kernel_1.excess_sig + &kernel_2.excess_sig;
        let mut body1 = AggregateBody::new(Vec::new(), vec![coinbase1, coinbase2], vec![kernel_1.clone()]);
        body1.sort();

        body1
            .check_coinbase_output(
                block_reward,
                rules.consensus_constants(0).coinbase_min_maturity(),
                &factories,
                42,
            )
            .unwrap();
        body1.verify_kernel_signatures().unwrap_err();

        // lets create a new kernel with a correct signature
        let (new_nonce1, nonce1) = key_manager
            .get_next_key(TransactionKeyManagerBranch::KernelNonce.get_branch_key())
            .await
            .unwrap();
        let (new_nonce2, nonce2) = key_manager
            .get_next_key(TransactionKeyManagerBranch::KernelNonce.get_branch_key())
            .await
            .unwrap();
        let nonce = &nonce1 + &nonce2;
        let kernel_message = TransactionKernel::build_kernel_signature_message(
            &TransactionKernelVersion::get_current_version(),
            kernel_1.fee,
            kernel_1.lock_height,
            &kernel_1.features,
            &None,
        );

        let mut kernel_signature = key_manager
            .get_partial_txo_kernel_signature(
                &wo1.spending_key_id,
                &new_nonce1,
                &nonce,
                excess.as_public_key(),
                &TransactionKernelVersion::get_current_version(),
                &kernel_message,
                &kernel_1.features,
                TxoStage::Output,
            )
            .await
            .unwrap();
        kernel_signature = &kernel_signature +
            &key_manager
                .get_partial_txo_kernel_signature(
                    &wo2.spending_key_id,
                    &new_nonce2,
                    &nonce,
                    excess.as_public_key(),
                    &TransactionKernelVersion::get_current_version(),
                    &kernel_message,
                    &kernel_1.features,
                    TxoStage::Output,
                )
                .await
                .unwrap();
        let kernel_new = KernelBuilder::new()
            .with_fee(0.into())
            .with_features(kernel_1.features)
            .with_lock_height(kernel_1.lock_height)
            .with_excess(&excess)
            .with_signature(kernel_signature)
            .build()
            .unwrap();

        let mut body2 = AggregateBody::new(Vec::new(), body1.outputs().clone(), vec![kernel_new]);
        body2.sort();

        body2
            .check_coinbase_output(
                block_reward,
                rules.consensus_constants(0).coinbase_min_maturity(),
                &factories,
                42,
            )
            .unwrap();
        body2.verify_kernel_signatures().unwrap();
    }
}
