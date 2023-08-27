// Copyright 2022. The Taiji Project
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

use super::{TransactionChainLinkedValidator, TransactionInternalConsistencyValidator};
use crate::{
    chain_storage::{BlockchainBackend, BlockchainDatabase},
    consensus::ConsensusManager,
    transactions::{transaction_components::Transaction, CryptoFactories},
    validation::{traits::TransactionValidator, ValidationError},
};

pub struct TransactionFullValidator<B> {
    db: BlockchainDatabase<B>,
    internal_validator: TransactionInternalConsistencyValidator,
    chain_validator: TransactionChainLinkedValidator<B>,
}

impl<B: BlockchainBackend> TransactionFullValidator<B> {
    pub fn new(
        factories: CryptoFactories,
        bypass_range_proof_verification: bool,
        db: BlockchainDatabase<B>,
        consensus_manager: ConsensusManager,
    ) -> Self {
        let internal_validator = TransactionInternalConsistencyValidator::new(
            bypass_range_proof_verification,
            consensus_manager.clone(),
            factories,
        );
        let chain_validator = TransactionChainLinkedValidator::new(db.clone(), consensus_manager);
        Self {
            db,
            internal_validator,
            chain_validator,
        }
    }
}

impl<B: BlockchainBackend> TransactionValidator for TransactionFullValidator<B> {
    fn validate(&self, tx: &Transaction) -> Result<(), ValidationError> {
        let tip = {
            let db = self.db.db_read_access()?;
            db.fetch_chain_metadata()
        }?;
        self.internal_validator.validate_with_current_tip(tx, tip)?;
        self.chain_validator.validate(tx)?;

        Ok(())
    }
}
