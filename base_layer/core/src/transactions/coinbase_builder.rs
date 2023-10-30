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

use tari_common_types::types::{Commitment, PrivateKey};
use tari_key_manager::key_manager_service::KeyManagerServiceError;
use tari_script::{inputs, script, TariScript};
use thiserror::Error;

use crate::{
    consensus::{
        emission::{Emission, EmissionSchedule},
        ConsensusConstants,
    },
    covenants::Covenant,
    transactions::{
        key_manager::{TariKeyId, TransactionKeyManagerBranch, TransactionKeyManagerInterface, TxoStage},
        tari_amount::{uT, MicroMinotari},
        transaction_components::{
            KernelBuilder,
            KernelFeatures,
            OutputFeatures,
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
    #[error("Key manager service error: `{0}`")]
    KeyManagerServiceError(String),
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
    script: Option<TariScript>,
    covenant: Covenant,
    extra: Option<Vec<u8>>,
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
            script: None,
            covenant: Covenant::default(),
            extra: None,
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

    /// Provides the spend key for this transaction. This will usually be provided by a miner's wallet instance.
    pub fn with_spend_key_id(mut self, key: TariKeyId) -> Self {
        self.spend_key_id = Some(key);
        self
    }

    /// Provides the script key for this transaction. This will usually be provided by a miner's wallet
    /// instance.
    pub fn with_script_key_id(mut self, key: TariKeyId) -> Self {
        self.script_key_id = Some(key);
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
        let covenant = self.covenant;
        let script = self.script.unwrap_or_else(|| script!(Nop));

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
        let public_script_key = self.key_manager.get_public_key_at_key_id(&script_key_id).await?;

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
        let output_features = OutputFeatures::create_coinbase(height + constants.coinbase_min_maturity(), self.extra);
        let encrypted_data = self
            .key_manager
            .encrypt_data_for_recovery(&spending_key_id, None, total_reward.into())
            .await?;
        let minimum_value_promise = MicroMinotari::zero();

        let output_version = TransactionOutputVersion::get_current_version();
        let metadata_message = TransactionOutput::metadata_signature_message_from_parts(
            &output_version,
            &script,
            &output_features,
            &covenant,
            &encrypted_data,
            &minimum_value_promise,
        );

        let (sender_offset_public_key_id, sender_offset_public_key) = self
            .key_manager
            .get_next_key(TransactionKeyManagerBranch::SenderOffset.get_branch_key())
            .await?;

        let metadata_sig = self
            .key_manager
            .get_metadata_signature(
                &spending_key_id,
                &value.into(),
                &sender_offset_public_key_id,
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
            inputs!(public_script_key),
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

#[cfg(test)]
mod test {
    use tari_common::configuration::Network;
    use tari_common_types::types::{Commitment, PrivateKey};
    use tari_utilities::ByteArray;

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
        CoinbaseBuilder<TestKeyManager>,
        ConsensusManager,
        CryptoFactories,
        TestKeyManager,
    ) {
        let network = Network::LocalNet;
        let rules = ConsensusManagerBuilder::new(network).build().unwrap();
        let key_manager = create_test_core_key_manager_with_memory_db();
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
        let builder = builder
            .with_block_height(42)
            .with_fees(145 * uT)
            .with_spend_key_id(p.spend_key_id.clone())
            .with_script_key_id(p.script_key_id);
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
        let builder = builder
            .with_block_height(42)
            .with_fees(145 * uT)
            .with_spend_key_id(p.spend_key_id)
            .with_script_key_id(p.script_key_id);
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
        let builder = builder
            .with_block_height(42)
            .with_fees(1 * uT)
            .with_spend_key_id(p.spend_key_id.clone())
            .with_script_key_id(p.script_key_id.clone());
        let (mut tx, _) = builder
            .build(rules.consensus_constants(0), rules.emission_schedule())
            .await
            .unwrap();
        let block_reward = rules.emission_schedule().block_reward(42) + missing_fee;
        let builder = CoinbaseBuilder::new(key_manager.clone());
        let builder = builder
            .with_block_height(4200000)
            .with_fees(1 * uT)
            .with_spend_key_id(p.spend_key_id.clone())
            .with_script_key_id(p.script_key_id.clone());
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
            .with_script_key_id(p.script_key_id);
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

    use crate::transactions::{
        aggregated_body::AggregateBody,
        key_manager::{TransactionKeyManagerBranch, TransactionKeyManagerInterface, TxoStage},
        test_helpers::{create_test_core_key_manager_with_memory_db, TestKeyManager},
        transaction_components::TransactionKernelVersion,
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
        let builder = builder
            .with_block_height(42)
            .with_fees(1 * uT)
            .with_spend_key_id(p.spend_key_id.clone())
            .with_script_key_id(p.script_key_id.clone());
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
            .with_script_key_id(p.script_key_id);
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
        assert!(sig.verify(&excess, &PrivateKey::from_bytes(&sig_challenge).unwrap()));

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
}
