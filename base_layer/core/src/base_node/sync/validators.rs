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
    chain_storage::{BlockchainBackend, BlockchainDatabase},
    consensus::ConsensusManager,
    transactions::types::CryptoFactories,
    validation::{
        block_validators::BlockValidator,
        CandidateBlockBodyValidation,
        ChainBalanceValidator,
        FinalHeaderStateValidation,
    },
};
use std::{fmt, sync::Arc};

#[derive(Clone)]
pub struct SyncValidators<B: BlockchainBackend> {
    pub block_body: Arc<dyn CandidateBlockBodyValidation<B>>,
    pub final_state: Arc<dyn FinalHeaderStateValidation>,
}

impl<B: BlockchainBackend + 'static> SyncValidators<B> {
    pub fn new<TBody, TFinal>(block_body: TBody, final_state: TFinal) -> Self
    where
        TBody: CandidateBlockBodyValidation<B> + 'static,
        TFinal: FinalHeaderStateValidation + 'static,
    {
        Self {
            block_body: Arc::new(block_body),
            final_state: Arc::new(final_state),
        }
    }

    pub fn full_consensus(db: BlockchainDatabase<B>, rules: ConsensusManager, factories: CryptoFactories) -> Self {
        Self::new(
            BlockValidator::new(rules.clone(), factories.clone()),
            ChainBalanceValidator::new(db, rules, factories),
        )
    }
}

impl<B: BlockchainBackend> fmt::Debug for SyncValidators<B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HorizonHeaderValidators")
            .field("header", &"...")
            .field("final_state", &"...")
            .finish()
    }
}
