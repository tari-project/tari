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

use std::{collections::HashMap, convert::TryFrom};

use tari_node_components::blocks::BlockHeader;
use tari_transaction_components::{
    consensus::ConsensusConstants,
    tari_proof_of_work::{Difficulty, PowAlgorithm},
};
use tari_utilities::epoch_time::EpochTime;

use crate::{
    consensus::BaseNodeConsensusManager,
    proof_of_work::{PowBackoffTracker, TargetDifficultyWindow, pow_backoff::check_pow_backoff_cap},
};

#[derive(Debug, Clone)]
pub struct TargetDifficulties {
    algos: HashMap<PowAlgorithm, TargetDifficultyWindow>,
    /// Tracks the trailing run of same-algorithm blocks so that the TIP-RFC-MT-0004 backoff modifier can be derived
    /// from the headers alone.
    tracker: PowBackoffTracker,
    /// The backoff cap in force at the height these difficulties are being calculated for. `1` disables the backoff.
    pow_backoff_cap: u64,
}

impl TargetDifficulties {
    pub fn new(consensus_rules: &BaseNodeConsensusManager, height: u64) -> Result<Self, String> {
        let consensus_constants = consensus_rules.consensus_constants(height);
        let permitted_algos = consensus_constants.current_permitted_pow_algos();
        let mut algos = HashMap::new();
        for algo in permitted_algos {
            let target_difficulty_window = consensus_rules.new_target_difficulty(algo, height)?;
            algos.insert(algo, target_difficulty_window);
        }
        let pow_backoff_cap = consensus_constants.pow_backoff_cap();
        check_pow_backoff_cap(pow_backoff_cap)?;
        Ok(Self {
            algos,
            tracker: PowBackoffTracker::new(),
            pow_backoff_cap,
        })
    }

    pub fn update_algos(&mut self, consensus_rules: &BaseNodeConsensusManager, height: u64) -> Result<(), String> {
        let consensus_constants = consensus_rules.consensus_constants(height);
        let permitted_algos = consensus_constants.current_permitted_pow_algos();
        let block_window = usize::try_from(consensus_constants.difficulty_block_window())
            .map_err(|e| format!("difficulty block window exceeds usize::MAX: {e}"))?;
        check_pow_backoff_cap(consensus_constants.pow_backoff_cap())?;
        self.pow_backoff_cap = consensus_constants.pow_backoff_cap();
        let current_keys: Vec<PowAlgorithm> = self.algos.keys().copied().collect();
        for algo in current_keys {
            if !permitted_algos.contains(&algo) {
                self.algos.remove(&algo);
            }
        }
        for algo in permitted_algos {
            if let std::collections::hash_map::Entry::Vacant(e) = self.algos.entry(algo) {
                let target_difficulty_window = consensus_rules.new_target_difficulty(algo, height)?;
                e.insert(target_difficulty_window);
            } else if let Some(target_diff) = self.algos.get_mut(&algo) {
                target_diff.update_target_time(consensus_constants.pow_target_block_interval(algo))?;
                // The LWMA window can shrink at a hard fork while this struct is live (header sync), so drop the
                // oldest data points that no longer fit.
                target_diff.update_block_window(block_window)?;
            } else {
                // clippy, this else should never be hit
            }
        }
        self.refresh_next_modifiers();
        Ok(())
    }

    /// Appends a header's target difficulty to the window of its PoW algorithm and advances the backoff tracker.
    ///
    /// `constants` must be the consensus constants in force at the *header's* height, not at the height these target
    /// difficulties are being calculated for. They differ for the blocks straddling the activation height, and using
    /// the wrong ones would normalise a pre-fork block by a modifier that was never in force for it.
    pub fn add_back(
        &mut self,
        header: &BlockHeader,
        target_difficulty: Difficulty,
        constants: &ConsensusConstants,
    ) -> Result<(), String> {
        self.add_back_parts(header.pow_algo(), header.timestamp(), target_difficulty, constants)
    }

    /// As [`TargetDifficulties::add_back`], but taking the header's parts rather than the header itself.
    pub fn add_back_parts(
        &mut self,
        algo: PowAlgorithm,
        timestamp: EpochTime,
        target_difficulty: Difficulty,
        constants: &ConsensusConstants,
    ) -> Result<(), String> {
        let modifier = self.tracker.modifier_for(algo, constants.pow_backoff_cap());
        let adjusted_target = crate::proof_of_work::adjust_target(
            target_difficulty,
            modifier,
            constants.min_pow_difficulty(algo),
            constants.max_pow_difficulty(algo),
        );
        self.get_mut(algo)?
            .add_back(timestamp, target_difficulty, adjusted_target);
        self.tracker.push(algo);
        self.refresh_next_modifiers();
        Ok(())
    }

    /// Advances the backoff tracker without adding a data point to any window. This is used for the headers of the
    /// lookback that sits before the start of the LWMA window(s).
    pub fn push_algo(&mut self, algo: PowAlgorithm) {
        self.tracker.push(algo);
        self.refresh_next_modifiers();
    }

    /// The backoff modifier a block of `algo` would pay if it were appended next.
    pub fn next_modifier(&self, algo: PowAlgorithm) -> u64 {
        self.tracker.modifier_for(algo, self.pow_backoff_cap)
    }

    /// The backoff cap in force at the height these target difficulties are being calculated for.
    pub fn pow_backoff_cap(&self) -> u64 {
        self.pow_backoff_cap
    }

    fn refresh_next_modifiers(&mut self) {
        let tracker = &self.tracker;
        let cap = self.pow_backoff_cap;
        for (algo, window) in &mut self.algos {
            window.set_next_modifier(tracker.modifier_for(*algo, cap));
        }
    }

    pub fn is_algo_full(&self, algo: PowAlgorithm) -> Result<bool, String> {
        Ok(self.get(algo)?.is_full())
    }

    pub fn is_full(&self) -> bool {
        let mut result = true;
        for algo in self.algos.values() {
            result = result && algo.is_full();
        }
        result
    }

    pub fn get(&self, algo: PowAlgorithm) -> Result<&TargetDifficultyWindow, String> {
        self.algos.get(&algo).ok_or("Algorithm not found".to_string())
    }

    fn get_mut(&mut self, algo: PowAlgorithm) -> Result<&mut TargetDifficultyWindow, String> {
        self.algos.get_mut(&algo).ok_or("Algorithm not found".to_string())
    }

    pub fn algo_count(&self) -> usize {
        self.algos.len()
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashMap as StdHashMap;

    use tari_common::configuration::Network;
    use tari_common_types::types::{FixedHash, HashOutput};
    use tari_node_components::blocks::{BlockHeaderAccumulatedData, ChainHeader};
    use tari_transaction_components::{
        consensus::{
            ConsensusConstantsBuilder,
            consensus_constants::{POW_BACKOFF_CAP, POW_BACKOFF_DISABLED, PowAlgorithmConstants},
        },
        tari_proof_of_work::ProofOfWork,
    };
    use tari_utilities::epoch_time::EpochTime;

    use super::*;
    use crate::{
        chain_storage::{
            ChainStorageError,
            blockchain_database::{
                ChainHeaderSource,
                target_difficulties_for_next_block,
                target_difficulty_for_next_block,
            },
        },
        proof_of_work::{MAX_BACKOFF_RUN_LOOKBACK, MAX_POW_BACKOFF_MODIFIER, TargetDifficultyWindow},
    };

    const TARGET_TIME: u64 = 240;
    const MIN_DIFFICULTY: u64 = 100;
    const MAX_DIFFICULTY: u64 = 1_000_000;

    /// Consensus rules with the backoff enabled and a difficulty range wide enough for it to bite.
    fn rules(block_window: u64, cap: u64) -> BaseNodeConsensusManager {
        let mut builder = ConsensusConstantsBuilder::new(Network::LocalNet)
            .clear_proof_of_work()
            .with_difficulty_block_window(block_window)
            .with_pow_backoff_cap(cap);
        for algo in [
            PowAlgorithm::Sha3x,
            PowAlgorithm::RandomXM,
            PowAlgorithm::RandomXT,
            PowAlgorithm::Cuckaroo,
        ] {
            builder = builder.add_proof_of_work(algo, PowAlgorithmConstants {
                min_difficulty: Difficulty::from_u64(MIN_DIFFICULTY).unwrap(),
                max_difficulty: Difficulty::from_u64(MAX_DIFFICULTY).unwrap(),
                target_time: TARGET_TIME,
            });
        }
        BaseNodeConsensusManager::builder(Network::LocalNet)
            .add_consensus_constants(builder.build())
            .build()
            .unwrap()
    }

    fn header(algo: PowAlgorithm, timestamp: u64) -> BlockHeader {
        let mut header = BlockHeader::new(0);
        header.pow = ProofOfWork::new(algo);
        header.timestamp = EpochTime::from(timestamp);
        header
    }

    /// An in-memory chain that satisfies the one lookup the difficulty window walk needs, so that the walk and the
    /// incremental header sync path can be compared directly.
    struct MemoryChain {
        headers: StdHashMap<HashOutput, ChainHeader>,
        /// Oldest first
        ordered: Vec<ChainHeader>,
    }

    impl ChainHeaderSource for MemoryChain {
        fn fetch_chain_header(&self, hash: &HashOutput) -> Result<ChainHeader, ChainStorageError> {
            self.headers
                .get(hash)
                .cloned()
                .ok_or_else(|| ChainStorageError::UnexpectedResult(format!("no header for {hash}")))
        }
    }

    impl MemoryChain {
        /// Builds a chain from a list of PoW algorithms, oldest first, starting at height 0.
        ///
        /// Both the target difficulty and the gap between timestamps vary per header. That matters: with a uniform
        /// difficulty and evenly spaced timestamps, `ave_difficulty` and every solve time are identical, so a window
        /// shifted by one entry computes the same target as the correct one and an off-by-one in *which* headers are
        /// selected would go unnoticed. Varying both makes `calculate_pair` a near-injective fingerprint of the
        /// window contents.
        #[allow(clippy::arithmetic_side_effects)]
        fn build(algos: &[PowAlgorithm], base_difficulty: u64) -> Self {
            let mut headers = StdHashMap::new();
            let mut ordered = Vec::new();
            let mut prev_hash = FixedHash::zero();
            let mut timestamp = 1_000_000u64;
            for (i, algo) in algos.iter().enumerate() {
                // Deterministic jitter spanning roughly half to double the target time
                let jitter = ((i as u64).saturating_mul(37)) % (TARGET_TIME.saturating_add(1));
                timestamp += TARGET_TIME / 2 + jitter;
                let mut block_header = header(*algo, timestamp);
                block_header.height = i as u64;
                block_header.prev_hash = prev_hash;
                // Headers differ by height, timestamp and prev_hash, so their hashes are unique
                let hash = block_header.hash();
                let mut accum = BlockHeaderAccumulatedData::genesis(hash, Default::default());
                accum.target_difficulty = Difficulty::from_u64(base_difficulty + i as u64 * 13).unwrap();
                let chain_header = ChainHeader::try_construct(block_header, accum).unwrap();
                headers.insert(hash, chain_header.clone());
                ordered.push(chain_header);
                prev_hash = hash;
            }
            Self { headers, ordered }
        }

        fn tip_hash(&self) -> HashOutput {
            *self.ordered.last().expect("chain is never empty").hash()
        }

        /// Replays the whole chain the way header sync does: one `add_back` per header, in order.
        fn incremental(&self, consensus_rules: &BaseNodeConsensusManager) -> TargetDifficulties {
            let next_height = self.ordered.len() as u64;
            let mut targets = TargetDifficulties::new(consensus_rules, next_height).unwrap();
            for chain_header in &self.ordered {
                targets
                    .add_back(
                        chain_header.header(),
                        chain_header.accumulated_data().target_difficulty,
                        consensus_rules.consensus_constants(chain_header.height()),
                    )
                    .unwrap();
            }
            targets
        }
    }

    fn assert_window_same(algo: PowAlgorithm, a: &TargetDifficultyWindow, b: &TargetDifficultyWindow, what: &str) {
        let min = Difficulty::from_u64(MIN_DIFFICULTY).unwrap();
        let max = Difficulty::from_u64(MAX_DIFFICULTY).unwrap();
        assert_eq!(a.len(), b.len(), "{algo} window length ({what})");
        assert_eq!(a.next_modifier(), b.next_modifier(), "{algo} next modifier ({what})");
        assert_eq!(
            a.calculate_pair(min, max),
            b.calculate_pair(min, max),
            "{algo} calculated target ({what})"
        );
    }

    fn assert_same(consensus_rules: &BaseNodeConsensusManager, a: &TargetDifficulties, b: &TargetDifficulties) {
        assert_eq!(a.algo_count(), b.algo_count());
        for algo in consensus_rules.consensus_constants(0).current_permitted_pow_algos() {
            assert_window_same(
                algo,
                a.get(algo).unwrap(),
                b.get(algo).unwrap(),
                "multi-algo vs incremental",
            );
        }
    }

    /// The single algorithm walk is a *separate* implementation of pass 2 from the multi-algo one, and it is the path
    /// used by `DifficultyCalculator::check_achieved_and_target_difficulty` for every block validation and by the
    /// miner block template. It also stops walking as soon as its one window fills, so it carries the least history
    /// of the two and its lookback is the tight case. It must agree with both other paths.
    fn assert_single_algo_agrees(
        chain: &MemoryChain,
        consensus_rules: &BaseNodeConsensusManager,
        reference: &TargetDifficulties,
    ) {
        for algo in consensus_rules.consensus_constants(0).current_permitted_pow_algos() {
            let single = target_difficulty_for_next_block(chain, consensus_rules, algo, &chain.tip_hash()).unwrap();
            assert_window_same(algo, &single, reference.get(algo).unwrap(), "single-algo vs reference");
        }
    }

    /// The highest value test in this module: a disagreement between the backwards database walk and the incremental
    /// header sync path is a chain split.
    #[test]
    fn the_database_walk_agrees_with_incremental_header_sync() {
        use PowAlgorithm::{Cuckaroo, RandomXM, RandomXT, Sha3x};
        let patterns: Vec<Vec<PowAlgorithm>> = vec![
            // Single algorithm: the maximal backoff case
            vec![Sha3x; 80],
            // Strict round robin: nothing ever pays a penalty
            (0..80)
                .map(|i| *[Sha3x, RandomXM, RandomXT, Cuckaroo].get(i % 4).expect("in range"))
                .collect(),
            // Long Sha3x runs punctuated by other algos
            (0..120).map(|i| if i % 11 == 0 { RandomXM } else { Sha3x }).collect(),
            // Two algos with runs long enough to saturate the cap
            (0..100)
                .map(|i| if (i / 7) % 2 == 0 { Sha3x } else { RandomXT })
                .collect(),
            // Chain shorter than the window
            vec![Sha3x, Sha3x, RandomXM, Sha3x, Cuckaroo, Cuckaroo],
            // Chain barely longer than the lookback
            vec![Sha3x; MAX_BACKOFF_RUN_LOOKBACK + 2],
        ];
        for block_window in [5u64, 10, 45] {
            for pattern in &patterns {
                let chain = MemoryChain::build(pattern, 1_000);
                let consensus_rules = rules(block_window, POW_BACKOFF_CAP);
                let walked = target_difficulties_for_next_block(&chain, &consensus_rules, &chain.tip_hash()).unwrap();
                let incremental = chain.incremental(&consensus_rules);
                assert_same(&consensus_rules, &walked, &incremental);
                // The block validation path must land on the same windows as both of the above
                assert_single_algo_agrees(&chain, &consensus_rules, &incremental);
            }
        }
    }

    #[test]
    fn the_database_walk_agrees_with_incremental_header_sync_pre_fork() {
        // The same equivalence must hold with the backoff disabled, i.e. for every pre-fork chain
        let chain = MemoryChain::build(&[PowAlgorithm::Sha3x; 80], 1_000);
        let consensus_rules = rules(45, POW_BACKOFF_DISABLED);
        let walked = target_difficulties_for_next_block(&chain, &consensus_rules, &chain.tip_hash()).unwrap();
        let incremental = chain.incremental(&consensus_rules);
        assert_same(&consensus_rules, &walked, &incremental);
        assert_single_algo_agrees(&chain, &consensus_rules, &incremental);
        assert_eq!(walked.next_modifier(PowAlgorithm::Sha3x), 1);
    }

    /// The lookback must be deep enough that every window entry whose modifier the LWMA actually *reads* gets the
    /// same modifier the incremental path would give it.
    ///
    /// Note the one header of slack: `raw_difficulty` skips the front (oldest) entry, using each entry's modifier
    /// only to normalise the gap it closes, so the front entry's modifier is never read. The deepest modifier that
    /// matters therefore belongs to the *second* oldest entry, and `MAX_BACKOFF_RUN_LOOKBACK - 1` headers would
    /// already be sufficient. The lookback is kept at `log2(MAX_POW_BACKOFF_MODIFIER)` because that is the bound the
    /// RFC states and it costs one header read. A consequence is that shortening the walk by exactly one header
    /// cannot fail any test; shortening it by two can, and does.
    #[test]
    fn the_lookback_is_deep_enough() {
        use PowAlgorithm::{RandomXM, Sha3x};
        for block_window in [5u64, 45] {
            let window_len = usize::try_from(block_window).unwrap() + 1;
            for lead_in in 0..=(MAX_BACKOFF_RUN_LOOKBACK + 3) {
                for lead_algo in [Sha3x, RandomXM] {
                    // `lead_in` same-or-other-algo blocks, then a long Sha3x run filling the window. The oldest
                    // *read* entry's run is always at least 6 here and so always capped; what the sweep varies is
                    // the history sitting before the lookback, which is precisely what a walk that does not reach
                    // back far enough would misread. Short runs at that entry are covered by the mixed patterns in
                    // the main equivalence test.
                    let mut algos = vec![RandomXM];
                    algos.extend(vec![lead_algo; lead_in]);
                    algos.extend(vec![Sha3x; window_len + MAX_BACKOFF_RUN_LOOKBACK]);
                    let chain = MemoryChain::build(&algos, 1_000);
                    let consensus_rules = rules(block_window, POW_BACKOFF_CAP);
                    let walked =
                        target_difficulties_for_next_block(&chain, &consensus_rules, &chain.tip_hash()).unwrap();
                    let incremental = chain.incremental(&consensus_rules);
                    assert_same(&consensus_rules, &walked, &incremental);
                    assert_single_algo_agrees(&chain, &consensus_rules, &incremental);
                }
            }
        }
    }

    /// A run that reaches back past the lookback is capped, which is precisely why a bounded lookback is exact.
    #[test]
    fn a_run_longer_than_the_lookback_is_capped() {
        use PowAlgorithm::{RandomXM, Sha3x};
        let mut tracker = PowBackoffTracker::new();
        tracker.push(RandomXM);
        for _ in 0..(MAX_BACKOFF_RUN_LOOKBACK - 1) {
            tracker.push(Sha3x);
        }
        // Run of exactly MAX_BACKOFF_RUN_LOOKBACK => one short of the cap
        assert_eq!(
            tracker.modifier_for(Sha3x, POW_BACKOFF_CAP),
            MAX_POW_BACKOFF_MODIFIER / 2
        );
        tracker.push(Sha3x);
        assert_eq!(tracker.modifier_for(Sha3x, POW_BACKOFF_CAP), MAX_POW_BACKOFF_MODIFIER);
        // ... and stays capped no matter how much deeper the run goes
        for _ in 0..50 {
            tracker.push(Sha3x);
            assert_eq!(tracker.modifier_for(Sha3x, POW_BACKOFF_CAP), MAX_POW_BACKOFF_MODIFIER);
        }
    }

    #[test]
    fn it_tracks_the_backoff_run_across_algos() {
        let consensus_rules = rules(45, POW_BACKOFF_CAP);
        let constants = consensus_rules.consensus_constants(0).clone();
        let mut targets = TargetDifficulties::new(&consensus_rules, 1).unwrap();

        // Nothing added yet, so nothing pays a penalty
        for algo in constants.current_permitted_pow_algos() {
            assert_eq!(targets.next_modifier(algo), 1);
        }

        let difficulty = Difficulty::from_u64(1_000).unwrap();
        targets
            .add_back(&header(PowAlgorithm::Sha3x, 100), difficulty, &constants)
            .unwrap();
        assert_eq!(targets.next_modifier(PowAlgorithm::Sha3x), 2);
        assert_eq!(targets.next_modifier(PowAlgorithm::RandomXM), 1);
        assert_eq!(targets.get(PowAlgorithm::Sha3x).unwrap().next_modifier(), 2);
        assert_eq!(targets.get(PowAlgorithm::RandomXM).unwrap().next_modifier(), 1);

        targets
            .add_back(&header(PowAlgorithm::Sha3x, 200), difficulty, &constants)
            .unwrap();
        assert_eq!(targets.next_modifier(PowAlgorithm::Sha3x), 4);

        // A different algo resets the Sha3x run
        targets
            .add_back(&header(PowAlgorithm::RandomXM, 300), difficulty, &constants)
            .unwrap();
        assert_eq!(targets.next_modifier(PowAlgorithm::Sha3x), 1);
        assert_eq!(targets.next_modifier(PowAlgorithm::RandomXM), 2);

        // Headers outside the windows still advance the run
        targets.push_algo(PowAlgorithm::RandomXM);
        assert_eq!(targets.next_modifier(PowAlgorithm::RandomXM), 4);

        assert_eq!(targets.get(PowAlgorithm::Sha3x).unwrap().len(), 2);
        assert_eq!(targets.get(PowAlgorithm::RandomXM).unwrap().len(), 1);
    }

    /// The fork boundary: the window shrinks 90 -> 45 and the cap goes 1 -> 32 while a header sync
    /// `TargetDifficulties` is live.
    #[test]
    fn update_algos_applies_the_fork_boundary_mid_window() {
        let pre_fork = rules(90, POW_BACKOFF_DISABLED);
        let post_fork = rules(45, POW_BACKOFF_CAP);
        let pre_constants = pre_fork.consensus_constants(0).clone();
        let post_constants = post_fork.consensus_constants(0).clone();

        let mut targets = TargetDifficulties::new(&pre_fork, 1).unwrap();
        assert_eq!(targets.pow_backoff_cap(), POW_BACKOFF_DISABLED);

        let mut timestamp = 1_000u64;
        for _ in 0..91 {
            targets
                .add_back(
                    &header(PowAlgorithm::Sha3x, timestamp),
                    Difficulty::from_u64(1_000).unwrap(),
                    &pre_constants,
                )
                .unwrap();
            timestamp += TARGET_TIME;
        }
        assert_eq!(targets.get(PowAlgorithm::Sha3x).unwrap().len(), 91);
        // Backoff disabled pre-fork, so a long Sha3x run still pays nothing
        assert_eq!(targets.next_modifier(PowAlgorithm::Sha3x), 1);

        // Cross the fork
        targets.update_algos(&post_fork, 1).unwrap();
        assert_eq!(targets.pow_backoff_cap(), POW_BACKOFF_CAP);
        assert_eq!(
            targets.get(PowAlgorithm::Sha3x).unwrap().len(),
            46,
            "the window must shrink to the new block window"
        );
        // The run carried over from before the fork, so the first post-fork Sha3x block pays the capped modifier
        assert_eq!(targets.next_modifier(PowAlgorithm::Sha3x), MAX_POW_BACKOFF_MODIFIER);

        // ... and the pre-fork entries are still de-normalised by 1, because that is what was in force for them
        let min = Difficulty::from_u64(MIN_DIFFICULTY).unwrap();
        let max = Difficulty::from_u64(MAX_DIFFICULTY).unwrap();
        let pair = targets.get(PowAlgorithm::Sha3x).unwrap().calculate_pair(min, max);
        assert_eq!(pair.base, Difficulty::from_u64(1_000).unwrap());
        assert_eq!(pair.adjusted, Difficulty::from_u64(32_000).unwrap());

        // A post-fork block records the adjusted target it actually had to clear
        targets
            .add_back(&header(PowAlgorithm::Sha3x, timestamp), pair.base, &post_constants)
            .unwrap();
        assert_eq!(targets.get(PowAlgorithm::Sha3x).unwrap().len(), 46);
    }

    #[test]
    fn it_accepts_only_valid_backoff_caps() {
        for cap in [POW_BACKOFF_DISABLED, 2, 4, 8, 16, POW_BACKOFF_CAP] {
            assert!(TargetDifficulties::new(&rules(45, cap), 1).is_ok(), "{cap}");
        }
        // The builder itself rejects a cap that is not a power of two in range
        assert!(std::panic::catch_unwind(|| rules(45, 3)).is_err());
        assert!(std::panic::catch_unwind(|| rules(45, 64)).is_err());
    }
}
