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
    blocks::BlockHeader,
    chain_storage::{BlockchainBackend, BlockchainDatabase},
    consensus::ConsensusManager,
    validation::{
        helpers::{check_achieved_and_target_difficulty, check_median_timestamp},
        Validation,
        ValidationError,
    },
};
use log::*;
use tari_crypto::tari_utilities::{hex::Hex, Hashable};

const LOG_TARGET: &str = "c::bn::states::horizon_state_sync::headers";

pub struct HeaderValidator<B> {
    rules: ConsensusManager,
    db: BlockchainDatabase<B>,
}

impl<B: BlockchainBackend> HeaderValidator<B> {
    pub fn new(db: BlockchainDatabase<B>, rules: ConsensusManager) -> Self {
        Self { db, rules }
    }
}

impl<B: BlockchainBackend> Validation<BlockHeader> for HeaderValidator<B> {
    fn validate(&self, header: &BlockHeader) -> Result<(), ValidationError> {
        let header_id = format!("header #{} ({})", header.height, header.hash().to_hex());
        self.check_median_timestamp(header)?;
        trace!(
            target: LOG_TARGET,
            "BlockHeader validation: Median timestamp is ok for {} ",
            &header_id
        );
        self.check_achieved_and_target_difficulty(header)?;
        trace!(
            target: LOG_TARGET,
            "BlockHeader validation: Achieved difficulty is ok for {} ",
            &header_id
        );
        debug!(
            target: LOG_TARGET,
            "Block header validation: BlockHeader is VALID for {}", &header_id
        );
        Ok(())
    }
}

impl<B: BlockchainBackend> HeaderValidator<B> {
    pub fn is_genesis(&self, block_header: &BlockHeader) -> bool {
        block_header.height == 0 && self.rules.get_genesis_block_hash() == block_header.hash()
    }

    /// Calculates the achieved and target difficulties at the specified height and compares them.
    pub fn check_achieved_and_target_difficulty(&self, block_header: &BlockHeader) -> Result<(), ValidationError> {
        // We cant use the actual tip, as this is used by the header sync, the tip has not yet been downloaded, but we
        // can assume that the previous header was added, so we use that as the tip.
        let tip_height = block_header.height.saturating_sub(1);
        let db = self.db.db_read_access()?;
        check_achieved_and_target_difficulty(&*db, block_header, tip_height, self.rules.clone())
    }

    /// This function tests that the block timestamp is greater than the median timestamp at the specified height.
    pub fn check_median_timestamp(&self, block_header: &BlockHeader) -> Result<(), ValidationError> {
        // We cant use the actual tip, as this is used by the header sync, the tip has not yet been downloaded, but we
        // can assume that the previous header was added, so we use that as the tip.
        let tip_height = block_header.height.saturating_sub(1);
        let db = self.db.db_read_access()?;
        check_median_timestamp(&*db, block_header, tip_height, self.rules.clone())
    }
}
