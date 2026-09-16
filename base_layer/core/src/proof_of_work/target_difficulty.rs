// Copyright 2019. The Tari Project
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

use tari_transaction_components::tari_proof_of_work::{Difficulty, PowAlgorithm};

/// A pair of target difficulties for a single block: the unadjusted LWMA target (which is what accumulates into the
/// total accumulated difficulty) and the same-algorithm-backoff adjusted target (which is the bar the block's proof
/// of work must actually clear).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdjustedTarget {
    /// The unadjusted target difficulty
    pub base: Difficulty,
    /// The target difficulty the proof of work must clear
    pub adjusted: Difficulty,
}

impl AdjustedTarget {
    /// Creates a pair where no backoff applies, i.e. the adjusted target equals the base target.
    pub fn unadjusted(target: Difficulty) -> Self {
        Self {
            base: target,
            adjusted: target,
        }
    }
}

/// Immutable struct that is guaranteed to have achieved the target difficulty
#[derive(Debug, Clone, Copy)]
pub struct AchievedTargetDifficulty {
    pow_algo: PowAlgorithm,
    achieved: Difficulty,
    target: Difficulty,
    adjusted_target: Difficulty,
}

impl AchievedTargetDifficulty {
    /// Checks if the achieved difficulty is higher than the *adjusted* target difficulty. If not, None is returned
    /// because a valid AchievedTargetDifficulty cannot be constructed. The unadjusted `target` is retained because
    /// that is what accumulates into the total accumulated difficulty.
    pub fn try_construct(
        pow_algo: PowAlgorithm,
        target: Difficulty,
        adjusted_target: Difficulty,
        achieved: Difficulty,
    ) -> Option<Self> {
        if achieved < adjusted_target {
            return None;
        }
        Some(Self {
            pow_algo,
            achieved,
            target,
            adjusted_target,
        })
    }

    /// Returns the achieved difficulty
    pub fn achieved(&self) -> Difficulty {
        self.achieved
    }

    /// Returns the unadjusted target difficulty
    pub fn target(&self) -> Difficulty {
        self.target
    }

    /// Returns the target difficulty that the proof of work had to clear, i.e. the unadjusted target with the
    /// same-algorithm backoff modifier applied.
    pub fn adjusted_target(&self) -> Difficulty {
        self.adjusted_target
    }

    /// Returns the PoW algorithm
    pub fn pow_algo(&self) -> PowAlgorithm {
        self.pow_algo
    }
}

#[cfg(test)]
mod test {
    use tari_common::configuration::Network;
    use tari_common_types::types::FixedHash;
    use tari_node_components::blocks::BlockHeaderAccumulatedData;
    use tari_transaction_components::consensus::ConsensusConstants;

    use super::*;
    use crate::blocks::BlockHeaderAccumulatedDataBuilder;

    fn difficulty(d: u64) -> Difficulty {
        Difficulty::from_u64(d).unwrap()
    }

    #[test]
    fn pow_that_clears_only_the_unadjusted_target_is_rejected() {
        let target = difficulty(1_000);
        let adjusted = difficulty(32_000);

        // Clears the unadjusted target but not the backoff adjusted one
        assert!(
            AchievedTargetDifficulty::try_construct(PowAlgorithm::Sha3x, target, adjusted, difficulty(1_000)).is_none()
        );
        assert!(
            AchievedTargetDifficulty::try_construct(PowAlgorithm::Sha3x, target, adjusted, difficulty(31_999))
                .is_none()
        );

        // Exactly on the adjusted target is accepted
        let achieved =
            AchievedTargetDifficulty::try_construct(PowAlgorithm::Sha3x, target, adjusted, difficulty(32_000)).unwrap();
        assert_eq!(achieved.achieved(), difficulty(32_000));
        // ... but the value that accumulates stays the unadjusted target
        assert_eq!(achieved.target(), target);
        assert_eq!(achieved.adjusted_target(), adjusted);
    }

    #[test]
    fn with_no_backoff_the_two_targets_agree() {
        let target = difficulty(1_000);
        assert!(
            AchievedTargetDifficulty::try_construct(PowAlgorithm::Sha3x, target, target, difficulty(999)).is_none()
        );
        let achieved = AchievedTargetDifficulty::try_construct(PowAlgorithm::Sha3x, target, target, target).unwrap();
        assert_eq!(achieved.target(), achieved.adjusted_target());

        let pair = AdjustedTarget::unadjusted(target);
        assert_eq!(pair.base, pair.adjusted);
    }

    /// The backoff must not leak into accumulated difficulty: chain comparison is unchanged by TIP-RFC-MT-0004.
    #[test]
    fn only_the_unadjusted_target_accumulates() {
        let constants = ConsensusConstants::for_network_at_height(Network::LocalNet, 1);
        let previous = BlockHeaderAccumulatedData::genesis(FixedHash::zero(), Default::default());

        let build = |target: Difficulty, adjusted: Difficulty| {
            let achieved = AchievedTargetDifficulty::try_construct(PowAlgorithm::Sha3x, target, adjusted, adjusted)
                .expect("achieved clears the adjusted target");
            BlockHeaderAccumulatedDataBuilder::from_previous(&previous)
                .with_hash(FixedHash::from([1u8; 32]))
                .with_achieved_target_difficulty(achieved)
                .with_total_kernel_offset(Default::default())
                .build(&constants)
                .unwrap()
        };

        let backed_off = build(difficulty(1_000), difficulty(32_000));
        let plain = build(difficulty(1_000), difficulty(1_000));

        // A 32x backoff changes neither the accumulated Sha3x difficulty nor the recorded target difficulty
        assert_eq!(
            backed_off.accumulated_sha3x_difficulty(),
            plain.accumulated_sha3x_difficulty()
        );
        assert_eq!(backed_off.target_difficulty, difficulty(1_000));
        assert_eq!(backed_off.target_difficulty, plain.target_difficulty);
        assert_eq!(
            backed_off.total_accumulated_difficulty,
            plain.total_accumulated_difficulty
        );
        // The achieved difficulty does differ, because the miner really did more work
        assert_eq!(backed_off.achieved_difficulty, difficulty(32_000));
    }
}
