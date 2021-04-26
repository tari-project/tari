//  Copyright 2020, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::{
    chain_storage::BlockchainBackend,
    consensus::ConsensusManager,
    transactions::{
        tari_amount::MicroTari,
        types::{Commitment, CryptoFactories, PrivateKey},
    },
    validation::{FinalHorizonStateValidation, ValidationError},
};
use log::*;
use std::marker::PhantomData;
use tari_crypto::commitment::HomomorphicCommitmentFactory;

const LOG_TARGET: &str = "c::bn::state_machine_service::states::horizon_state_sync::chain_balance";

/// Validate that the chain balances at a given height.
pub struct ChainBalanceValidator<B> {
    rules: ConsensusManager,
    factories: CryptoFactories,
    _phantom: PhantomData<B>,
}

impl<B: BlockchainBackend> ChainBalanceValidator<B> {
    pub fn new(rules: ConsensusManager, factories: CryptoFactories) -> Self {
        Self {
            rules,
            factories,
            _phantom: Default::default(),
        }
    }
}

impl<B: BlockchainBackend> FinalHorizonStateValidation<B> for ChainBalanceValidator<B> {
    fn validate(
        &self,
        height: u64,
        total_utxo_sum: &Commitment,
        total_kernel_sum: &Commitment,
        backend: &B,
    ) -> Result<(), ValidationError>
    {
        let emission_h = self.get_emission_commitment_at(height);
        let total_offset = self.fetch_total_offset_commitment(height, backend)?;

        debug!(
            target: LOG_TARGET,
            "Emission:{:?}. Offset:{:?}, total kernel: {:?}, height: {}, total_utxo: {:?}",
            emission_h,
            total_offset,
            total_kernel_sum,
            height,
            total_utxo_sum
        );
        let input = &(&emission_h + total_kernel_sum) + &total_offset;

        if total_utxo_sum != &input {
            return Err(ValidationError::ChainBalanceValidationFailed(height));
        }

        Ok(())
    }
}

impl<B: BlockchainBackend> ChainBalanceValidator<B> {
    fn fetch_total_offset_commitment(&self, height: u64, backend: &B) -> Result<Commitment, ValidationError> {
        let chain_header = backend.fetch_chain_header_by_height(height)?;
        let offset = &chain_header.accumulated_data().total_kernel_offset;
        Ok(self.factories.commitment.commit(&offset, &0u64.into()))
    }

    fn get_emission_commitment_at(&self, height: u64) -> Commitment {
        let total_supply =
            self.rules.get_total_emission_at(height) + self.rules.consensus_constants(height).faucet_value();
        debug!(
            target: LOG_TARGET,
            "Expected emission at height {} is {}", height, total_supply
        );
        self.commit_value(total_supply)
    }

    #[inline]
    fn commit_value(&self, v: MicroTari) -> Commitment {
        self.factories.commitment.commit_value(&PrivateKey::default(), v.into())
    }
}
