// Copyright 2021. The Tari Project
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

use tari_node_components::blocks::BlockHeader;

use crate::{
    chain_storage::{BlockchainBackend, fetch_target_difficulty_for_next_block},
    consensus::BaseNodeConsensusManager,
    proof_of_work::{AchievedTargetDifficulty, randomx_factory::RandomXFactory},
    validation::{ValidationError, helpers::check_target_difficulty},
};

pub const TARI_RX_VM_KEY_BLOCK_SWAP: u64 = 2048;
const TARI_RX_VM_KEY_REORG_SAFETY_NUMBER: u64 = 64;

#[derive(Clone)]
pub struct DifficultyCalculator {
    pub rules: BaseNodeConsensusManager,
    pub randomx_factory: RandomXFactory,
}

impl DifficultyCalculator {
    pub fn new(rules: BaseNodeConsensusManager, randomx_factory: RandomXFactory) -> Self {
        Self { rules, randomx_factory }
    }

    pub fn check_achieved_and_target_difficulty<B: BlockchainBackend>(
        &self,
        db: &B,
        block_header: &BlockHeader,
    ) -> Result<AchievedTargetDifficulty, ValidationError> {
        let difficulty_window =
            fetch_target_difficulty_for_next_block(db, &self.rules, block_header.pow_algo(), &block_header.prev_hash)?;
        let constants = self.rules.consensus_constants(block_header.height);
        // `target` is the unadjusted LWMA target (what accumulates into the total accumulated difficulty), while
        // `adjusted` is the bar this block's proof of work must actually clear.
        let target = difficulty_window.calculate_pair(
            constants.min_pow_difficulty(block_header.pow.pow_algo),
            constants.max_pow_difficulty(block_header.pow.pow_algo),
        );
        let gen_hash = *self.rules.get_genesis_block().hash();
        let vm_key = *db
            .fetch_chain_header_by_height(tari_rx_vm_key_height(block_header.height))?
            .hash();
        let achieved_target = check_target_difficulty(
            block_header,
            target,
            &self.randomx_factory,
            &gen_hash,
            &self.rules,
            vm_key,
        )?;

        Ok(achieved_target)
    }
}

pub fn tari_rx_vm_key_height(height: u64) -> u64 {
    if height <= TARI_RX_VM_KEY_BLOCK_SWAP + TARI_RX_VM_KEY_REORG_SAFETY_NUMBER {
        0
    } else {
        // The guard above proves neither subtraction can underflow.
        height
            .saturating_sub(TARI_RX_VM_KEY_REORG_SAFETY_NUMBER)
            .saturating_sub(1) &
            !TARI_RX_VM_KEY_BLOCK_SWAP.saturating_sub(1)
    }
}

#[cfg(test)]
mod backoff_test {
    use tari_common::configuration::Network;
    use tari_node_components::blocks::{BlockHeader, BlockHeaderValidationError};
    use tari_transaction_components::{
        consensus::{
            ConsensusConstantsBuilder,
            consensus_constants::{POW_BACKOFF_CAP, POW_BACKOFF_DISABLED, PowAlgorithmConstants},
        },
        tari_proof_of_work::{Difficulty, PowAlgorithm, PowError},
    };

    use super::*;
    use crate::{
        consensus::BaseNodeConsensusManager,
        proof_of_work::sha3x_difficulty,
        test_helpers::blockchain::create_custom_blockchain,
        validation::ValidationError,
    };

    fn rules(cap: u64) -> BaseNodeConsensusManager {
        let constants = ConsensusConstantsBuilder::new(Network::LocalNet)
            .clear_proof_of_work()
            .with_pow_backoff_cap(cap)
            .add_proof_of_work(PowAlgorithm::Sha3x, PowAlgorithmConstants {
                min_difficulty: Difficulty::min(),
                max_difficulty: Difficulty::from_u64(1_000_000).unwrap(),
                target_time: 240,
            })
            .build();
        BaseNodeConsensusManager::builder(Network::LocalNet)
            .add_consensus_constants(constants)
            .build()
            .unwrap()
    }

    /// Grinds the nonce until the header's Sha3x proof of work satisfies `wanted`.
    fn grind(header: &mut BlockHeader, wanted: impl Fn(Difficulty) -> bool) {
        for _ in 0..200_000 {
            if wanted(sha3x_difficulty(header).unwrap()) {
                return;
            }
            header.nonce = header.nonce.saturating_add(1);
        }
        panic!("could not grind a header with the wanted difficulty");
    }

    /// End to end over the real consensus path: `check_achieved_and_target_difficulty` ->
    /// `fetch_target_difficulty_for_next_block` -> `check_target_difficulty`. LocalNet's genesis is a Sha3x block, so
    /// the block at height 1 is the second consecutive Sha3x block and owes a 2x backoff.
    #[test]
    fn a_second_consecutive_same_algo_block_must_clear_the_adjusted_target() {
        let consensus_rules = rules(POW_BACKOFF_CAP);
        let db = create_custom_blockchain(consensus_rules.clone());
        let genesis = db.fetch_chain_header(0).unwrap();
        let calculator = DifficultyCalculator::new(consensus_rules, RandomXFactory::default());
        let access = db.db_read_access().unwrap();

        // Clears the unadjusted target of 1 but not the 2x adjusted target
        let mut header = BlockHeader::from_previous(genesis.header());
        grind(&mut header, |d| d == Difficulty::min());
        let err = calculator
            .check_achieved_and_target_difficulty(&*access, &header)
            .unwrap_err();
        match err {
            ValidationError::BlockHeaderError(BlockHeaderValidationError::ProofOfWorkError(
                PowError::AchievedDifficultyTooLow { achieved, target },
            )) => {
                assert_eq!(achieved, Difficulty::min());
                assert_eq!(target, Difficulty::from_u64(2).unwrap(), "the backoff adjusted target");
            },
            other => panic!("expected AchievedDifficultyTooLow, got {other:?}"),
        }

        // Clearing the adjusted target is accepted ...
        let mut header = BlockHeader::from_previous(genesis.header());
        grind(&mut header, |d| d >= Difficulty::from_u64(2).unwrap());
        let achieved = calculator
            .check_achieved_and_target_difficulty(&*access, &header)
            .unwrap();
        assert!(achieved.achieved() >= Difficulty::from_u64(2).unwrap());
        assert_eq!(achieved.adjusted_target(), Difficulty::from_u64(2).unwrap());
        // ... and what accumulates is still the unadjusted target
        assert_eq!(achieved.target(), Difficulty::min());
    }

    /// The same header is accepted with the backoff switched off, which pins the rejection above on the backoff
    /// rather than on anything else in the validation path.
    #[test]
    fn the_same_block_is_accepted_before_the_fork() {
        let consensus_rules = rules(POW_BACKOFF_DISABLED);
        let db = create_custom_blockchain(consensus_rules.clone());
        let genesis = db.fetch_chain_header(0).unwrap();
        let calculator = DifficultyCalculator::new(consensus_rules, RandomXFactory::default());
        let access = db.db_read_access().unwrap();

        let mut header = BlockHeader::from_previous(genesis.header());
        grind(&mut header, |d| d == Difficulty::min());
        let achieved = calculator
            .check_achieved_and_target_difficulty(&*access, &header)
            .unwrap();
        assert_eq!(achieved.target(), Difficulty::min());
        assert_eq!(achieved.adjusted_target(), Difficulty::min());
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_tari_vm_key_calc() {
        let height = 0;
        let expected = 0;
        assert_eq!(tari_rx_vm_key_height(height), expected);

        let height = 1000;
        let expected = 0;
        assert_eq!(tari_rx_vm_key_height(height), expected);

        let height = 2047;
        let expected = 0;
        assert_eq!(tari_rx_vm_key_height(height), expected);

        let height = 2048;
        let expected = 0;
        assert_eq!(tari_rx_vm_key_height(height), expected);

        let height = 3048;
        let expected = 2048;
        assert_eq!(tari_rx_vm_key_height(height), expected);

        let height = 4000;
        let expected = 2048;
        assert_eq!(tari_rx_vm_key_height(height), expected);

        let height = 4159;
        let expected = 2048;
        assert_eq!(tari_rx_vm_key_height(height), expected);

        let height = 4160;
        let expected = 2048;
        assert_eq!(tari_rx_vm_key_height(height), expected);

        let height = 4161;
        let expected = 4096;
        assert_eq!(tari_rx_vm_key_height(height), expected);
    }
}
