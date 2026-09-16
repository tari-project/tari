// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Exponential same-algorithm proof of work backoff (TIP-RFC-MT-0004).
//!
//! Tari compares competing chain tips by the product of the per-algorithm accumulated difficulties. An algorithm
//! whose difficulty grows much faster than the others therefore produces a larger relative increase per block and
//! can reorg the other algorithms disproportionately often. To counter this, a run of `r` consecutive blocks of the
//! same PoW algorithm multiplies that algorithm's target time by `m = min(2^(r-1), MAX_POW_BACKOFF_MODIFIER)`. Any
//! block of a different algorithm resets the run.
//!
//! All PoW algorithms are independent penalty scopes; `RandomXM` and `RandomXT` are deliberately *not* grouped
//! together.
//!
//! The modifier enters the LWMA only through `target_time`, so it is equivalent to multiplying the computed target
//! difficulty by `m`. No new header fields are needed: the modifier is recomputable from the PoW algorithms of the
//! preceding headers, with at most [`MAX_BACKOFF_RUN_LOOKBACK`] headers of lookback beyond the start of the LWMA
//! window.

use std::cmp::min;

use tari_transaction_components::{consensus::consensus_constants::POW_BACKOFF_CAP, tari_proof_of_work::PowAlgorithm};

/// `M_MAX` - the maximum backoff modifier that a run of same-algorithm blocks can accrue.
pub const MAX_POW_BACKOFF_MODIFIER: u64 = 32;

/// `log2(M_MAX)` - the number of headers of lookback needed (beyond the start of an LWMA window) to derive the
/// modifier of the oldest block in that window exactly. If the entire lookback is a single algorithm the run is at
/// least `MAX_BACKOFF_RUN_LOOKBACK + 1` long and the modifier is capped anyway, so no deeper walk is ever required.
pub const MAX_BACKOFF_RUN_LOOKBACK: usize = MAX_BACKOFF_RUN_EXPONENT as usize;

/// The same value as [`MAX_BACKOFF_RUN_LOOKBACK`], as the exponent type used by `u64::pow`.
const MAX_BACKOFF_RUN_EXPONENT: u32 = MAX_POW_BACKOFF_MODIFIER.ilog2();

// The consensus constants live in a lower level crate and therefore carry their own copy of the cap. The scaled LWMA
// arithmetic is only exact if the two agree, so tie them together at compile time rather than in a unit test.
const _: () = assert!(
    POW_BACKOFF_CAP == MAX_POW_BACKOFF_MODIFIER,
    "consensus POW_BACKOFF_CAP must equal MAX_POW_BACKOFF_MODIFIER"
);
// A power of two keeps `2^r` reachable exactly and bounds the de-normalisation ratio by `MAX_POW_BACKOFF_MODIFIER`.
const _: () = assert!(
    MAX_POW_BACKOFF_MODIFIER.is_power_of_two(),
    "MAX_POW_BACKOFF_MODIFIER must be a power of two"
);
const _: () = assert!(
    2u64.pow(MAX_BACKOFF_RUN_EXPONENT) == MAX_POW_BACKOFF_MODIFIER,
    "MAX_BACKOFF_RUN_LOOKBACK must be log2(MAX_POW_BACKOFF_MODIFIER)"
);

/// Tracks the trailing run of same-algorithm blocks so that the backoff modifier can be derived from headers alone.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PowBackoffTracker {
    last: Option<PowAlgorithm>,
    run_len: u32,
}

impl PowBackoffTracker {
    /// Creates a new, empty tracker.
    pub fn new() -> Self {
        Self::default()
    }

    /// The modifier a block of `algo` would pay when appended to the tracked chain. A `cap` of 1 (or 0) disables the
    /// backoff entirely, which is how the pre-fork behaviour is preserved.
    ///
    /// The result is always capped at [`MAX_POW_BACKOFF_MODIFIER`] regardless of `cap`. That bound is relied upon by
    /// the LWMA: it is what guarantees `adjusted_target / target <= MAX_POW_BACKOFF_MODIFIER`, which in turn keeps the
    /// scaled solve time arithmetic free of overflow. `check_pow_backoff_cap` rejects out of range caps at
    /// construction, so this clamp is only a defence in depth.
    pub fn modifier_for(&self, algo: PowAlgorithm, cap: u64) -> u64 {
        if cap <= 1 {
            return 1;
        }
        match self.last {
            Some(last) if last == algo => min(min(2u64.pow(self.run_len), cap), MAX_POW_BACKOFF_MODIFIER),
            _ => 1,
        }
    }

    /// Appends a block of `algo` to the tracked chain.
    pub fn push(&mut self, algo: PowAlgorithm) {
        if self.last == Some(algo) {
            // Saturate so that `2u64.pow(run_len)` can never overflow. Any run longer than the lookback is capped.
            self.run_len = min(self.run_len.saturating_add(1), MAX_BACKOFF_RUN_EXPONENT);
        } else {
            self.last = Some(algo);
            self.run_len = 1;
        }
    }

    /// The algorithm of the most recently pushed block, if any.
    pub fn last_algo(&self) -> Option<PowAlgorithm> {
        self.last
    }

    /// The length of the trailing run of same-algorithm blocks (saturated at [`MAX_BACKOFF_RUN_LOOKBACK`]).
    pub fn run_len(&self) -> u32 {
        self.run_len
    }
}

/// Validates a `pow_backoff_cap` from the consensus constants. The cap must be a power of two no greater than
/// [`MAX_POW_BACKOFF_MODIFIER`]; `1` means the backoff is disabled.
pub fn check_pow_backoff_cap(cap: u64) -> Result<(), String> {
    if cap == 0 || !cap.is_power_of_two() || cap > MAX_POW_BACKOFF_MODIFIER {
        return Err(format!(
            "pow_backoff_cap must be a power of two in 1..={MAX_POW_BACKOFF_MODIFIER}, but {cap} was given"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    const CAP: u64 = MAX_POW_BACKOFF_MODIFIER;

    #[test]
    fn empty_tracker_has_no_penalty() {
        let tracker = PowBackoffTracker::new();
        for algo in [
            PowAlgorithm::Sha3x,
            PowAlgorithm::RandomXM,
            PowAlgorithm::RandomXT,
            PowAlgorithm::Cuckaroo,
        ] {
            assert_eq!(tracker.modifier_for(algo, CAP), 1);
        }
    }

    #[test]
    fn run_doubles_the_modifier_up_to_the_cap() {
        let mut tracker = PowBackoffTracker::new();
        // The first Sha3x block after nothing pays nothing
        assert_eq!(tracker.modifier_for(PowAlgorithm::Sha3x, CAP), 1);
        tracker.push(PowAlgorithm::Sha3x);
        // r = 2 => 2^1
        assert_eq!(tracker.modifier_for(PowAlgorithm::Sha3x, CAP), 2);
        tracker.push(PowAlgorithm::Sha3x);
        assert_eq!(tracker.modifier_for(PowAlgorithm::Sha3x, CAP), 4);
        tracker.push(PowAlgorithm::Sha3x);
        assert_eq!(tracker.modifier_for(PowAlgorithm::Sha3x, CAP), 8);
        tracker.push(PowAlgorithm::Sha3x);
        assert_eq!(tracker.modifier_for(PowAlgorithm::Sha3x, CAP), 16);
        tracker.push(PowAlgorithm::Sha3x);
        assert_eq!(tracker.modifier_for(PowAlgorithm::Sha3x, CAP), 32);
        // Capped from here on, no overflow no matter how long the run gets
        for _ in 0..1000 {
            tracker.push(PowAlgorithm::Sha3x);
            assert_eq!(tracker.modifier_for(PowAlgorithm::Sha3x, CAP), 32);
        }
    }

    #[test]
    fn a_different_algo_resets_the_run() {
        let mut tracker = PowBackoffTracker::new();
        for _ in 0..10 {
            tracker.push(PowAlgorithm::Sha3x);
        }
        assert_eq!(tracker.modifier_for(PowAlgorithm::Sha3x, CAP), 32);
        // A different algo pays nothing...
        assert_eq!(tracker.modifier_for(PowAlgorithm::RandomXM, CAP), 1);
        tracker.push(PowAlgorithm::RandomXM);
        // ... and resets the Sha3x run
        assert_eq!(tracker.modifier_for(PowAlgorithm::Sha3x, CAP), 1);
        assert_eq!(tracker.modifier_for(PowAlgorithm::RandomXM, CAP), 2);
    }

    #[test]
    fn randomx_variants_are_independent_scopes() {
        let mut tracker = PowBackoffTracker::new();
        tracker.push(PowAlgorithm::RandomXM);
        tracker.push(PowAlgorithm::RandomXM);
        assert_eq!(tracker.modifier_for(PowAlgorithm::RandomXM, CAP), 4);
        // RandomXT is a separate penalty scope, it pays nothing for a RandomXM run
        assert_eq!(tracker.modifier_for(PowAlgorithm::RandomXT, CAP), 1);
        tracker.push(PowAlgorithm::RandomXT);
        assert_eq!(tracker.modifier_for(PowAlgorithm::RandomXM, CAP), 1);
        assert_eq!(tracker.modifier_for(PowAlgorithm::RandomXT, CAP), 2);
    }

    #[test]
    fn cap_of_one_disables_the_backoff() {
        let mut tracker = PowBackoffTracker::new();
        for _ in 0..10 {
            tracker.push(PowAlgorithm::Sha3x);
            assert_eq!(tracker.modifier_for(PowAlgorithm::Sha3x, 1), 1);
            assert_eq!(tracker.modifier_for(PowAlgorithm::Sha3x, 0), 1);
        }
    }

    #[test]
    fn a_lower_cap_clamps_the_modifier() {
        let mut tracker = PowBackoffTracker::new();
        for _ in 0..10 {
            tracker.push(PowAlgorithm::Cuckaroo);
        }
        assert_eq!(tracker.modifier_for(PowAlgorithm::Cuckaroo, 4), 4);
        assert_eq!(tracker.modifier_for(PowAlgorithm::Cuckaroo, 8), 8);
    }

    #[test]
    fn a_short_run_is_not_capped_by_a_large_cap() {
        let mut tracker = PowBackoffTracker::new();
        tracker.push(PowAlgorithm::Cuckaroo);
        assert_eq!(tracker.modifier_for(PowAlgorithm::Cuckaroo, 32), 2);
        assert_eq!(tracker.modifier_for(PowAlgorithm::Cuckaroo, 2), 2);
    }

    #[test]
    fn it_validates_the_configured_cap() {
        for good in [1u64, 2, 4, 8, 16, 32] {
            assert!(check_pow_backoff_cap(good).is_ok(), "{good}");
        }
        for bad in [0u64, 3, 5, 6, 24, 64, u64::MAX] {
            assert!(check_pow_backoff_cap(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_modifier_never_exceeds_the_compile_time_cap() {
        // Even a nonsensical cap that slipped past validation cannot break the LWMA's overflow bound
        let mut tracker = PowBackoffTracker::new();
        for _ in 0..10 {
            tracker.push(PowAlgorithm::Sha3x);
            assert!(tracker.modifier_for(PowAlgorithm::Sha3x, u64::MAX) <= MAX_POW_BACKOFF_MODIFIER);
            assert!(tracker.modifier_for(PowAlgorithm::Sha3x, 1024) <= MAX_POW_BACKOFF_MODIFIER);
        }
    }

    #[test]
    fn activation_matches_the_agreed_network_rollout() {
        use tari_common::configuration::Network;
        use tari_transaction_components::consensus::{
            ConsensusConstants,
            consensus_constants::{
                ESMERALDA_TIP004_ACTIVATION_HEIGHT,
                IGOR_TIP004_ACTIVATION_HEIGHT,
                MAINNET_TIP004_ACTIVATION_HEIGHT,
                NEXTNET_TIP004_ACTIVATION_HEIGHT,
                POW_BACKOFF_DISABLED,
                STAGENET_TIP004_ACTIVATION_HEIGHT,
                TIP004_DIFFICULTY_BLOCK_WINDOW,
            },
        };

        // LocalNet is ephemeral, so it activates from height 0. Note that LocalNet also sets
        // `min_difficulty == max_difficulty == 1`, which makes the backoff a no-op there.
        let localnet = ConsensusConstants::for_network_at_height(Network::LocalNet, 0);
        assert_eq!(localnet.pow_backoff_cap(), MAX_POW_BACKOFF_MODIFIER);
        assert_eq!(localnet.difficulty_block_window(), TIP004_DIFFICULTY_BLOCK_WINDOW);

        // Every network with live history keeps its current rules until its activation height, so that historical
        // blocks keep recomputing to the target recorded in their accumulated data.
        for (network, activation) in [
            (Network::MainNet, MAINNET_TIP004_ACTIVATION_HEIGHT),
            (Network::StageNet, STAGENET_TIP004_ACTIVATION_HEIGHT),
            (Network::NextNet, NEXTNET_TIP004_ACTIVATION_HEIGHT),
            (Network::Esmeralda, ESMERALDA_TIP004_ACTIVATION_HEIGHT),
            (Network::Igor, IGOR_TIP004_ACTIVATION_HEIGHT),
        ] {
            let pre_fork = ConsensusConstants::for_network_at_height(network, 0);
            assert_eq!(
                pre_fork.pow_backoff_cap(),
                POW_BACKOFF_DISABLED,
                "{network} at height 0"
            );
            assert_eq!(pre_fork.difficulty_block_window(), 90, "{network} at height 0");

            let at_fork = ConsensusConstants::for_network_at_height(network, activation);
            assert_eq!(at_fork.pow_backoff_cap(), MAX_POW_BACKOFF_MODIFIER, "{network} at fork");
            assert_eq!(
                at_fork.difficulty_block_window(),
                TIP004_DIFFICULTY_BLOCK_WINDOW,
                "{network} at fork"
            );
        }
    }

    #[test]
    fn every_configured_cap_is_valid() {
        use tari_common::configuration::Network;
        use tari_transaction_components::consensus::ConsensusConstants;

        for network in [
            Network::LocalNet,
            Network::Igor,
            Network::Esmeralda,
            Network::NextNet,
            Network::StageNet,
            Network::MainNet,
        ] {
            for constants in ConsensusConstants::for_network(network) {
                assert!(
                    check_pow_backoff_cap(constants.pow_backoff_cap()).is_ok(),
                    "{network} has an invalid pow_backoff_cap {}",
                    constants.pow_backoff_cap()
                );
            }
        }
    }

    #[test]
    fn run_len_saturates_at_the_lookback() {
        let mut tracker = PowBackoffTracker::new();
        for _ in 0..100 {
            tracker.push(PowAlgorithm::Sha3x);
        }
        assert_eq!(tracker.run_len(), MAX_BACKOFF_RUN_EXPONENT);
        assert_eq!(tracker.last_algo(), Some(PowAlgorithm::Sha3x));
    }
}
