//  Copyright 2022, The Tari Project
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

use super::{MempoolTransactionValidation, ValidationError};
use crate::{
    chain_storage::{BlockchainBackend, BlockchainDatabase},
    transactions::transaction_components::{OutputType, Transaction},
};

mod acceptance_validator;
use acceptance_validator::validate_acceptance;

mod constitution_validator;
use constitution_validator::validate_constitution;

mod definition_validator;
use definition_validator::validate_definition;

mod update_proposal_validator;
use update_proposal_validator::validate_update_proposal;

mod update_proposal_acceptance_validator;
use update_proposal_acceptance_validator::validate_update_proposal_acceptance;

mod amendment_validator;
use amendment_validator::validate_amendment;

mod checkpoint_validator;
use checkpoint_validator::validate_contract_checkpoint;

mod helpers;

mod error;
pub use error::DanLayerValidationError;

#[cfg(test)]
mod test_helpers;

/// Validator of Digital Asset Network consensus rules.
#[derive(Clone)]
pub struct TxDanLayerValidator<B> {
    db: BlockchainDatabase<B>,
}

impl<B: BlockchainBackend> TxDanLayerValidator<B> {
    pub fn new(db: BlockchainDatabase<B>) -> Self {
        Self { db }
    }
}

impl<B: BlockchainBackend> MempoolTransactionValidation for TxDanLayerValidator<B> {
    fn validate(&self, tx: &Transaction) -> Result<(), ValidationError> {
        for output in tx.body().outputs() {
            match output.features.output_type {
                OutputType::ContractDefinition => validate_definition(&self.db, output)?,
                OutputType::ContractConstitution => validate_constitution(&self.db, output)?,
                OutputType::ContractValidatorAcceptance => validate_acceptance(&self.db, output)?,
                OutputType::ContractCheckpoint => validate_contract_checkpoint(&self.db, output)?,
                OutputType::ContractConstitutionProposal => validate_update_proposal(&self.db, output)?,
                OutputType::ContractConstitutionChangeAcceptance => {
                    validate_update_proposal_acceptance(&self.db, output)?
                },
                OutputType::ContractAmendment => validate_amendment(&self.db, output)?,
                _ => continue,
            }
        }

        Ok(())
    }
}
