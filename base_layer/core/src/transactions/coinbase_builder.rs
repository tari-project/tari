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
use blake2::Blake2b;
use digest::consts::U64;
use log::*;
use rand::rngs::OsRng;
use tari_common_types::{
    key_branches::TransactionKeyManagerBranch,
    tari_address::{TariAddress, TariAddressFeatures},
    types::{CompressedCommitment, CompressedPublicKey, PrivateKey, Signature, UncompressedSignature},
};
use tari_comms::types::CommsDHKE;
use tari_crypto::{commitment::HomomorphicCommitmentFactory, keys::SecretKey as _};
use tari_crypto::{
    hashing::DomainSeparatedHasher,
    keys::PublicKey,
    ristretto::{RistrettoPublicKey, RistrettoSecretKey},
    signatures::SchnorrSignatureError,
};
use tari_hashing::KeyManagerTransactionsHashDomain;
use tari_script::{push_pubkey_script, ExecutionStack, TariScript};
use tari_utilities::ByteArray;
use tari_utilities::ByteArrayError;
use thiserror::Error;

use crate::{
    consensus::{
        emission::{Emission, EmissionSchedule},
        ConsensusConstants,
    },
    covenants::Covenant,
    one_sided::{shared_secret_to_output_encryption_key, shared_secret_to_output_spending_key},
    transactions::{
        crypto_factories,
        tari_amount::{uT, MicroMinotari},
        transaction_components::{
            encrypted_data::PaymentId, CoinBaseExtra, EncryptedData, KernelBuilder, KernelFeatures, OutputFeatures,
            RangeProofType, Transaction, TransactionBuilder, TransactionError, TransactionKernel,
            TransactionKernelVersion, TransactionOutput, TransactionOutputVersion, WalletOutput,
        },
        transaction_key_manager::{
            error::KeyManagerServiceError, get_metadata_signature, get_partial_txo_kernel_signature_for_coinbase,
            CoreKeyManagerError, MemoryDbKeyManager, TariKeyId, TransactionKeyManagerInterface, TxoStage,
        },
        transaction_protocol::TransactionMetadata,
        CryptoFactories,
    },
};

pub const LOG_TARGET: &str = "c::tx::coinbase_builder";

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum CoinbaseBuildError {
    #[error("Unable to create a signature for the coinbase transaction")]
    CouldNotCreateSignature,
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
    #[error("The wallet public view key for this coinbase transaction wasn't provided")]
    MissingWalletPublicViewKey,
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

pub struct CoinbaseBuilder {
    block_height: Option<u64>,
    fees: Option<MicroMinotari>,
    commitment_mask_key: Option<RistrettoSecretKey>,
    // script_key: Option<RistrettoSecretKey>,
    encryption_key: Option<RistrettoSecretKey>,
    sender_offset_key: Option<RistrettoSecretKey>,
    script: Option<TariScript>,
    covenant: Covenant,
    extra: Option<CoinBaseExtra>,
    range_proof_type: Option<RangeProofType>,
}

impl CoinbaseBuilder {
    /// Start building a new Coinbase transaction. From here you can build the transaction piecemeal with the builder
    /// methods.
    pub fn new() -> Self {
        CoinbaseBuilder {
            block_height: None,
            fees: None,
            commitment_mask_key: None,
            // script_key: None,
            encryption_key: None,
            sender_offset_key: None,
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

    /// Provides the commitment mask key for this transaction.
    pub fn with_commitment_mask_key(mut self, key: RistrettoSecretKey) -> Self {
        self.commitment_mask_key = Some(key);
        self
    }

    /// Provides the script key for this transaction. This will usually be provided by a miner's wallet
    /// instance.
    // pub fn with_script_key(mut self, key: RistrettoSecretKey) -> Self {
    // self.script_key = Some(key);
    // self
    // }

    /// Provides the encryption key for this transaction. This will usually be provided by a Diffie-Hellman shared
    /// secret.
    pub fn with_encryption_key(mut self, key: RistrettoSecretKey) -> Self {
        self.encryption_key = Some(key);
        self
    }

    /// Provides the sender offset key for this transaction. This will usually be provided by a miner's wallet
    /// instance.
    pub fn with_sender_offset_key(mut self, key: RistrettoSecretKey) -> Self {
        self.sender_offset_key = Some(key);
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
    pub fn with_extra(mut self, extra: CoinBaseExtra) -> Self {
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
        payment_id: PaymentId,
    ) -> Result<Transaction, CoinbaseBuildError> {
        let height = self.block_height.ok_or(CoinbaseBuildError::MissingBlockHeight)?;
        let reward = emission_schedule.block_reward(height);
        self.build_with_reward(constants, reward, payment_id).await.map(|x| x.0)
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
        payment_id: PaymentId,
    ) -> Result<(Transaction, RistrettoSecretKey), CoinbaseBuildError> {
        let crypto_factories = &crypto_factories::CryptoFactories::default();
        // gets tx details
        let height = self.block_height.ok_or(CoinbaseBuildError::MissingBlockHeight)?;
        let total_reward = block_reward + self.fees.ok_or(CoinbaseBuildError::MissingFees)?;
        let commitment_mask_key = self.commitment_mask_key.ok_or(CoinbaseBuildError::MissingSpendKey)?;
        let script_key = RistrettoSecretKey::default(); // self.script_key.ok_or(CoinbaseBuildError::MissingScriptKey)?;
        let encryption_key = self.encryption_key.ok_or(CoinbaseBuildError::MissingEncryptionKey)?;
        let sender_offset_key = self
            .sender_offset_key
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
        let (private_nonce, public_nonce) = RistrettoPublicKey::random_keypair(&mut OsRng);

        let public_commitment_mask_key = RistrettoPublicKey::from_secret_key(&commitment_mask_key);

        let compressed_public_commitment_mask_key =
            CompressedPublicKey::new_from_pk(public_commitment_mask_key.clone());
        let kernel_signature = get_partial_txo_kernel_signature_for_coinbase(
            &commitment_mask_key,
            private_nonce,
            &CompressedPublicKey::new_from_pk(public_nonce),
            &compressed_public_commitment_mask_key,
            &kernel_version,
            &kernel_message,
            // &metadata.kernel_features,
            // TxoStage::Output,
        )
        .map_err(|e| CoinbaseBuildError::CouldNotCreateSignature)?;

        let excess = CompressedCommitment::from_compressed_key(compressed_public_commitment_mask_key);
        // generate tx details
        let value: u64 = total_reward.into();
        let output_features =
            OutputFeatures::create_coinbase(height + constants.coinbase_min_maturity(), self.extra, range_proof_type);
        fn encrypt_data_for_recovery(
            crypto_factories: &CryptoFactories,
            commitment_mask_key: &RistrettoSecretKey,
            recovery_key: &RistrettoSecretKey,
            value: u64,
            payment_id: PaymentId,
        ) -> Result<EncryptedData, TransactionError> {
            // let recovery_key = if let Some(key_id) = custom_recovery_key_id {
            //     self.get_private_key(key_id).await?
            // } else {
            //     self.get_private_view_key().await?
            // };
            // let value_key = value.into();

            // let commitment = self.get_commitment(commitment_mask_key_id, &value_key).await?;
            let commitment = CompressedCommitment::from_commitment(
                crypto_factories.commitment.commit_value(&commitment_mask_key, value),
            );
            // let commitment_private_key = commitment_mask_key;
            let data = EncryptedData::encrypt_data(
                &recovery_key,
                &commitment,
                value.into(),
                &commitment_mask_key,
                payment_id,
            )?;
            Ok(data)
        }
        let encrypted_data =
        // .key_manager
        encrypt_data_for_recovery(
        crypto_factories,
        &commitment_mask_key,
        &encryption_key,
        total_reward.into(),
        payment_id.clone(),
        )?;
        // .await?;
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

        let sender_offset_public_key = RistrettoPublicKey::from_secret_key(&sender_offset_key);
        let compressed_sender_offset_public_key = CompressedPublicKey::new_from_pk(sender_offset_public_key.clone());

        let metadata_sig = get_metadata_signature(
            &crypto_factories,
            &commitment_mask_key,
            &value.into(),
            &sender_offset_key,
            &compressed_sender_offset_public_key,
            &output_version,
            &metadata_message,
            output_features.range_proof_type,
        )?;

        // let commitment_mask_key_id = TariKeyId::Imported {
        //     key: compressed_public_commitment_mask_key.clone(),
        // };
        // let script_key_id = TariKeyId::Imported {
        //     key: CompressedPublicKey::new_from_pk(script_key.clone()),
        // };

        let range_proof = if output_features.range_proof_type == RangeProofType::BulletProofPlus {
            todo!("Bulletproofs range proof not implemented yet")
            // Some(
            //     key_manager
            //         .construct_range_proof(&spending_key_id, value.into(), minimum_value_promise.into())
            //         .await?,
            // )
        } else {
            None
        };

        let commitment = CompressedCommitment::from_commitment(
            crypto_factories.commitment.commit_value(&commitment_mask_key, value),
        );
        let output = TransactionOutput::new(
            output_version,
            output_features,
            commitment,
            range_proof,
            script,
            compressed_sender_offset_public_key,
            metadata_sig,
            covenant,
            encrypted_data,
            minimum_value_promise,
        );
        // let output = wallet_output
        // .to_transaction_output(&self.key_manager)
        // .await
        // .map_err(|e| CoinbaseBuildError::BuildError(e.to_string()))?;
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
        Ok((tx, commitment_mask_key))
    }
}

/// Clients that do not need to spend the wallet output must call this function to generate a coinbase transaction,
/// so that the only way to get access to the funds will be via the Diffie-Hellman shared secret.
pub async fn generate_coinbase(
    fee: MicroMinotari,
    reward: MicroMinotari,
    height: u64,
    extra: &CoinBaseExtra,
    key_manager: &MemoryDbKeyManager,
    wallet_payment_address: &TariAddress,
    stealth_payment: bool,
    consensus_constants: &ConsensusConstants,
    range_proof_type: RangeProofType,
    payment_id: PaymentId,
) -> Result<(TransactionOutput, TransactionKernel), CoinbaseBuildError> {
    // The script key is not used in the Diffie-Hellmann protocol, so we assign default.
    // let script_key_id = TariKeyId::default();
    let (_, coinbase_output, coinbase_kernel, _) = generate_coinbase_with_wallet_output(
        fee,
        reward,
        height,
        extra,
        key_manager,
        // &script_key_id,
        wallet_payment_address,
        stealth_payment,
        consensus_constants,
        range_proof_type,
        payment_id,
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
    extra: &CoinBaseExtra,
    key_manager: &MemoryDbKeyManager,
    // script_key_id: &TariKeyId,
    wallet_payment_address: &TariAddress,
    stealth_payment: bool,
    consensus_constants: &ConsensusConstants,
    range_proof_type: RangeProofType,
    payment_id: PaymentId,
) -> Result<(Transaction, TransactionOutput, TransactionKernel, PrivateKey), CoinbaseBuildError> {
    if !wallet_payment_address
        .features()
        .contains(TariAddressFeatures::create_one_sided_only())
    {
        return Err(CoinbaseBuildError::BuildError(
            "Invalid address, address must be one-sided enabled".to_string(),
        ));
    }
    // let sender_offset = key_manager
    // .get_next_key(TransactionKeyManagerBranch::SenderOffset.get_branch_key())
    // .await?;

    let (sender_offset_sk, sender_offset_pk) = RistrettoPublicKey::random_keypair(&mut OsRng);
    let shared_secret = CommsDHKE::new(
        &sender_offset_sk,
        &wallet_payment_address
            .public_view_key()
            .ok_or(CoinbaseBuildError::MissingWalletPublicViewKey)?
            .to_public_key()?,
    );
    // let shared_secret = key_manager
    //     .get_diffie_hellman_shared_secret(
    //         &sender_offset.key_id,
    //         wallet_payment_address
    //             .public_view_key()
    //             .ok_or(CoinbaseBuildError::MissingWalletPublicViewKey)?,
    //     )
    //     .await?;
    let commitment_mask_private_key = shared_secret_to_output_spending_key(&shared_secret)?;
    // let commitment_mask_key_id = key_manager.import_key(commitment_mask.clone()).await?;

    let encryption_private_key = shared_secret_to_output_encryption_key(&shared_secret)?;
    // let encryption_key_id = key_manager.import_key(encryption_private_key).await?;

    fn stealth_address_script_spending_key(
        private_key: &RistrettoSecretKey,
        spend_key: &RistrettoPublicKey,
    ) -> Result<CompressedPublicKey, TransactionError> {
        // let private_key = self.get_private_key(commitment_mask_key_id).await?;
        let hasher =
            DomainSeparatedHasher::<Blake2b<U64>, KeyManagerTransactionsHashDomain>::new_with_label("script key");
        let hasher = hasher.chain(private_key.as_bytes()).finalize();
        let private_key = PrivateKey::from_uniform_bytes(hasher.as_ref())
            .map_err(|_| KeyManagerServiceError::UnknownError("Invalid commitment mask private key".to_string()))?;
        let public_key = RistrettoPublicKey::from_secret_key(&private_key);
        let public_key = spend_key + &public_key;
        Ok(CompressedPublicKey::new_from_pk(public_key))
    }

    let script_spending_pubkey = if stealth_payment {
        stealth_address_script_spending_key(
            &commitment_mask_private_key,
            &wallet_payment_address.public_spend_key().to_public_key()?,
        )?
    } else {
        wallet_payment_address.public_spend_key().clone()
    };
    let script = push_pubkey_script(&script_spending_pubkey);
    let (transaction, private_key) = CoinbaseBuilder::new()
        .with_block_height(height)
        .with_fees(fee)
        .with_commitment_mask_key(commitment_mask_private_key)
        .with_encryption_key(encryption_private_key)
        .with_sender_offset_key(sender_offset_sk)
        .with_script(script)
        .with_extra(extra.clone())
        .with_range_proof_type(range_proof_type)
        .build_with_reward(consensus_constants, reward, payment_id)
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

    trace!(target: LOG_TARGET, "Coinbase kernel: {}", kernel.clone());
    trace!(target: LOG_TARGET, "Coinbase output: {}", output.clone());
    Ok((transaction.clone(), output.clone(), kernel.clone(), private_key))
}

#[cfg(test)]
mod test {
    use tari_common::configuration::Network;
    use tari_common_types::{
        key_branches::TransactionKeyManagerBranch,
        tari_address::TariAddress,
        types::{CompressedCommitment, CompressedPublicKey, Signature},
    };
    use tari_comms::types::CompressedSignature;

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
        let key_manager = create_memory_db_key_manager().unwrap();
        let factories = CryptoFactories::default();
        (CoinbaseBuilder::new(key_manager.clone()), rules, factories, key_manager)
    }

    #[tokio::test]
    async fn missing_height() {
        let (builder, rules, _, _) = get_builder();

        assert_eq!(
            builder
                .build(
                    rules.consensus_constants(0),
                    rules.emission_schedule(),
                    PaymentId::Empty
                )
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
                .build(
                    rules.consensus_constants(42),
                    rules.emission_schedule(),
                    PaymentId::Empty
                )
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
                .build(
                    rules.consensus_constants(42),
                    rules.emission_schedule(),
                    PaymentId::Empty
                )
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
            .with_commitment_mask_id(p.commitment_mask_key_id.clone())
            .with_encryption_key_id(TariKeyId::default())
            .with_sender_offset_key_id(p.sender_offset_key_id)
            .with_script_key_id(p.script_key_id)
            .with_script(push_pubkey_script(wallet_payment_address.public_spend_key()))
            .with_range_proof_type(RangeProofType::RevealedValue);
        let (tx, _unblinded_output) = builder
            .build(
                rules.consensus_constants(42),
                rules.emission_schedule(),
                PaymentId::Empty,
            )
            .await
            .unwrap();
        let utxo = &tx.body.outputs()[0];
        let block_reward = rules.emission_schedule().block_reward(42) + 145 * uT;

        let commitment = key_manager
            .get_commitment(&p.commitment_mask_key_id, &block_reward.into())
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
                1,
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
            .with_commitment_mask_id(p.commitment_mask_key_id)
            .with_encryption_key_id(TariKeyId::default())
            .with_sender_offset_key_id(p.sender_offset_key_id)
            .with_script_key_id(p.script_key_id)
            .with_script(push_pubkey_script(wallet_payment_address.public_spend_key()))
            .with_range_proof_type(RangeProofType::BulletProofPlus);
        let (mut tx, _) = builder
            .build(
                rules.consensus_constants(42),
                rules.emission_schedule(),
                PaymentId::Empty,
            )
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
                42,
                1
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
            .with_commitment_mask_id(p.commitment_mask_key_id.clone())
            .with_encryption_key_id(TariKeyId::default())
            .with_sender_offset_key_id(p.sender_offset_key_id.clone())
            .with_script_key_id(p.script_key_id.clone())
            .with_script(push_pubkey_script(wallet_payment_address.public_spend_key()))
            .with_range_proof_type(RangeProofType::BulletProofPlus);
        let (mut tx, _) = builder
            .build(
                rules.consensus_constants(0),
                rules.emission_schedule(),
                PaymentId::Empty,
            )
            .await
            .unwrap();
        let block_reward = rules.emission_schedule().block_reward(42) + missing_fee;
        let builder = CoinbaseBuilder::new(key_manager.clone());
        let builder = builder
            .with_block_height(4_200_000)
            .with_fees(1 * uT)
            .with_commitment_mask_id(p.commitment_mask_key_id.clone())
            .with_encryption_key_id(TariKeyId::default())
            .with_sender_offset_key_id(p.sender_offset_key_id.clone())
            .with_script_key_id(p.script_key_id.clone())
            .with_script(push_pubkey_script(wallet_payment_address.public_spend_key()))
            .with_range_proof_type(RangeProofType::BulletProofPlus);
        let (tx2, _) = builder
            .build(
                rules.consensus_constants(0),
                rules.emission_schedule(),
                PaymentId::Empty,
            )
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
                42,
                1
            ),
            Err(TransactionError::InvalidCoinbase)
        ));
        // lets construct a correct one now, with the correct amount.
        let builder = CoinbaseBuilder::new(key_manager.clone());
        let builder = builder
            .with_block_height(42)
            .with_fees(missing_fee)
            .with_commitment_mask_id(p.commitment_mask_key_id)
            .with_encryption_key_id(TariKeyId::default())
            .with_sender_offset_key_id(p.sender_offset_key_id)
            .with_script_key_id(p.script_key_id)
            .with_script(push_pubkey_script(wallet_payment_address.public_spend_key()))
            .with_range_proof_type(RangeProofType::BulletProofPlus);
        let (tx3, _) = builder
            .build(
                rules.consensus_constants(0),
                rules.emission_schedule(),
                PaymentId::Empty,
            )
            .await
            .unwrap();
        assert!(tx3
            .body
            .check_coinbase_output(
                block_reward,
                rules.consensus_constants(0).coinbase_min_maturity(),
                &factories,
                42,
                1
            )
            .is_ok());
    }
    use tari_script::push_pubkey_script;

    use crate::transactions::{
        aggregated_body::AggregateBody,
        transaction_components::{encrypted_data::PaymentId, KernelBuilder, RangeProofType, TransactionKernelVersion},
        transaction_key_manager::{
            create_memory_db_key_manager, MemoryDbKeyManager, TariKeyId, TransactionKeyManagerInterface, TxoStage,
        },
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
            .with_commitment_mask_id(p.commitment_mask_key_id.clone())
            .with_encryption_key_id(TariKeyId::default())
            .with_sender_offset_key_id(p.sender_offset_key_id.clone())
            .with_script_key_id(p.script_key_id.clone())
            .with_script(push_pubkey_script(wallet_payment_address.public_spend_key()))
            .with_range_proof_type(RangeProofType::RevealedValue);
        let (mut tx, _) = builder
            .build(
                rules.consensus_constants(0),
                rules.emission_schedule(),
                PaymentId::Empty,
            )
            .await
            .unwrap();

        // we calculate a duplicate tx here so that we can have a coinbase with the correct fee amount
        let block_reward = rules.emission_schedule().block_reward(42) + missing_fee;
        let builder = CoinbaseBuilder::new(key_manager.clone());
        let builder = builder
            .with_block_height(4200000)
            .with_fees(1 * uT)
            .with_commitment_mask_id(p.commitment_mask_key_id.clone())
            .with_encryption_key_id(TariKeyId::default())
            .with_sender_offset_key_id(p.sender_offset_key_id)
            .with_script_key_id(p.script_key_id)
            .with_script(push_pubkey_script(wallet_payment_address.public_spend_key()))
            .with_range_proof_type(RangeProofType::RevealedValue);
        let (tx2, output) = builder
            .build(
                rules.consensus_constants(0),
                rules.emission_schedule(),
                PaymentId::Empty,
            )
            .await
            .unwrap();
        let mut tx_kernel_test = tx.clone();

        // let add duplicate coinbase flagged utxo with missing amount
        let coinbase2 = tx2.body.outputs()[0].clone();
        assert!(coinbase2.is_coinbase());
        let mut coinbase_kernel2 = tx2.body.kernels()[0].clone();
        assert!(coinbase_kernel2.is_coinbase());
        coinbase_kernel2.features = KernelFeatures::empty();
        let new_nonce = key_manager
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
            .get_txo_kernel_signature_excess_with_offset(&output.spending_key_id, &new_nonce.key_id)
            .await
            .unwrap();
        let sig = key_manager
            .get_partial_txo_kernel_signature(
                &output.spending_key_id,
                &new_nonce.key_id,
                &new_nonce.pub_key,
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
            .get_txo_private_kernel_offset(&output.spending_key_id, &new_nonce.key_id)
            .await
            .unwrap();
        let sig_challenge = TransactionKernel::finalize_kernel_signature_challenge(
            &TransactionKernelVersion::get_current_version(),
            &new_nonce.pub_key,
            &excess,
            &kernel_message,
        );
        assert!(sig
            .to_schnorr_signature()
            .unwrap()
            .verify_raw_uniform(&excess.to_public_key().unwrap(), &sig_challenge));

        // we fix the signature and the excess with the now included offset.
        coinbase_kernel2.excess_sig = sig;
        coinbase_kernel2.excess = CompressedCommitment::from_compressed_key(excess);

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
                42,
                2
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
            .with_commitment_mask_id(p.commitment_mask_key_id.clone())
            .with_encryption_key_id(TariKeyId::default())
            .with_sender_offset_key_id(p.sender_offset_key_id.clone())
            .with_script_key_id(p.script_key_id.clone())
            .with_script(push_pubkey_script(wallet_payment_address.public_spend_key()))
            .with_range_proof_type(RangeProofType::RevealedValue);
        let (tx1, wo1) = builder
            .build(
                rules.consensus_constants(0),
                rules.emission_schedule(),
                PaymentId::Empty,
            )
            .await
            .unwrap();

        // we calculate a duplicate tx here so that we can have a coinbase with the correct fee amount
        let block_reward = rules.emission_schedule().block_reward(42) + missing_fee;
        let builder = CoinbaseBuilder::new(key_manager.clone());
        let builder = builder
            .with_block_height(4200000)
            .with_fees(1 * uT)
            .with_commitment_mask_id(p.commitment_mask_key_id.clone())
            .with_encryption_key_id(TariKeyId::default())
            .with_sender_offset_key_id(p.sender_offset_key_id)
            .with_script_key_id(p.script_key_id)
            .with_script(push_pubkey_script(wallet_payment_address.public_spend_key()))
            .with_range_proof_type(RangeProofType::RevealedValue);
        let (tx2, wo2) = builder
            .build(
                rules.consensus_constants(0),
                rules.emission_schedule(),
                PaymentId::Empty,
            )
            .await
            .unwrap();

        let coinbase1 = tx1.body.outputs()[0].clone();
        let coinbase2 = tx2.body.outputs()[0].clone();
        let mut kernel_1 = tx1.body.kernels()[0].clone();
        let kernel_2 = tx2.body.kernels()[0].clone();
        let excess = &kernel_1.excess.to_commitment().unwrap() + &kernel_2.excess.to_commitment().unwrap();
        kernel_1.excess = CompressedCommitment::from_commitment(
            &kernel_1.excess.to_commitment().unwrap() + &kernel_2.excess.to_commitment().unwrap(),
        );
        kernel_1.excess_sig = CompressedSignature::new_from_schnorr(
            &kernel_1.excess_sig.to_schnorr_signature().unwrap() + &kernel_2.excess_sig.to_schnorr_signature().unwrap(),
        );
        let mut body1 = AggregateBody::new(Vec::new(), vec![coinbase1, coinbase2], vec![kernel_1.clone()]);
        body1.sort();

        body1
            .check_coinbase_output(
                block_reward,
                rules.consensus_constants(0).coinbase_min_maturity(),
                &factories,
                42,
                2,
            )
            .unwrap();
        body1.verify_kernel_signatures().unwrap_err();

        // lets create a new kernel with a correct signature
        let new_nonce1 = key_manager
            .get_next_key(TransactionKeyManagerBranch::KernelNonce.get_branch_key())
            .await
            .unwrap();
        let new_nonce2 = key_manager
            .get_next_key(TransactionKeyManagerBranch::KernelNonce.get_branch_key())
            .await
            .unwrap();
        let nonce = &new_nonce1.pub_key.to_public_key().unwrap() + &new_nonce2.pub_key.to_public_key().unwrap();
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
                &new_nonce1.key_id,
                &CompressedPublicKey::new_from_pk(nonce.clone()),
                &CompressedPublicKey::new_from_pk(excess.as_public_key().clone()),
                &TransactionKernelVersion::get_current_version(),
                &kernel_message,
                &kernel_1.features,
                TxoStage::Output,
            )
            .await
            .unwrap()
            .to_schnorr_signature()
            .unwrap();
        kernel_signature = &kernel_signature
            + &key_manager
                .get_partial_txo_kernel_signature(
                    &wo2.spending_key_id,
                    &new_nonce2.key_id,
                    &CompressedPublicKey::new_from_pk(nonce.clone()),
                    &CompressedPublicKey::new_from_pk(excess.as_public_key().clone()),
                    &TransactionKernelVersion::get_current_version(),
                    &kernel_message,
                    &kernel_1.features,
                    TxoStage::Output,
                )
                .await
                .unwrap()
                .to_schnorr_signature()
                .unwrap();
        let kernel_new = KernelBuilder::new()
            .with_fee(0.into())
            .with_features(kernel_1.features)
            .with_lock_height(kernel_1.lock_height)
            .with_excess(&CompressedCommitment::from_commitment(excess))
            .with_signature(Signature::new_from_schnorr(kernel_signature))
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
                2,
            )
            .unwrap();
        body2.verify_kernel_signatures().unwrap();
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    #[allow(clippy::identity_op)]
    async fn too_may_coinbases() {
        let (builder, rules, factories, key_manager) = get_builder();
        let p = TestParams::new(&key_manager).await;
        // We just want some small amount here.
        let missing_fee = rules.emission_schedule().block_reward(4200000) + (2 * uT);
        let wallet_payment_address = TariAddress::default();
        let builder = builder
            .with_block_height(42)
            .with_fees(1 * uT)
            .with_commitment_mask_id(p.commitment_mask_key_id.clone())
            .with_encryption_key_id(TariKeyId::default())
            .with_sender_offset_key_id(p.sender_offset_key_id.clone())
            .with_script_key_id(p.script_key_id.clone())
            .with_script(push_pubkey_script(wallet_payment_address.public_spend_key()))
            .with_range_proof_type(RangeProofType::RevealedValue);
        let (tx1, wo1) = builder
            .build(
                rules.consensus_constants(0),
                rules.emission_schedule(),
                PaymentId::Empty,
            )
            .await
            .unwrap();

        // we calculate a duplicate tx here so that we can have a coinbase with the correct fee amount
        let block_reward = rules.emission_schedule().block_reward(42) + missing_fee;
        let builder = CoinbaseBuilder::new(key_manager.clone());
        let builder = builder
            .with_block_height(4200000)
            .with_fees(1 * uT)
            .with_commitment_mask_id(p.commitment_mask_key_id.clone())
            .with_encryption_key_id(TariKeyId::default())
            .with_sender_offset_key_id(p.sender_offset_key_id)
            .with_script_key_id(p.script_key_id)
            .with_script(push_pubkey_script(wallet_payment_address.public_spend_key()))
            .with_range_proof_type(RangeProofType::RevealedValue);
        let (tx2, wo2) = builder
            .build(
                rules.consensus_constants(0),
                rules.emission_schedule(),
                PaymentId::Empty,
            )
            .await
            .unwrap();

        let coinbase1 = tx1.body.outputs()[0].clone();
        let coinbase2 = tx2.body.outputs()[0].clone();

        let kernel_1 = tx1.body.kernels()[0].clone();
        let kernel_2 = tx2.body.kernels()[0].clone();
        let excess = &kernel_1.excess.to_commitment().unwrap() + &kernel_2.excess.to_commitment().unwrap();

        // lets create a new kernel with a correct signature
        let new_nonce1 = key_manager
            .get_next_key(TransactionKeyManagerBranch::KernelNonce.get_branch_key())
            .await
            .unwrap();
        let new_nonce2 = key_manager
            .get_next_key(TransactionKeyManagerBranch::KernelNonce.get_branch_key())
            .await
            .unwrap();
        let nonce = &new_nonce1.pub_key.to_public_key().unwrap() + &new_nonce2.pub_key.to_public_key().unwrap();
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
                &new_nonce1.key_id,
                &CompressedPublicKey::new_from_pk(nonce.clone()),
                &CompressedPublicKey::new_from_pk(excess.as_public_key().clone()),
                &TransactionKernelVersion::get_current_version(),
                &kernel_message,
                &kernel_1.features,
                TxoStage::Output,
            )
            .await
            .unwrap()
            .to_schnorr_signature()
            .unwrap();
        kernel_signature = &kernel_signature
            + &key_manager
                .get_partial_txo_kernel_signature(
                    &wo2.spending_key_id,
                    &new_nonce2.key_id,
                    &CompressedPublicKey::new_from_pk(nonce),
                    &CompressedPublicKey::new_from_pk(excess.as_public_key().clone()),
                    &TransactionKernelVersion::get_current_version(),
                    &kernel_message,
                    &kernel_1.features,
                    TxoStage::Output,
                )
                .await
                .unwrap()
                .to_schnorr_signature()
                .unwrap();
        let kernel = KernelBuilder::new()
            .with_fee(0.into())
            .with_features(kernel_1.features)
            .with_lock_height(kernel_1.lock_height)
            .with_excess(&CompressedCommitment::from_commitment(excess))
            .with_signature(Signature::new_from_schnorr(kernel_signature))
            .build()
            .unwrap();

        let mut body = AggregateBody::new(Vec::new(), vec![coinbase1, coinbase2], vec![kernel]);
        body.sort();

        body.check_coinbase_output(
            block_reward,
            rules.consensus_constants(0).coinbase_min_maturity(),
            &factories,
            42,
            1,
        )
        .unwrap_err();
    }
}
