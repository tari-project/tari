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

use crate::{
    chain_storage::{BlockchainBackend, BlockchainDatabase},
    proof_of_work::{
        diff_adj_manager::{diff_adj_storage::DiffAdjStorage, error::DiffAdjManagerError},
        Difficulty,
        PowAlgorithm,
    },
};
use std::sync::{Arc, RwLock};

/// The DiffAdjManager is used to calculate the current target difficulty based on PoW recorded in the latest blocks of
/// the current best chain.
pub struct DiffAdjManager<T>
where T: BlockchainBackend
{
    diff_adj_storage: Arc<RwLock<DiffAdjStorage<T>>>,
}

impl<T> DiffAdjManager<T>
where T: BlockchainBackend
{
    /// Constructs a new DiffAdjManager with access to the blockchain db.
    pub fn new(blockchain_db: BlockchainDatabase<T>) -> Result<Self, DiffAdjManagerError> {
        Ok(Self {
            diff_adj_storage: Arc::new(RwLock::new(DiffAdjStorage::new(blockchain_db))),
        })
    }

    /// Returns the estimated target difficulty for the specified PoW algorithm.
    pub fn get_target_difficulty(&self, pow_algo: &PowAlgorithm) -> Result<Difficulty, DiffAdjManagerError> {
        self.diff_adj_storage
            .write()
            .map_err(|_| DiffAdjManagerError::PoisonedAccess)?
            .get_target_difficulty(pow_algo)
    }
}

impl<T> Clone for DiffAdjManager<T>
where T: BlockchainBackend
{
    fn clone(&self) -> Self {
        Self {
            diff_adj_storage: self.diff_adj_storage.clone(),
        }
    }
}

#[cfg(test)]
pub mod test {
    use crate::{
        blocks::genesis_block::get_genesis_block,
        chain_storage::{BlockchainDatabase, MemoryDatabase},
        proof_of_work::{DiffAdjManager, Difficulty, PowAlgorithm},
        test_utils::builders::{add_block_and_update_header, chain_block},
    };
    use tari_transactions::{consensus::TARGET_BLOCK_INTERVAL, types::HashDigest};
    use tari_utilities::epoch_time::EpochTime;

    fn create_test_pow_blockchain(
        store: &BlockchainDatabase<MemoryDatabase<HashDigest>>,
        pow_algos: Vec<PowAlgorithm>,
    )
    {
        let mut prev_block = get_genesis_block();
        prev_block.header.timestamp = EpochTime::from(1575018842);
        prev_block = add_block_and_update_header(&store, prev_block);

        for pow_algo in pow_algos {
            let mut new_block = chain_block(&prev_block, Vec::new());
            new_block.header.timestamp = prev_block.header.timestamp.increase(TARGET_BLOCK_INTERVAL);
            new_block.header.pow.pow_algo = pow_algo;
            prev_block = add_block_and_update_header(&store, new_block);
        }
    }

    fn append_to_pow_blockchain(
        store: &BlockchainDatabase<MemoryDatabase<HashDigest>>,
        append_height: u64,
        pow_algos: Vec<PowAlgorithm>,
    )
    {
        let mut prev_block = store.fetch_block(append_height).unwrap().block().clone();

        for pow_algo in pow_algos {
            let mut new_block = chain_block(&prev_block, Vec::new());
            new_block.header.timestamp = prev_block.header.timestamp.increase(TARGET_BLOCK_INTERVAL);
            new_block.header.pow.pow_algo = pow_algo;
            prev_block = add_block_and_update_header(&store, new_block);
        }
    }

    #[test]
    fn test_initial_sync() {
        let store = BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
        let diff_adj_manager = DiffAdjManager::new(store.clone()).unwrap();
        assert!(diff_adj_manager.get_target_difficulty(&PowAlgorithm::Monero).is_err());
        assert!(diff_adj_manager.get_target_difficulty(&PowAlgorithm::Blake).is_err());

        let pow_algos = vec![
            PowAlgorithm::Blake,
            PowAlgorithm::Monero,
            PowAlgorithm::Blake,
            PowAlgorithm::Blake,
            PowAlgorithm::Monero,
            PowAlgorithm::Monero,
            PowAlgorithm::Blake,
        ];
        create_test_pow_blockchain(&store, pow_algos);
        let diff_adj_manager = DiffAdjManager::new(store.clone()).unwrap();
        assert_eq!(
            diff_adj_manager.get_target_difficulty(&PowAlgorithm::Monero),
            Ok(Difficulty::from(2))
        );
        assert_eq!(
            diff_adj_manager.get_target_difficulty(&PowAlgorithm::Blake),
            Ok(Difficulty::from(8))
        );
    }

    #[test]
    fn test_sync_to_chain_tip() {
        let store = BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
        let diff_adj_manager = DiffAdjManager::new(store.clone()).unwrap();

        let pow_algos = vec![
            PowAlgorithm::Monero,
            PowAlgorithm::Blake,
            PowAlgorithm::Blake,
            PowAlgorithm::Monero,
            PowAlgorithm::Blake,
        ];
        create_test_pow_blockchain(&store, pow_algos);
        assert_eq!(store.get_height(), Ok(Some(5)));
        assert_eq!(
            diff_adj_manager.get_target_difficulty(&PowAlgorithm::Monero),
            Ok(Difficulty::from(1))
        );
        assert_eq!(
            diff_adj_manager.get_target_difficulty(&PowAlgorithm::Blake),
            Ok(Difficulty::from(4))
        );

        let pow_algos = vec![
            PowAlgorithm::Blake,
            PowAlgorithm::Monero,
            PowAlgorithm::Blake,
            PowAlgorithm::Monero,
        ];
        let append_height = store.get_height().unwrap().unwrap();
        append_to_pow_blockchain(&store, append_height, pow_algos);
        assert_eq!(store.get_height(), Ok(Some(9)));
        assert_eq!(
            diff_adj_manager.get_target_difficulty(&PowAlgorithm::Monero),
            Ok(Difficulty::from(1))
        );
        assert_eq!(
            diff_adj_manager.get_target_difficulty(&PowAlgorithm::Blake),
            Ok(Difficulty::from(5))
        );
    }

    #[test]
    #[ignore] // TODO Wait for reorg logic to be refactored
    fn test_full_sync_on_reorg() {
        let store = BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
        let diff_adj_manager = DiffAdjManager::new(store.clone()).unwrap();

        let pow_algos = vec![
            PowAlgorithm::Blake,
            PowAlgorithm::Blake,
            PowAlgorithm::Blake,
            PowAlgorithm::Monero,
        ];
        create_test_pow_blockchain(&store, pow_algos);
        assert_eq!(store.get_height(), Ok(Some(4)));
        assert_eq!(
            diff_adj_manager.get_target_difficulty(&PowAlgorithm::Monero),
            Ok(Difficulty::from(1))
        );
        assert_eq!(
            diff_adj_manager.get_target_difficulty(&PowAlgorithm::Blake),
            Ok(Difficulty::from(18))
        );

        let pow_algos = vec![
            PowAlgorithm::Blake,
            PowAlgorithm::Blake,
            PowAlgorithm::Monero,
            PowAlgorithm::Monero,
            PowAlgorithm::Blake,
            PowAlgorithm::Monero,
            PowAlgorithm::Blake,
            PowAlgorithm::Monero,
        ];
        append_to_pow_blockchain(&store, 2, pow_algos);
        assert_eq!(store.get_height(), Ok(Some(8)));
        assert_eq!(
            diff_adj_manager.get_target_difficulty(&PowAlgorithm::Monero),
            Ok(Difficulty::from(2))
        );
        assert_eq!(
            diff_adj_manager.get_target_difficulty(&PowAlgorithm::Blake),
            Ok(Difficulty::from(9))
        );
    }
}
