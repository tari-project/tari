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
use std::cmp::Ordering;

use log::*;
use primitive_types::U512;
use tari_common_types::types::{FixedHash, HashOutput};
use tari_node_components::blocks::{BlockHeader, BlockHeaderAccumulatedData, BlockHeaderValidationError, ChainHeader};
use tari_transaction_components::tari_proof_of_work::PowAlgorithm;
use tari_utilities::{epoch_time::EpochTime, hex::Hex};

use crate::{
    base_node::sync::{BlockHeaderSyncError, header_sync::HEADER_SYNC_INITIAL_MAX_HEADERS},
    blocks::BlockHeaderAccumulatedDataBuilder,
    chain_storage::{BlockchainBackend, ChainStorageError, TargetDifficulties, async_db::AsyncBlockchainDb},
    common::rolling_vec::RollingVec,
    consensus::BaseNodeConsensusManager,
    proof_of_work::randomx_factory::RandomXFactory,
    validation::{
        DifficultyCalculator,
        HeaderChainLinkedValidator,
        TARI_RX_VM_KEY_BLOCK_SWAP,
        ValidationError,
        header::HeaderFullValidator,
        tari_rx_vm_key_height,
    },
};

const LOG_TARGET: &str = "c::bn::header_sync";

#[derive(Clone)]
pub struct BlockHeaderSyncValidator<B> {
    db: AsyncBlockchainDb<B>,
    state: Option<State>,
    consensus_rules: BaseNodeConsensusManager,
    validator: HeaderFullValidator,
}

#[derive(Debug, Clone)]
struct State {
    current_height: u64,
    timestamps: RollingVec<EpochTime>,
    target_difficulties: TargetDifficulties,
    previous_accum: BlockHeaderAccumulatedData,
    previous_header: BlockHeader,
    valid_headers: Vec<ChainHeader>,
    /// The Tari RandomX VM key boundaries (`height`, `hash`) that are known to belong to the chain currently being
    /// validated. Seeded in [`BlockHeaderSyncValidator::initialize_state`] with the boundaries at or below the chain
    /// split (which both chains agree on) and extended in [`BlockHeaderSyncValidator::validate`] with every boundary
    /// the peer's chain has proven.
    vm_key: Vec<(u64, FixedHash)>,
    /// The height of the chain split this sync started from. The local database is only a trustworthy source of VM
    /// keys at or below this height; above it, it still holds the chain that is about to be rewound.
    chain_split_height: u64,
}

impl<B: BlockchainBackend + 'static> BlockHeaderSyncValidator<B> {
    pub fn new(
        db: AsyncBlockchainDb<B>,
        consensus_rules: BaseNodeConsensusManager,
        randomx_factory: RandomXFactory,
    ) -> Self {
        let difficulty_calculator = DifficultyCalculator::new(consensus_rules.clone(), randomx_factory);
        let validator = HeaderFullValidator::new(consensus_rules.clone(), difficulty_calculator);
        Self {
            db,
            state: None,
            consensus_rules,
            validator,
        }
    }

    #[allow(clippy::ptr_arg)]
    pub async fn initialize_state(&mut self, start_hash: &HashOutput) -> Result<(), BlockHeaderSyncError> {
        let start_header = self
            .db
            .fetch_header_by_block_hash(*start_hash)
            .await?
            .ok_or_else(|| BlockHeaderSyncError::StartHashNotFound(start_hash.to_hex()))?;
        let timestamps = self.db.fetch_block_timestamps(*start_hash).await?;
        let target_difficulties = self.db.fetch_target_difficulties_for_next_block(*start_hash).await?;
        let previous_accum = self
            .db
            .fetch_header_accumulated_data(*start_hash)
            .await?
            .ok_or_else(|| ChainStorageError::ValueNotFound {
                entity: "BlockHeaderAccumulatedData",
                field: "hash",
                value: start_hash.to_hex(),
            })?;
        debug!(
            target: LOG_TARGET,
            "Setting header validator state ({} timestamp(s), target difficulties: {} SHA3, {} Monero RandomX, {} Tari RandomX)",
            timestamps.len(),
            target_difficulties.get(PowAlgorithm::Sha3x).map(|t| t.len()).unwrap_or(0),
            target_difficulties.get(PowAlgorithm::RandomXM).map(|t| t.len()).unwrap_or(0),
            target_difficulties.get(PowAlgorithm::RandomXT).map(|t| t.len()).unwrap_or(0),
        );

        let gen_hash = *self.consensus_rules.get_genesis_block().hash();
        // `start_hash` is the chain split: the last block this node and the sync peer agree on.
        let chain_split_height = start_header.height;

        // Seed the Tari RandomX VM key cache with the band boundaries that both chains agree on.
        //
        // Header sync validates every header *before* `switch_to_pending_chain` rewinds the local chain, so for the
        // whole of validation the database still returns this node's own (possibly losing) fork. Only blocks at or
        // below the chain split are common to both chains, so those are the only VM keys that may be taken from the
        // database. Boundaries above the split are filled in by `validate` as the peer's headers are proven.
        //
        // Only the boundaries this sync can actually consult are seeded: the lowest VM key height any header above
        // the split can ask for is `tari_rx_vm_key_height(chain_split_height + 1)`, and VM key heights are
        // non-decreasing in header height, so the loop below runs a small, constant number of times.
        let mut vm_key = vec![(0, gen_hash)];
        let mut key_height = tari_rx_vm_key_height(chain_split_height.saturating_add(1));
        while key_height <= chain_split_height {
            if key_height > 0 {
                let hash = *self.db.fetch_chain_header(key_height).await?.hash();
                vm_key.push((key_height, hash));
            }
            key_height = key_height.saturating_add(TARI_RX_VM_KEY_BLOCK_SWAP);
        }

        self.state = Some(State {
            current_height: start_header.height,
            timestamps,
            target_difficulties,
            previous_accum,
            previous_header: start_header,
            // One large allocation is usually better even if it is not always used.
            valid_headers: Vec::with_capacity(HEADER_SYNC_INITIAL_MAX_HEADERS),
            vm_key,
            chain_split_height,
        });

        Ok(())
    }

    pub fn current_valid_chain_tip_header(&self) -> Option<&ChainHeader> {
        self.valid_headers().last()
    }

    pub async fn validate(&mut self, header: BlockHeader) -> Result<U512, BlockHeaderSyncError> {
        let constants = self.consensus_rules.consensus_constants(header.height).clone();
        if constants.effective_from_height() == header.height &&
            let Some(&mut ref mut mut_state) = self.state.as_mut()
        {
            // We need to update the target difficulties for the new algorithm
            mut_state
                .target_difficulties
                .update_algos(&self.consensus_rules, header.height)
                .map_err(BlockHeaderSyncError::TargetDifficultiesError)?;
        }

        let state = self.state();

        // The unadjusted target accumulates into the total accumulated difficulty and feeds the LWMA window; the
        // adjusted target is the bar this header's proof of work must clear (TIP-RFC-MT-0004).
        let target_difficulty = state
            .target_difficulties
            .get(header.pow_algo())
            .map_err(BlockHeaderSyncError::TargetDifficultiesError)?
            .calculate_pair(
                constants.min_pow_difficulty(header.pow_algo()),
                constants.max_pow_difficulty(header.pow_algo()),
            );

        let result = {
            let txn = self.db.inner().db_read_access()?;
            let vm_key_height = tari_rx_vm_key_height(header.height);
            // The Tari RandomX VM key must come from the chain we are validating, never from the chain in the
            // database. Header sync runs entirely before the reorg rewind in `switch_to_pending_chain`, so on a
            // forking node `fetch_chain_header_by_height` still returns the fork that is about to be discarded. A VM
            // key taken from there makes every `PowAlgorithm::RandomXT` header of the honest chain hash to garbage,
            // fail the difficulty check, and get the honest peer banned.
            //
            // `state.vm_key` is authoritative and is therefore consulted first: it is seeded in `initialize_state`
            // with the boundaries at or below the chain split (common to both chains) and extended with every
            // boundary this sync has proven, which covers every boundary above the split because headers are
            // validated in order from the split and a VM key height always lags the header height.
            let cached_vm_key = state
                .vm_key
                .iter()
                .find(|(height, _)| *height == vm_key_height)
                .map(|(_, hash)| *hash);
            let vm_key = match cached_vm_key {
                Some(hash) => hash,
                // At or below the chain split the database is, by definition, an ancestor of the split block and so
                // agrees with the peer's chain. This is a belt-and-braces fallback; `initialize_state` should
                // already have seeded these.
                None if vm_key_height <= state.chain_split_height => {
                    *txn.fetch_chain_header_by_height(vm_key_height)?.hash()
                },
                // Above the split the database is the wrong chain, so failing is the only safe answer. This is not a
                // peer offence, and `ChainStorageError::UnexpectedResult` carries no ban reason.
                None => {
                    let split_height = state.chain_split_height;
                    return Err(ChainStorageError::UnexpectedResult(format!(
                        "Tari RandomX VM key at height {vm_key_height} is above the chain split at height \
                         {split_height} and has not been validated during this sync"
                    ))
                    .into());
                },
            };
            self.validator.validate(
                &*txn,
                &header,
                &state.previous_header,
                &state.timestamps,
                Some(target_difficulty),
                vm_key,
            )
        };
        let achieved_target = match result {
            Ok(achieved_target) => achieved_target,
            // future timelimit validation can succeed at a later time. As the block is not yet valid, we discard it
            // for now and ban the peer, but wont blacklist the block.
            Err(e @ ValidationError::BlockHeaderError(BlockHeaderValidationError::InvalidTimestampFutureTimeLimit)) => {
                return Err(e.into());
            },
            // We dont want to mark a block as bad for internal failures
            Err(
                e @ ValidationError::FatalStorageError(_) |
                e @ ValidationError::IncorrectNumberOfTimestampsProvided { .. },
            ) => return Err(e.into()),
            // We dont have to mark the block twice
            Err(e @ ValidationError::BadBlockFound { .. }) => return Err(e.into()),

            Err(e) => {
                let mut txn = self.db.write_transaction();
                txn.insert_bad_block(header.hash(), header.height, e.to_string());
                txn.commit().await?;
                return Err(e.into());
            },
        };

        // Header is valid, add this header onto the validation state for the next round
        // Mutable borrow done later in the function to allow multiple immutable borrows before this line. This has
        // nothing to do with locking or concurrency.
        let state = self.state_mut();
        state.previous_header = header.clone();

        // Ensure that timestamps are inserted in sorted order
        let maybe_index = state.timestamps.iter().position(|ts| *ts >= header.timestamp());
        match maybe_index {
            Some(pos) => {
                state.timestamps.insert(pos, header.timestamp());
            },
            None => {
                state.timestamps.push(header.timestamp());
            },
        }

        state.current_height = header.height;
        // Add a "more recent" datapoint onto the target difficulty
        state
            .target_difficulties
            .add_back(&header, target_difficulty.base, &constants)
            .map_err(ChainStorageError::UnexpectedResult)?;

        let accumulated_data = BlockHeaderAccumulatedDataBuilder::from_previous(&state.previous_accum)
            .with_hash(header.hash())
            .with_achieved_target_difficulty(achieved_target)
            .with_total_kernel_offset(header.total_kernel_offset.clone())
            .build(&constants)?;

        let total_accumulated_difficulty = accumulated_data.total_accumulated_difficulty;
        // NOTE: accumulated_data constructed from header so they are guaranteed to correspond
        let chain_header = ChainHeader::try_construct(header, accumulated_data).unwrap();

        state.previous_accum = chain_header.accumulated_data().clone();
        if chain_header.header().height.is_multiple_of(TARI_RX_VM_KEY_BLOCK_SWAP) {
            // we need to save the hash of this header and height
            state.vm_key.push((chain_header.header().height, *chain_header.hash()));
        }
        state.valid_headers.push(chain_header);

        Ok(total_accumulated_difficulty)
    }

    /// Drains and returns all the headers that were validated.
    ///
    /// ## Panics
    ///
    /// Panics if initialize_state was not called prior to calling this function
    pub fn take_valid_headers(&mut self) -> Vec<ChainHeader> {
        std::mem::take(&mut self.state_mut().valid_headers)
    }

    /// Returns a slice containing the current valid headers
    ///
    /// ## Panics
    ///
    /// Panics if initialize_state was not called prior to calling this function
    pub fn valid_headers(&self) -> &[ChainHeader] {
        &self.state().valid_headers
    }

    pub fn compare_chains(&self, our_header: &ChainHeader, their_header: &ChainHeader) -> Ordering {
        debug!(
            target: LOG_TARGET,
            "Comparing PoW on remote header #{} and local header #{}",
            their_header.height(),
            our_header.height()
        );

        self.consensus_rules
            .chain_strength_comparer()
            .compare(our_header, their_header)
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
    // Overflow in test code panics, which is the desired failure mode for a test.
    #![allow(clippy::arithmetic_side_effects)]
    use tari_common::configuration::Network;
    use tari_test_utils::unpack_enum;
    use tari_transaction_components::tari_proof_of_work::PowAlgorithm;

    use super::*;
    use crate::test_helpers::blockchain::{TempDatabase, create_new_blockchain};

    fn setup() -> (
        BlockHeaderSyncValidator<TempDatabase>,
        AsyncBlockchainDb<TempDatabase>,
        BaseNodeConsensusManager,
    ) {
        let rules = BaseNodeConsensusManager::builder(Network::LocalNet).build().unwrap();
        let randomx_factory = RandomXFactory::default();
        let db = create_new_blockchain();
        (
            BlockHeaderSyncValidator::new(db.clone().into(), rules.clone(), randomx_factory),
            db.into(),
            rules,
        )
    }

    async fn setup_with_headers(
        n: usize,
    ) -> (
        BlockHeaderSyncValidator<TempDatabase>,
        AsyncBlockchainDb<TempDatabase>,
        ChainHeader,
    ) {
        let (validator, db, cm) = setup();
        let mut tip = db.fetch_tip_header().await.unwrap();
        for _ in 0..n {
            let mut header = BlockHeader::from_previous(tip.header());
            header.version = cm.consensus_constants(header.height).blockchain_version().into();
            // Needed to have unique keys for the blockchain db mmr count indexes (MDB_KEY_EXIST error)
            header.kernel_mmr_size = header.kernel_mmr_size.saturating_add(1);
            header.output_smt_size = header.output_smt_size.saturating_add(1);
            let acc_data = BlockHeaderAccumulatedData::genesis(header.hash(), header.total_kernel_offset.clone());

            let chain_header = ChainHeader::try_construct(header.clone(), acc_data.clone()).unwrap();
            db.insert_valid_headers(vec![chain_header.clone()]).await.unwrap();
            tip = chain_header;
        }

        (validator, db, tip)
    }

    mod initialize_state {
        use std::convert::TryInto;

        use super::*;

        #[tokio::test]
        async fn it_initializes_state_to_given_header() {
            let (mut validator, _, tip) = setup_with_headers(1).await;
            validator.initialize_state(&tip.header().hash()).await.unwrap();
            let state = validator.state();
            assert!(state.valid_headers.is_empty());
            assert_eq!(state.target_difficulties.get(PowAlgorithm::Sha3x).unwrap().len(), 2);
            assert!(
                state
                    .target_difficulties
                    .get(PowAlgorithm::RandomXM)
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(state.timestamps.len(), 2);
            assert_eq!(state.current_height, 1);
        }

        #[tokio::test]
        async fn it_errors_if_hash_does_not_exist() {
            let (mut validator, _, _cm) = setup();
            let start_hash = vec![0; 32];
            let err = validator
                .initialize_state(&start_hash.clone().try_into().unwrap())
                .await
                .unwrap_err();
            unpack_enum!(BlockHeaderSyncError::StartHashNotFound(hash) = err);
            assert_eq!(hash, start_hash.to_hex());
        }
    }

    mod validate {
        use super::*;

        #[tokio::test]
        async fn it_passes_if_headers_are_valid() {
            let (mut validator, _, tip) = setup_with_headers(1).await;
            validator.initialize_state(tip.hash()).await.unwrap();
            assert!(validator.valid_headers().is_empty());
            let mut next = BlockHeader::from_previous(tip.header());
            next.timestamp = tip.header().timestamp.checked_add(EpochTime::from(1)).unwrap();
            validator.validate(next).await.unwrap();
            assert_eq!(validator.valid_headers().len(), 1);
            let tip = validator.valid_headers().last().cloned().unwrap();
            let mut next = BlockHeader::from_previous(tip.header());
            next.timestamp = tip.header().timestamp.checked_add(EpochTime::from(1)).unwrap();
            validator.validate(next).await.unwrap();
            assert_eq!(validator.valid_headers().len(), 2);
        }

        #[tokio::test]
        async fn it_fails_if_height_is_not_serial() {
            let (mut validator, _, tip) = setup_with_headers(12).await;
            validator.initialize_state(tip.hash()).await.unwrap();
            let mut next = BlockHeader::from_previous(tip.header());
            next.height = 14;
            let err = validator.validate(next).await.unwrap_err();
            unpack_enum!(BlockHeaderSyncError::ValidationFailed(val_err) = err);
            unpack_enum!(ValidationError::BlockHeaderError(header_err) = val_err);
            unpack_enum!(BlockHeaderValidationError::InvalidHeight { actual, expected } = header_err);
            assert_eq!(actual, 14);
            assert_eq!(expected, 13);
        }
    }

    /// The Tari RandomX VM key must be resolved from the chain that is being validated, never from the chain sitting
    /// in the database.
    ///
    /// Header sync validates every header *before* `switch_to_pending_chain` rewinds, so while validation runs the
    /// database still returns this node's own fork. A fork whose split point falls below a VM key band boundary used
    /// to hand the losing fork's boundary hash to every `RandomXT` header of the honest chain, which then hashed to
    /// garbage, failed the difficulty check and got the honest peer banned.
    mod tari_rx_vm_key {
        // Test-only chain building: heights are small `u64` constants indexed into `Vec`s, and any overflow or
        // out-of-range index is a bug in the test that should panic loudly.
        #![allow(clippy::indexing_slicing, clippy::cast_possible_truncation)]

        use tari_transaction_components::{
            consensus::{ConsensusConstantsBuilder, consensus_constants::PowAlgorithmConstants},
            tari_proof_of_work::{Difficulty, ProofOfWork},
        };

        use super::*;
        use crate::{proof_of_work::tari_randomx_difficulty, test_helpers::blockchain::create_custom_blockchain};

        /// The band boundary the tests fork around.
        const BOUNDARY: u64 = TARI_RX_VM_KEY_BLOCK_SWAP;
        /// The lowest height whose VM key is `BOUNDARY` rather than genesis (see `tari_rx_vm_key_height`).
        const FORK_TIP: u64 = BOUNDARY + 65;
        /// The local chain reaches past the boundary, so the database always has a block at `BOUNDARY` to hand out.
        const LOCAL_TIP: u64 = BOUNDARY + 52;
        /// A split below the boundary: the two chains hold *different* blocks at `BOUNDARY`. This is the broken case.
        const SPLIT_BELOW: u64 = BOUNDARY - 48;
        /// A split above the boundary: both chains share the block at `BOUNDARY`, so nothing can go wrong.
        const SPLIT_ABOVE: u64 = BOUNDARY + 12;

        struct Fork {
            validator: BlockHeaderSyncValidator<TempDatabase>,
            db: AsyncBlockchainDb<TempDatabase>,
            factory: RandomXFactory,
            rules: BaseNodeConsensusManager,
            /// This node's chain. `local[i]` is the header at height `i + 1`.
            local: Vec<BlockHeader>,
            /// The sync peer's chain, from the split (exclusive) up to and including `FORK_TIP`.
            peer: Vec<BlockHeader>,
        }

        impl Fork {
            fn split_hash(&self, split_height: u64) -> FixedHash {
                self.local[(split_height - 1) as usize].hash()
            }

            /// The hash this node's (soon to be rewound) chain holds at the band boundary.
            fn local_boundary(&self) -> FixedHash {
                self.local[(BOUNDARY - 1) as usize].hash()
            }

            /// The hash the peer's chain holds at the band boundary.
            fn peer_boundary(&self, split_height: u64) -> FixedHash {
                self.peer[(BOUNDARY - split_height - 1) as usize].hash()
            }
        }

        fn build_headers(
            rules: &BaseNodeConsensusManager,
            parent: &BlockHeader,
            to_height: u64,
            base_timestamp: u64,
            timestamp_offset: u64,
        ) -> Vec<BlockHeader> {
            let mut headers = Vec::new();
            let mut prev = parent.clone();
            for height in (parent.height + 1)..=to_height {
                let mut header = BlockHeader::from_previous(&prev);
                header.version = rules.consensus_constants(height).blockchain_version().into();
                // Strictly increasing, and comfortably in the past so the future time limit is never hit. The offset
                // is what makes the peer's chain differ from (and hash differently to) this node's chain.
                header.timestamp = EpochTime::from(base_timestamp + height * 10 + timestamp_offset);
                // Needed to have unique keys for the blockchain db mmr count indexes (MDB_KEY_EXIST error)
                header.kernel_mmr_size = prev.kernel_mmr_size + 1;
                header.output_smt_size = prev.output_smt_size + 1;
                headers.push(header.clone());
                prev = header;
            }
            headers
        }

        fn to_chain_headers(headers: &[BlockHeader]) -> Vec<ChainHeader> {
            headers
                .iter()
                .map(|header| {
                    let accum = BlockHeaderAccumulatedData::genesis(header.hash(), header.total_kernel_offset.clone());
                    ChainHeader::try_construct(header.clone(), accum).unwrap()
                })
                .collect()
        }

        /// Grinds the tip's nonce until its Tari RandomX difficulty under the peer's boundary hash and under this
        /// node's boundary hash relate as `wanted` asks. Most randomly keyed hashes land on the lowest difficulty,
        /// so a test may not assume that two different VM keys produce two different difficulties.
        fn grind_apart(
            header: &mut BlockHeader,
            factory: &RandomXFactory,
            peer_key: &FixedHash,
            local_key: &FixedHash,
            wanted: impl Fn(Difficulty, Difficulty) -> bool,
        ) {
            for _ in 0..10_000 {
                let with_peer_key = tari_randomx_difficulty(header, factory, peer_key).unwrap();
                let with_local_key = tari_randomx_difficulty(header, factory, local_key).unwrap();
                if wanted(with_peer_key, with_local_key) {
                    return;
                }
                header.nonce += 1;
            }
            panic!("could not grind a nonce that separates the two VM keys");
        }

        /// Builds a node whose database holds `local` (heights 1..=`LOCAL_TIP`) as its chain, and a competing peer
        /// chain that forks at `split_height` and runs to `FORK_TIP`. Only the local chain is written to the
        /// database - exactly the state header sync validates in, before any rewind has happened.
        ///
        /// The tip of the peer's chain is a `RandomXT` header, the only algorithm that consults the VM key.
        async fn setup_fork(rules: BaseNodeConsensusManager, split_height: u64) -> Fork {
            let factory = RandomXFactory::default();
            let db: AsyncBlockchainDb<TempDatabase> = create_custom_blockchain(rules.clone()).into();
            let genesis = db.fetch_chain_header(0).await.unwrap();
            let base_timestamp = genesis.header().timestamp.as_u64();

            let local = build_headers(&rules, genesis.header(), LOCAL_TIP, base_timestamp, 0);
            db.insert_valid_headers(to_chain_headers(&local)).await.unwrap();

            let mut peer = build_headers(&rules, &local[(split_height - 1) as usize], FORK_TIP, base_timestamp, 5);
            peer.last_mut().unwrap().pow = ProofOfWork::new(PowAlgorithm::RandomXT);

            let validator = BlockHeaderSyncValidator::new(db.clone(), rules.clone(), factory.clone());
            Fork {
                validator,
                db,
                factory,
                rules,
                local,
                peer,
            }
        }

        /// LocalNet pins every difficulty at 1, so a header validates whichever VM key is used. That is fine for the
        /// tests that assert *which* key was used, because the achieved difficulty is recorded in the accumulated
        /// data.
        fn localnet_rules() -> BaseNodeConsensusManager {
            BaseNodeConsensusManager::builder(Network::LocalNet).build().unwrap()
        }

        /// The Tari RandomX target this node's chain must fail to clear and the peer's chain must clear.
        const RANDOMX_T_TARGET: u64 = 16;

        /// LocalNet, but with a Tari RandomX difficulty that a wrongly keyed hash cannot clear, so that picking the
        /// wrong VM key is an outright validation failure rather than a silent difference.
        fn rules_with_a_real_randomx_t_target() -> BaseNodeConsensusManager {
            let target = Difficulty::from_u64(RANDOMX_T_TARGET).unwrap();
            let constants = ConsensusConstantsBuilder::new(Network::LocalNet)
                .clear_proof_of_work()
                .add_proof_of_work(PowAlgorithm::Sha3x, PowAlgorithmConstants {
                    min_difficulty: Difficulty::min(),
                    max_difficulty: Difficulty::min(),
                    target_time: 360,
                })
                .add_proof_of_work(PowAlgorithm::RandomXT, PowAlgorithmConstants {
                    min_difficulty: target,
                    max_difficulty: target,
                    target_time: 360,
                })
                .build();
            BaseNodeConsensusManager::builder(Network::LocalNet)
                .add_consensus_constants(constants)
                .build()
                .unwrap()
        }

        /// Regression test for the VM key being read from the chain that is about to be rewound.
        ///
        /// The split is below the band boundary, so this node's chain and the peer's chain hold different blocks at
        /// `BOUNDARY`. The `RandomXT` header in that band must be hashed under the *peer's* boundary hash; before the
        /// fix `fetch_chain_header_by_height` succeeded and the local (losing) chain's hash won.
        #[tokio::test]
        async fn it_uses_the_peer_chain_vm_key_when_the_split_is_below_the_boundary() {
            let mut fork = setup_fork(localnet_rules(), SPLIT_BELOW).await;
            let local_boundary = fork.local_boundary();
            let peer_boundary = fork.peer_boundary(SPLIT_BELOW);
            assert_ne!(
                local_boundary, peer_boundary,
                "the two chains must disagree at the band boundary for this test to mean anything"
            );

            let mut tip = fork.peer.last().unwrap().clone();
            assert_eq!(tip.pow_algo(), PowAlgorithm::RandomXT);
            assert_eq!(tari_rx_vm_key_height(tip.height), BOUNDARY);
            grind_apart(
                &mut tip,
                &fork.factory,
                &peer_boundary,
                &local_boundary,
                |peer, local| peer != local,
            );
            *fork.peer.last_mut().unwrap() = tip.clone();

            fork.validator
                .initialize_state(&fork.split_hash(SPLIT_BELOW))
                .await
                .unwrap();
            for header in fork.peer.clone() {
                fork.validator.validate(header).await.unwrap();
            }

            let expected = tari_randomx_difficulty(&tip, &fork.factory, &peer_boundary).unwrap();
            let from_the_rewound_chain = tari_randomx_difficulty(&tip, &fork.factory, &local_boundary).unwrap();
            assert_ne!(expected, from_the_rewound_chain);

            let validated = fork.validator.valid_headers().last().unwrap();
            assert_eq!(validated.header().height, FORK_TIP);
            assert_eq!(validated.accumulated_data().achieved_difficulty, expected);
        }

        /// A split at or above the boundary leaves both chains agreeing on the boundary block, so the database and
        /// the cache return the same hash. Nothing about this case may change.
        #[tokio::test]
        async fn it_is_unchanged_when_the_split_is_at_or_above_the_boundary() {
            let mut fork = setup_fork(localnet_rules(), SPLIT_ABOVE).await;
            let boundary = fork.local_boundary();
            assert!(
                fork.peer.iter().all(|h| h.height > BOUNDARY),
                "the peer's chain must start above the boundary, so the boundary block is shared"
            );

            fork.validator
                .initialize_state(&fork.split_hash(SPLIT_ABOVE))
                .await
                .unwrap();
            for header in fork.peer.clone() {
                fork.validator.validate(header).await.unwrap();
            }

            let tip = fork.peer.last().unwrap();
            let expected = tari_randomx_difficulty(tip, &fork.factory, &boundary).unwrap();
            let validated = fork.validator.valid_headers().last().unwrap();
            assert_eq!(validated.header().height, FORK_TIP);
            assert_eq!(validated.accumulated_data().achieved_difficulty, expected);
        }

        /// `initialize_state` seeds the cache from the database with the boundaries at or below the split - the only
        /// ones the two chains are guaranteed to agree on - and with nothing above it.
        #[tokio::test]
        async fn initialize_state_seeds_the_cache_with_the_boundaries_at_or_below_the_split() {
            let mut fork = setup_fork(localnet_rules(), SPLIT_ABOVE).await;
            let gen_hash = *fork.rules.get_genesis_block().hash();
            fork.validator
                .initialize_state(&fork.split_hash(SPLIT_ABOVE))
                .await
                .unwrap();
            assert_eq!(fork.validator.state().chain_split_height, SPLIT_ABOVE);
            assert_eq!(fork.validator.state().vm_key, vec![
                (0, gen_hash),
                (BOUNDARY, fork.local_boundary()),
            ]);

            // Below the split there is no boundary other than genesis to seed: the block at `BOUNDARY` belongs to the
            // contested part of the chain and has to be proven by the peer during the sync.
            let mut fork = setup_fork(localnet_rules(), SPLIT_BELOW).await;
            fork.validator
                .initialize_state(&fork.split_hash(SPLIT_BELOW))
                .await
                .unwrap();
            assert_eq!(fork.validator.state().chain_split_height, SPLIT_BELOW);
            assert_eq!(fork.validator.state().vm_key, vec![(0, gen_hash)]);
        }

        /// An honest peer must never be banned because of a fork that is entirely local.
        ///
        /// The nonce is ground so that the header clears the Tari RandomX target under the peer's VM key and misses
        /// it under this node's, which makes the outcome depend on nothing but which key the validator picks. With
        /// the old lookup this produced a ban-able `ValidationError` and a persisted bad block.
        #[tokio::test]
        async fn an_honest_peer_is_not_banned_because_of_a_local_fork() {
            let mut fork = setup_fork(rules_with_a_real_randomx_t_target(), SPLIT_BELOW).await;
            let local_boundary = fork.local_boundary();
            let peer_boundary = fork.peer_boundary(SPLIT_BELOW);
            let target = Difficulty::from_u64(RANDOMX_T_TARGET).unwrap();

            let mut tip = fork.peer.last().unwrap().clone();
            grind_apart(
                &mut tip,
                &fork.factory,
                &peer_boundary,
                &local_boundary,
                |peer, local| peer >= target && local < target,
            );
            *fork.peer.last_mut().unwrap() = tip.clone();

            fork.validator
                .initialize_state(&fork.split_hash(SPLIT_BELOW))
                .await
                .unwrap();
            for header in fork.peer.clone() {
                let (height, hash) = (header.height, header.hash());
                if let Err(err) = fork.validator.validate(header).await {
                    panic!(
                        "header #{height} from the honest peer was rejected ({err}), ban reason: {:?}",
                        err.get_ban_reason()
                    );
                }
                let (is_bad_block, reason) = fork.db.bad_block_exists(hash).await.unwrap();
                assert!(!is_bad_block, "header #{height} was recorded as a bad block: {reason}");
            }

            // The counterfactual: had the VM key come from the chain that is about to be rewound, this header would
            // have missed the target and the peer would have been banned for it.
            assert!(tari_randomx_difficulty(&tip, &fork.factory, &local_boundary).unwrap() < target);
            assert!(tari_randomx_difficulty(&tip, &fork.factory, &peer_boundary).unwrap() >= target);
        }
    }
}
