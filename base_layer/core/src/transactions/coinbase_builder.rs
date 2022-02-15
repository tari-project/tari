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

use rand::rngs::OsRng;
use tari_common_types::types::{BlindingFactor, PrivateKey, PublicKey, Signature};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    inputs,
    keys::{PublicKey as PK, SecretKey},
    script,
    script::TariScript,
};
use thiserror::Error;

use crate::{
    consensus::{
        emission::{Emission, EmissionSchedule},
        ConsensusConstants,
    },
    covenants::Covenant,
    transactions::{
        crypto_factories::CryptoFactories,
        tari_amount::{uT, MicroTari},
        transaction_components::{
            KernelBuilder,
            KernelFeatures,
            OutputFeatures,
            Transaction,
            TransactionBuilder,
            TransactionOutput,
            UnblindedOutput,
        },
        transaction_protocol::{build_challenge, RewindData, TransactionMetadata},
    },
};

#[derive(Debug, Clone, Error, PartialEq)]
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
    #[error("An error occurred building the final transaction: `{0}`")]
    BuildError(String),
    #[error("Some inconsistent data was given to the builder. This transaction is not valid")]
    InvalidTransaction,
}

pub struct CoinbaseBuilder {
    factories: CryptoFactories,
    block_height: Option<u64>,
    fees: Option<MicroTari>,
    spend_key: Option<PrivateKey>,
    script_key: Option<PrivateKey>,
    script: Option<TariScript>,
    private_nonce: Option<PrivateKey>,
    rewind_data: Option<RewindData>,
    covenant: Covenant,
}

impl CoinbaseBuilder {
    /// Start building a new Coinbase transaction. From here you can build the transaction piecemeal with the builder
    /// methods, or pass in a block to `using_block` to determine most of the coinbase parameters automatically.
    pub fn new(factories: CryptoFactories) -> Self {
        CoinbaseBuilder {
            factories,
            block_height: None,
            fees: None,
            spend_key: None,
            script_key: None,
            script: None,
            private_nonce: None,
            rewind_data: None,
            covenant: Covenant::default(),
        }
    }

    /// Assign the block height. This is used to determine the lock height of the transaction.
    pub fn with_block_height(mut self, height: u64) -> Self {
        self.block_height = Some(height);
        self
    }

    /// Indicates the sum total of all fees that the coinbase transaction earns, over and above the block reward
    pub fn with_fees(mut self, value: MicroTari) -> Self {
        self.fees = Some(value);
        self
    }

    /// Provides the private spend key for this transaction. This will usually be provided by a miner's wallet instance.
    pub fn with_spend_key(mut self, key: PrivateKey) -> Self {
        self.spend_key = Some(key);
        self
    }

    /// Provides the private script key for this transaction. This will usually be provided by a miner's wallet
    /// instance.
    pub fn with_script_key(mut self, key: PrivateKey) -> Self {
        self.script_key = Some(key);
        self
    }

    /// Provides the private script for this transaction, usually by a miner's wallet instance.
    pub fn with_script(mut self, script: TariScript) -> Self {
        self.script = Some(script);
        self
    }

    /// Set the covenant for this transaction.
    pub fn with_covenant(mut self, covenant: Covenant) -> Self {
        self.covenant = covenant;
        self
    }

    /// The nonce to be used for this transaction. This will usually be provided by a miner's wallet instance.
    pub fn with_nonce(mut self, nonce: PrivateKey) -> Self {
        self.private_nonce = Some(nonce);
        self
    }

    /// Add the rewind data needed to make this coinbase output rewindable
    pub fn with_rewind_data(mut self, rewind_data: RewindData) -> Self {
        self.rewind_data = Some(rewind_data);
        self
    }

    /// Try and construct a Coinbase Transaction. The block reward is taken from the emission curve for the current
    /// block height. The other parameters (keys, nonces etc.) are provided by the caller. Other data is
    /// automatically set: Coinbase transactions have an offset of zero, no fees, the `COINBASE_OUTPUT` flags are set
    /// on the output and kernel, and the maturity schedule is set from the consensus rules.
    ///
    /// After `build` is called, the struct is destroyed and the private keys stored are dropped and the memory zeroed
    /// out (by virtue of the zero_on_drop crate).

    pub fn build(
        self,
        constants: &ConsensusConstants,
        emission_schedule: &EmissionSchedule,
    ) -> Result<(Transaction, UnblindedOutput), CoinbaseBuildError> {
        let height = self.block_height.ok_or(CoinbaseBuildError::MissingBlockHeight)?;
        let reward = emission_schedule.block_reward(height);
        self.build_with_reward(constants, reward)
    }

    /// Try and construct a Coinbase Transaction while specifying the block reward. The other parameters (keys, nonces
    /// etc.) are provided by the caller. Other data is automatically set: Coinbase transactions have an offset of
    /// zero, no fees, the `COINBASE_OUTPUT` flags are set on the output and kernel, and the maturity schedule is
    /// set from the consensus rules.
    ///
    /// After `build_with_reward` is called, the struct is destroyed and the private keys stored are dropped and the
    /// memory zeroed out (by virtue of the zero_on_drop crate).
    #[allow(clippy::erasing_op)] // This is for 0 * uT
    pub fn build_with_reward(
        self,
        constants: &ConsensusConstants,
        block_reward: MicroTari,
    ) -> Result<(Transaction, UnblindedOutput), CoinbaseBuildError> {
        let height = self.block_height.ok_or(CoinbaseBuildError::MissingBlockHeight)?;
        let total_reward = block_reward + self.fees.ok_or(CoinbaseBuildError::MissingFees)?;
        let nonce = self.private_nonce.ok_or(CoinbaseBuildError::MissingNonce)?;
        let public_nonce = PublicKey::from_secret_key(&nonce);
        let spending_key = self.spend_key.ok_or(CoinbaseBuildError::MissingSpendKey)?;
        let script_private_key = self.script_key.unwrap_or_else(|| spending_key.clone());
        let script = self.script.unwrap_or_else(|| script!(Nop));
        let output_features = OutputFeatures::create_coinbase(height + constants.coinbase_lock_height());
        let excess = self.factories.commitment.commit_value(&spending_key, 0);
        let kernel_features = KernelFeatures::create_coinbase();
        let metadata = TransactionMetadata::default();
        let challenge = build_challenge(&public_nonce, &metadata);
        let sig = Signature::sign(spending_key.clone(), nonce, &challenge)
            .map_err(|_| CoinbaseBuildError::BuildError("Challenge could not be represented as a scalar".into()))?;

        let sender_offset_private_key = PrivateKey::random(&mut OsRng);
        let sender_offset_public_key = PublicKey::from_secret_key(&sender_offset_private_key);
        let covenant = self.covenant;

        let metadata_sig = TransactionOutput::create_final_metadata_signature(
            &total_reward,
            &spending_key,
            &script,
            &output_features,
            &sender_offset_private_key,
            &covenant,
        )
        .map_err(|e| CoinbaseBuildError::BuildError(e.to_string()))?;

        let unblinded_output = UnblindedOutput::new_current_version(
            total_reward,
            spending_key,
            output_features,
            script,
            inputs!(PublicKey::from_secret_key(&script_private_key)),
            script_private_key,
            sender_offset_public_key,
            metadata_sig,
            0,
            covenant,
        );
        let output = if let Some(rewind_data) = self.rewind_data.as_ref() {
            unblinded_output
                .as_rewindable_transaction_output(&self.factories, rewind_data, None)
                .map_err(|e| CoinbaseBuildError::BuildError(e.to_string()))?
        } else {
            unblinded_output
                .as_transaction_output(&self.factories)
                .map_err(|e| CoinbaseBuildError::BuildError(e.to_string()))?
        };
        let kernel = KernelBuilder::new()
            .with_fee(0 * uT)
            .with_features(kernel_features)
            .with_lock_height(0)
            .with_excess(&excess)
            .with_signature(&sig)
            .build()
            .map_err(|e| CoinbaseBuildError::BuildError(e.to_string()))?;

        let mut builder = TransactionBuilder::new();
        builder
            .add_output(output)
            .add_offset(BlindingFactor::default())
            .add_script_offset(BlindingFactor::default())
            .with_reward(total_reward)
            .with_kernel(kernel);
        let tx = builder
            .build(&self.factories, None, height)
            .map_err(|e| CoinbaseBuildError::BuildError(e.to_string()))?;
        Ok((tx, unblinded_output))
    }
}

#[cfg(test)]
mod test {
    use rand::rngs::OsRng;
    use tari_common::configuration::Network;
    use tari_common_types::types::{BlindingFactor, PrivateKey};
    use tari_crypto::{commitment::HomomorphicCommitmentFactory, keys::SecretKey as SecretKeyTrait};

    use crate::{
        consensus::{emission::Emission, ConsensusManager, ConsensusManagerBuilder},
        transactions::{
            coinbase_builder::CoinbaseBuildError,
            crypto_factories::CryptoFactories,
            tari_amount::uT,
            test_helpers::TestParams,
            transaction_components::{KernelFeatures, OutputFeatures, OutputFlags, TransactionError},
            transaction_protocol::RewindData,
            CoinbaseBuilder,
        },
    };

    fn get_builder() -> (CoinbaseBuilder, ConsensusManager, CryptoFactories) {
        let network = Network::LocalNet;
        let rules = ConsensusManagerBuilder::new(network).build();
        let factories = CryptoFactories::default();
        (CoinbaseBuilder::new(factories.clone()), rules, factories)
    }

    #[test]
    fn missing_height() {
        let (builder, rules, _) = get_builder();
        assert_eq!(
            builder
                .build(rules.consensus_constants(0), rules.emission_schedule())
                .unwrap_err(),
            CoinbaseBuildError::MissingBlockHeight
        );
    }

    #[test]
    fn missing_fees() {
        let (builder, rules, _) = get_builder();
        let builder = builder.with_block_height(42);
        assert_eq!(
            builder
                .build(rules.consensus_constants(42), rules.emission_schedule())
                .unwrap_err(),
            CoinbaseBuildError::MissingFees
        );
    }

    #[test]
    #[allow(clippy::erasing_op)]
    fn missing_spend_key() {
        let p = TestParams::new();
        let (builder, rules, _) = get_builder();
        let fees = 0 * uT;
        let builder = builder.with_block_height(42).with_fees(fees).with_nonce(p.nonce);
        assert_eq!(
            builder
                .build(rules.consensus_constants(42), rules.emission_schedule())
                .unwrap_err(),
            CoinbaseBuildError::MissingSpendKey
        );
    }

    #[test]
    fn valid_coinbase() {
        let p = TestParams::new();
        let (builder, rules, factories) = get_builder();
        let builder = builder
            .with_block_height(42)
            .with_fees(145 * uT)
            .with_nonce(p.nonce.clone())
            .with_spend_key(p.spend_key.clone());
        let (tx, _unblinded_output) = builder
            .build(rules.consensus_constants(42), rules.emission_schedule())
            .unwrap();
        let utxo = &tx.body.outputs()[0];
        let block_reward = rules.emission_schedule().block_reward(42) + 145 * uT;

        assert!(factories
            .commitment
            .open_value(&p.spend_key, block_reward.into(), utxo.commitment()));
        utxo.verify_range_proof(&factories.range_proof).unwrap();
        assert!(utxo.features.flags.contains(OutputFlags::COINBASE_OUTPUT));
        tx.body
            .check_coinbase_output(
                block_reward,
                rules.consensus_constants(0).coinbase_lock_height(),
                &factories,
                42,
            )
            .unwrap();
    }

    #[test]
    fn valid_coinbase_with_rewindable_output() {
        let rewind_key = PrivateKey::random(&mut OsRng);
        let rewind_blinding_key = PrivateKey::random(&mut OsRng);
        let proof_message = b"alice__12345678910111";

        let rewind_data = RewindData {
            rewind_key: rewind_key.clone(),
            rewind_blinding_key: rewind_blinding_key.clone(),
            proof_message: proof_message.to_owned(),
        };

        let p = TestParams::new();
        let (builder, rules, factories) = get_builder();
        let builder = builder
            .with_block_height(42)
            .with_fees(145 * uT)
            .with_nonce(p.nonce.clone())
            .with_spend_key(p.spend_key.clone())
            .with_rewind_data(rewind_data);
        let (tx, _) = builder
            .build(rules.consensus_constants(42), rules.emission_schedule())
            .unwrap();
        let block_reward = rules.emission_schedule().block_reward(42) + 145 * uT;

        let rewind_result = tx.body.outputs()[0]
            .full_rewind_range_proof(&factories.range_proof, &rewind_key, &rewind_blinding_key)
            .unwrap();
        assert_eq!(rewind_result.committed_value, block_reward);
        assert_eq!(&rewind_result.proof_message, proof_message);
        assert_eq!(rewind_result.blinding_factor, p.spend_key);
    }

    #[test]
    fn invalid_coinbase_maturity() {
        let p = TestParams::new();
        let (builder, rules, factories) = get_builder();
        let block_reward = rules.emission_schedule().block_reward(42) + 145 * uT;
        let builder = builder
            .with_block_height(42)
            .with_fees(145 * uT)
            .with_nonce(p.nonce.clone())
            .with_spend_key(p.spend_key);
        let (mut tx, _) = builder
            .build(rules.consensus_constants(42), rules.emission_schedule())
            .unwrap();
        tx.body.outputs_mut()[0].features.maturity = 1;
        assert!(matches!(
            tx.body.check_coinbase_output(
                block_reward,
                rules.consensus_constants(0).coinbase_lock_height(),
                &factories,
                42
            ),
            Err(TransactionError::InvalidCoinbaseMaturity)
        ));
    }

    #[test]
    #[allow(clippy::identity_op)]
    fn invalid_coinbase_value() {
        let p = TestParams::new();
        let (builder, rules, factories) = get_builder();
        // We just want some small amount here.
        let missing_fee = rules.emission_schedule().block_reward(4200000) + (2 * uT);
        let builder = builder
            .with_block_height(42)
            .with_fees(1 * uT)
            .with_nonce(p.nonce.clone())
            .with_spend_key(p.spend_key.clone());
        let (mut tx, _) = builder
            .build(rules.consensus_constants(0), rules.emission_schedule())
            .unwrap();
        let block_reward = rules.emission_schedule().block_reward(42) + missing_fee;
        let builder = CoinbaseBuilder::new(factories.clone());
        let builder = builder
            .with_block_height(4200000)
            .with_fees(1 * uT)
            .with_nonce(p.nonce.clone())
            .with_spend_key(p.spend_key.clone());
        let (tx2, _) = builder
            .build(rules.consensus_constants(0), rules.emission_schedule())
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
                rules.consensus_constants(0).coinbase_lock_height(),
                &factories,
                42
            ),
            Err(TransactionError::InvalidCoinbase)
        ));
        // lets construct a correct one now, with the correct amount.
        let builder = CoinbaseBuilder::new(factories.clone());
        let builder = builder
            .with_block_height(42)
            .with_fees(missing_fee)
            .with_nonce(p.nonce.clone())
            .with_spend_key(p.spend_key);
        let (tx3, _) = builder
            .build(rules.consensus_constants(0), rules.emission_schedule())
            .unwrap();
        assert!(tx3
            .body
            .check_coinbase_output(
                block_reward,
                rules.consensus_constants(0).coinbase_lock_height(),
                &factories,
                42
            )
            .is_ok());
    }

    #[test]
    #[allow(clippy::identity_op)]
    fn invalid_coinbase_amount() {
        let p = TestParams::new();
        let (builder, rules, factories) = get_builder();
        // We just want some small amount here.
        let missing_fee = rules.emission_schedule().block_reward(4200000) + (2 * uT);
        let builder = builder
            .with_block_height(42)
            .with_fees(1 * uT)
            .with_nonce(p.nonce.clone())
            .with_spend_key(p.spend_key.clone());
        let (mut tx, _) = builder
            .build(rules.consensus_constants(0), rules.emission_schedule())
            .unwrap();
        let block_reward = rules.emission_schedule().block_reward(42) + missing_fee;
        let builder = CoinbaseBuilder::new(factories.clone());
        let builder = builder
            .with_block_height(4200000)
            .with_fees(1 * uT)
            .with_nonce(p.nonce.clone())
            .with_spend_key(p.spend_key);
        let (tx2, _) = builder
            .build(rules.consensus_constants(0), rules.emission_schedule())
            .unwrap();
        let mut tx_kernel_test = tx.clone();

        // let add duplicate coinbase flagged utxo
        let coinbase2 = tx2.body.outputs()[0].clone();
        let mut coinbase_kernel2 = tx2.body.kernels()[0].clone();
        coinbase_kernel2.features = KernelFeatures::empty();
        tx.body.add_output(coinbase2);
        tx.body.add_kernel(coinbase_kernel2);

        // lets add duplciate coinbase kernel
        let mut coinbase2 = tx2.body.outputs()[0].clone();
        coinbase2.features = OutputFeatures::default();
        let coinbase_kernel2 = tx2.body.kernels()[0].clone();
        tx_kernel_test.body.add_output(coinbase2);
        tx_kernel_test.body.add_kernel(coinbase_kernel2);

        // test catches that coinbase count on the utxo is wrong
        assert!(matches!(
            tx.body.check_coinbase_output(
                block_reward,
                rules.consensus_constants(0).coinbase_lock_height(),
                &factories,
                42
            ),
            Err(TransactionError::MoreThanOneCoinbase)
        ));
        // test catches that coinbase count on the kernel is wrong
        assert!(matches!(
            tx_kernel_test.body.check_coinbase_output(
                block_reward,
                rules.consensus_constants(0).coinbase_lock_height(),
                &factories,
                42
            ),
            Err(TransactionError::MoreThanOneCoinbase)
        ));
        // testing that "block" is still valid
        tx.body
            .validate_internal_consistency(
                &BlindingFactor::default(),
                &PrivateKey::default(),
                false,
                block_reward,
                &factories,
                None,
                u64::MAX,
            )
            .unwrap();
    }
}
