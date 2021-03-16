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
    chain_storage::{
        async_db::AsyncBlockchainDb,
        BlockHeaderAccumulatedData,
        BlockHeaderAccumulatedDataBuilder,
        BlockchainBackend,
        ChainHeader,
        ChainStorageError,
        TargetDifficulties,
    },
    common::rolling_vec::RollingVec,
    consensus::ConsensusManager,
    proof_of_work::{randomx_factory::RandomXFactory, PowAlgorithm},
    tari_utilities::{epoch_time::EpochTime, hash::Hashable, hex::Hex},
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
    randomx_factory: RandomXFactory,
}

#[derive(Debug, Clone)]
struct State {
    current_height: u64,
    timestamps: RollingVec<EpochTime>,
    target_difficulties: TargetDifficulties,
    previous_accum: BlockHeaderAccumulatedData,
    valid_headers: Vec<ChainHeader>,
}

impl<B: BlockchainBackend + 'static> BlockHeaderSyncValidator<B> {
    pub fn new(db: AsyncBlockchainDb<B>, consensus_rules: ConsensusManager, randomx_factory: RandomXFactory) -> Self {
        Self {
            db,
            state: None,
            consensus_rules,
            randomx_factory,
        }
    }

    pub async fn initialize_state(&mut self, start_hash: HashOutput) -> Result<(), BlockHeaderSyncError> {
        let start_header = self
            .db
            .fetch_header_by_block_hash(start_hash.clone())
            .await?
            .ok_or_else(|| BlockHeaderSyncError::StartHashNotFound(start_hash.to_hex()))?;
        let timestamps = self.db.fetch_block_timestamps(start_hash.clone()).await?;
        let target_difficulties = self.db.fetch_target_difficulties(start_hash.clone()).await?;
        let previous_accum = self
            .db
            .fetch_header_accumulated_data(start_hash.clone())
            .await?
            .ok_or_else(|| ChainStorageError::ValueNotFound {
                entity: "BlockHeaderAccumulatedData".to_string(),
                field: "hash".to_string(),
                value: start_hash.to_hex(),
            })?;
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
            previous_accum,
            // One large allocation is usually better even if it is not always used.
            valid_headers: Vec::with_capacity(1000),
        });

        Ok(())
    }

    pub fn validate(&mut self, header: BlockHeader) -> Result<(), BlockHeaderSyncError> {
        let state = self.state();
        let expected_height = state.current_height + 1;
        if header.height != expected_height {
            return Err(BlockHeaderSyncError::InvalidBlockHeight {
                expected: expected_height,
                actual: header.height,
            });
        }
        check_timestamp_ftl(&header, &self.consensus_rules)?;

        check_header_timestamp_greater_than_median(&header, &state.timestamps)?;

        let constants = self.consensus_rules.consensus_constants(header.height);
        let target_difficulty = state.target_difficulties.get(header.pow_algo()).calculate(
            constants.min_pow_difficulty(header.pow_algo()),
            constants.max_pow_difficulty(header.pow_algo()),
        );
        let achieved = check_target_difficulty(&header, target_difficulty, &self.randomx_factory)?;
        let metadata = BlockHeaderAccumulatedDataBuilder::default()
            .hash(header.hash())
            .target_difficulty(target_difficulty)
            .achieved_difficulty(&state.previous_accum, header.pow_algo(), achieved)
            .total_kernel_offset(&state.previous_accum.total_kernel_offset, &header.total_kernel_offset)
            .build()?;
        check_pow_data(&header, &self.consensus_rules, &*self.db.inner().db_read_access()?)?;

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
        state.target_difficulties.add_back(&header, target_difficulty);
        state.previous_accum = metadata.clone();
        state.valid_headers.push(ChainHeader {
            header,
            accumulated_data: metadata,
        });

        Ok(())
    }

    /// Drains and returns all the headers that were validated.
    ///
    /// ## Panics
    ///
    /// Panics if initialize_state was not called prior to calling this function
    pub fn take_valid_headers(&mut self) -> Vec<ChainHeader> {
        self.state_mut().valid_headers.drain(..).collect::<Vec<_>>()
    }

    /// Returns a slice containing the current valid headers
    ///
    /// ## Panics
    ///
    /// Panics if initialize_state was not called prior to calling this function
    pub fn valid_headers(&self) -> &[ChainHeader] {
        &self.state().valid_headers
    }

    pub fn check_stronger_chain(
        &self,
        our_header: &ChainHeader,
        their_header: &ChainHeader,
    ) -> Result<(), BlockHeaderSyncError>
    {
        debug!(
            target: LOG_TARGET,
            "Comparing PoW on remote header #{} and local header #{}",
            their_header.height(),
            our_header.height()
        );

        match self
            .consensus_rules
            .chain_strength_comparer()
            .compare(&our_header, their_header)
        {
            Ordering::Less => Ok(()),
            Ordering::Greater | Ordering::Equal => Err(BlockHeaderSyncError::WeakerChain),
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        blocks::BlockHeader,
        chain_storage::{async_db::AsyncBlockchainDb, BlockHeaderAccumulatedData},
        consensus::{ConsensusManager, Network},
        crypto::tari_utilities::{hex::Hex, Hashable},
        proof_of_work::{randomx_factory::RandomXFactory, PowAlgorithm},
        test_helpers::blockchain::{create_new_blockchain, TempDatabase},
    };
    use tari_test_utils::unpack_enum;

    fn setup() -> (BlockHeaderSyncValidator<TempDatabase>, AsyncBlockchainDb<TempDatabase>) {
        let rules = ConsensusManager::builder(Network::LocalNet).build();
        let randomx_factory = RandomXFactory::default();
        let db = create_new_blockchain();
        (
            BlockHeaderSyncValidator::new(db.clone().into(), rules, randomx_factory),
            db.into(),
        )
    }

    async fn setup_with_headers(
        n: usize,
    ) -> (
        BlockHeaderSyncValidator<TempDatabase>,
        AsyncBlockchainDb<TempDatabase>,
        ChainHeader,
    ) {
        let (validator, db) = setup();
        let mut tip = db.fetch_tip_header().await.unwrap();
        for _ in 0..n {
            let mut header = BlockHeader::from_previous(&tip.header).unwrap();
            // Needed to have unique keys for the blockchain db mmr count indexes (MDB_KEY_EXIST error)
            header.kernel_mmr_size += 1;
            header.output_mmr_size += 1;
            let acc_data = BlockHeaderAccumulatedData {
                hash: header.hash(),
                ..Default::default()
            };

            db.insert_valid_headers(vec![(header.clone(), acc_data.clone())])
                .await
                .unwrap();
            tip = ChainHeader {
                header,
                accumulated_data: acc_data,
            };
        }

        (validator, db, tip)
    }

    mod initialize_state {
        use super::*;

        #[tokio_macros::test_basic]
        async fn it_initializes_state_to_given_header() {
            let (mut validator, _, tip) = setup_with_headers(1).await;
            validator.initialize_state(tip.header.hash()).await.unwrap();
            let state = validator.state();
            assert!(state.valid_headers.is_empty());
            assert_eq!(state.target_difficulties.get(PowAlgorithm::Sha3).len(), 2);
            assert!(state.target_difficulties.get(PowAlgorithm::Monero).is_empty());
            assert_eq!(state.timestamps.len(), 2);
            assert_eq!(state.current_height, 1);
        }

        #[tokio_macros::test_basic]
        async fn it_errors_if_hash_does_not_exist() {
            let (mut validator, _) = setup();
            let start_hash = vec![0; 32];
            let err = validator.initialize_state(start_hash.clone()).await.unwrap_err();
            unpack_enum!(BlockHeaderSyncError::StartHashNotFound(hash) = err);
            assert_eq!(hash, start_hash.to_hex());
        }
    }

    mod validate {
        use super::*;

        #[tokio_macros::test_basic]
        async fn it_passes_if_headers_are_valid() {
            let (mut validator, _, tip) = setup_with_headers(1).await;
            validator.initialize_state(tip.header.hash()).await.unwrap();
            assert!(validator.valid_headers().is_empty());
            let next = BlockHeader::from_previous(&tip.header).unwrap();
            validator.validate(next).unwrap();
            assert_eq!(validator.valid_headers().len(), 1);
            let tip = validator.valid_headers().last().cloned().unwrap();
            let next = BlockHeader::from_previous(&tip.header).unwrap();
            validator.validate(next).unwrap();
            assert_eq!(validator.valid_headers().len(), 2);
        }

        #[tokio_macros::test_basic]
        async fn it_fails_if_height_is_not_serial() {
            let (mut validator, _, tip) = setup_with_headers(2).await;
            validator.initialize_state(tip.header.hash()).await.unwrap();
            let mut next = BlockHeader::from_previous(&tip.header).unwrap();
            next.height = 10;
            let err = validator.validate(next).unwrap_err();
            unpack_enum!(BlockHeaderSyncError::InvalidBlockHeight { expected, actual } = err);
            assert_eq!(actual, 10);
            assert_eq!(expected, 3);
        }
    }
}
