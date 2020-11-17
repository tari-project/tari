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
    base_node::sync::BlockHeaderSyncError,
    blocks::BlockHeader,
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend, TargetDifficulties},
    common::rolling_vec::RollingVec,
    consensus::ConsensusManager,
    proof_of_work::PowAlgorithm,
    tari_utilities::{epoch_time::EpochTime, hex::Hex},
    transactions::types::HashOutput,
    validation::helpers::{
        check_header_timestamp_greater_than_median,
        check_pow_data,
        check_target_difficulty,
        check_timestamp_ftl,
    },
};
use log::*;
use std::cmp::Ordering;

const LOG_TARGET: &str = "c::bn::header_sync";

#[derive(Clone)]
pub struct BlockHeaderSyncValidator<B> {
    db: AsyncBlockchainDb<B>,
    state: Option<State>,
    consensus_rules: ConsensusManager,
}

#[derive(Debug, Clone)]
struct State {
    current_height: u64,
    timestamps: RollingVec<EpochTime>,
    target_difficulties: TargetDifficulties,
}

impl<B: BlockchainBackend + 'static> BlockHeaderSyncValidator<B> {
    pub fn new(db: AsyncBlockchainDb<B>, consensus_rules: ConsensusManager) -> Self {
        Self {
            db,
            state: None,
            consensus_rules,
        }
    }

    pub async fn initialize_state(&mut self, start_hash: HashOutput) -> Result<(), BlockHeaderSyncError> {
        let start_header = self
            .db
            .fetch_header_by_block_hash(start_hash.clone())
            .await?
            .ok_or_else(|| BlockHeaderSyncError::StartHashNotFound(start_hash.to_hex()))?;
        let timestamps = self.db.fetch_block_timestamps(start_hash.clone()).await?;
        let target_difficulties = self.db.fetch_target_difficulties(start_hash).await?;
        debug!(
            target: LOG_TARGET,
            "Setting header validator state ({} timestamp(s), target difficulties: {} SHA3, {} Monero)",
            timestamps.len(),
            target_difficulties.get(PowAlgorithm::Sha3).len(),
            target_difficulties.get(PowAlgorithm::Monero).len(),
        );
        self.state = Some(State {
            current_height: start_header.height,
            timestamps,
            target_difficulties,
        });

        Ok(())
    }

    pub fn validate(&mut self, header: &BlockHeader) -> Result<(), BlockHeaderSyncError> {
        let expected_height = self.state().current_height + 1;
        if header.height != expected_height {
            return Err(BlockHeaderSyncError::InvalidBlockHeight(expected_height, header.height));
        }
        check_timestamp_ftl(header, &self.consensus_rules)?;

        let state = self.state();
        check_header_timestamp_greater_than_median(header, &state.timestamps)?;

        let target_difficulty = state.target_difficulties.get(header.pow_algo()).calculate();
        check_target_difficulty(header, target_difficulty)?;
        check_pow_data(header, &self.consensus_rules, &*self.db.inner().db_read_access()?)?;

        // Header is valid, add this header onto the validation state for the next round
        let state = self.state_mut();

        // Ensure that timestamps are inserted in sorted order
        let maybe_index = state.timestamps.iter().position(|ts| ts >= &header.timestamp());
        match maybe_index {
            Some(pos) => {
                state.timestamps.insert(pos, header.timestamp());
            },
            None => state.timestamps.push(header.timestamp()),
        }

        state.current_height = header.height;
        // Add a "more recent" datapoint onto the target difficulty
        state.target_difficulties.add_back(header);

        Ok(())
    }

    pub async fn check_stronger_chain(&mut self, their_header: &BlockHeader) -> Result<(), BlockHeaderSyncError> {
        // Compare their header to ours at the same height, or if we don't have a header at that height, our current tip
        // header
        let our_header = match self.db.fetch_header(their_header.height).await? {
            Some(h) => h,
            None => self.db.fetch_tip_header().await?,
        };

        debug!(
            target: LOG_TARGET,
            "Comparing PoW on remote header #{} and local header #{}", their_header.height, our_header.height
        );

        match self
            .consensus_rules
            .chain_strength_comparer()
            .compare(&our_header, their_header)
        {
            Ordering::Less => Ok(()),
            Ordering::Equal | Ordering::Greater => Err(BlockHeaderSyncError::WeakerChain),
        }
    }

    fn state_mut(&mut self) -> &mut State {
        self.state
            .as_mut()
            .expect("state_mut() called before state was initialized (using the `begin` method)")
    }

    fn state(&self) -> &State {
        self.state
            .as_ref()
            .expect("state() called before state was initialized (using the `begin` method)")
    }
}
