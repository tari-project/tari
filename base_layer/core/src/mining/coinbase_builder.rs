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

use crate::{
    chain_storage::BlockchainBackend,
    consensus::{ConsensusConstants, ConsensusManager},
};
use derive_error::Error;
use std::sync::Arc;
use tari_crypto::{commitment::HomomorphicCommitmentFactory, keys::PublicKey as PK};
use tari_transactions::{
    tari_amount::{uT, MicroTari},
    transaction::{KernelBuilder, KernelFeatures, OutputFeatures, Transaction, TransactionBuilder, UnblindedOutput},
    transaction_protocol::{build_challenge, TransactionMetadata},
    types::{BlindingFactor, CryptoFactories, PrivateKey, PublicKey, Signature},
};

#[derive(Debug, Clone, Error, PartialEq)]
pub enum CoinbaseBuildError {
    /// The block height for this coinbase transaction wasn't provided
    MissingBlockHeight,
    /// The value for the coinbase transaction is missing
    MissingFees,
    /// The private nonce for this coinbase transaction wasn't provided
    MissingNonce,
    /// The spend key for this coinbase transaction wasn't provided
    MissingSpendKey,
    /// An error occurred building the final transaction
    #[error(msg_embedded, no_from, non_std)]
    BuildError(String),
    /// Some inconsistent data was given to the builder. This transaction is not valid
    InvalidTransaction,
}

pub struct CoinbaseBuilder {
    factories: Arc<CryptoFactories>,
    block_height: Option<u64>,
    fees: Option<MicroTari>,
    spend_key: Option<PrivateKey>,
    private_nonce: Option<PrivateKey>,
}

impl CoinbaseBuilder {
    /// Start building a new Coinbase transaction. From here you can build the transaction piecemeal with the builder
    /// methods, or pass in a block to `using_block` to determine most of the coinbase parameters automatically.
    pub fn new(factories: Arc<CryptoFactories>) -> Self {
        CoinbaseBuilder {
            factories,
            block_height: None,
            fees: None,
            spend_key: None,
            private_nonce: None,
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

    /// The nonce to be used for this transaction. This will usually be provided by a miner's wallet instance.
    pub fn with_nonce(mut self, nonce: PrivateKey) -> Self {
        self.private_nonce = Some(nonce);
        self
    }

    /// Try and construct a Coinbase Transaction. The block reward is taken from the emission curve for the current
    /// block height. The other parameters (keys, nonces etc.) are provided by the caller. Other data is
    /// automatically set: Coinbase transactions have an offset of zero, no fees, the `COINBASE_OUTPUT` flags are set
    /// on the output and kernel, and the maturity schedule is set from the consensus rules.
    ///
    /// After `build` is called, the struct is destroyed and the private keys stored are dropped and the memory zeroed
    /// out (by virtue of the zero_on_drop crate).
    pub fn build<B: BlockchainBackend>(self, rules: ConsensusManager<B>) -> Result<Transaction, CoinbaseBuildError> {
        let height = self.block_height.ok_or(CoinbaseBuildError::MissingBlockHeight)?;
        let reward =
            rules.emission_schedule().block_reward(height) + self.fees.ok_or(CoinbaseBuildError::MissingFees)?;
        let nonce = self.private_nonce.ok_or(CoinbaseBuildError::MissingNonce)?;
        let public_nonce = PublicKey::from_secret_key(&nonce);
        let key = self.spend_key.ok_or(CoinbaseBuildError::MissingSpendKey)?;
        let output_features =
            OutputFeatures::create_coinbase(height + ConsensusConstants::current().coinbase_lock_height());
        let excess = self.factories.commitment.commit_value(&key, 0);
        let kernel_features = KernelFeatures::create_coinbase();
        let metadata = TransactionMetadata::default();
        let challenge = build_challenge(&public_nonce, &metadata);
        let sig = Signature::sign(key.clone(), nonce, &challenge)
            .map_err(|_| CoinbaseBuildError::BuildError("Challenge could not be represented as a scalar".into()))?;
        let output = UnblindedOutput::new(reward, key, Some(output_features));
        let output = output
            .as_transaction_output(&self.factories)
            .map_err(|e| CoinbaseBuildError::BuildError(e.to_string()))?;
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
            .with_reward(reward)
            .with_kernel(kernel);
        builder
            .build(&self.factories)
            .map_err(|e| CoinbaseBuildError::BuildError(e.to_string()))
    }
}

#[cfg(test)]
mod test {
    use crate::{
        consensus::ConsensusManager,
        helpers::MockBackend,
        mining::{coinbase_builder::CoinbaseBuildError, CoinbaseBuilder},
    };
    use std::sync::Arc;
    use tari_crypto::commitment::HomomorphicCommitmentFactory;
    use tari_transactions::{helpers::TestParams, tari_amount::uT, transaction::OutputFlags, types::CryptoFactories};

    fn get_builder() -> (CoinbaseBuilder, ConsensusManager<MockBackend>, Arc<CryptoFactories>) {
        let rules = ConsensusManager::default();
        let factories = Arc::new(CryptoFactories::default());
        (CoinbaseBuilder::new(factories.clone()), rules, factories)
    }

    #[test]
    fn missing_height() {
        let (builder, rules, _) = get_builder();
        assert_eq!(
            builder.build(rules).unwrap_err(),
            CoinbaseBuildError::MissingBlockHeight
        );
    }

    #[test]
    fn missing_fees() {
        let (builder, rules, _) = get_builder();
        let builder = builder.with_block_height(42);
        assert_eq!(builder.build(rules).unwrap_err(), CoinbaseBuildError::MissingFees);
    }

    #[test]
    fn missing_spend_key() {
        let p = TestParams::new();
        let (builder, rules, _) = get_builder();
        let builder = builder.with_block_height(42).with_fees(0 * uT).with_nonce(p.nonce);
        assert_eq!(builder.build(rules).unwrap_err(), CoinbaseBuildError::MissingSpendKey);
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
        let tx = builder.build(rules.clone()).unwrap();
        let utxo = &tx.body.outputs()[0];
        let block_reward = rules.emission_schedule().block_reward(42) + 145 * uT;
        assert!(factories
            .commitment
            .open_value(&p.spend_key, block_reward.into(), utxo.commitment()));
        assert!(utxo.verify_range_proof(&factories.range_proof).unwrap());
        assert!(utxo.features.flags.contains(OutputFlags::COINBASE_OUTPUT));
    }
}
