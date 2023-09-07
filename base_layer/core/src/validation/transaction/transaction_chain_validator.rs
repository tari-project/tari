// Copyright 2022. The Tari Project
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

use crate::{
    chain_storage::{BlockchainBackend, BlockchainDatabase},
    consensus::ConsensusManager,
    transactions::transaction_components::Transaction,
    validation::{aggregate_body::AggregateBodyChainLinkedValidator, TransactionValidator, ValidationError},
};

pub struct TransactionChainLinkedValidator<B> {
    db: BlockchainDatabase<B>,
    aggregate_body_validator: AggregateBodyChainLinkedValidator,
}

impl<B: BlockchainBackend> TransactionChainLinkedValidator<B> {
    pub fn new(db: BlockchainDatabase<B>, consensus_manager: ConsensusManager) -> Self {
        Self {
            aggregate_body_validator: AggregateBodyChainLinkedValidator::new(consensus_manager),
            db,
        }
    }
}

impl<B: BlockchainBackend> TransactionValidator for TransactionChainLinkedValidator<B> {
    fn validate(&self, tx: &Transaction) -> Result<(), ValidationError> {
        let consensus_constants = self.db.consensus_constants()?;
        // validate maximum tx weight
        if tx
            .calculate_weight(consensus_constants.transaction_weight_params())
            .map_err(|e| {
                ValidationError::SerializationError(format!("Unable to calculate the transaction weight: {}", e))
            })? >
            consensus_constants.max_block_weight_excluding_coinbase().map_err(|e| {
                ValidationError::ConsensusError(format!(
                    "Unable to get max block weight from consensus constants: {}",
                    e
                ))
            })?
        {
            return Err(ValidationError::MaxTransactionWeightExceeded);
        }

        {
            let db = self.db.db_read_access()?;
            let tip_height = db.fetch_chain_metadata()?.height_of_longest_chain();
            self.aggregate_body_validator.validate(&tx.body, tip_height, &*db)?;
        };

        Ok(())
    }
}
