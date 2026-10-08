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

//! Provides methods for building template data and storing them with timestamps.

use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

#[cfg(not(test))]
use chrono::Duration;
use chrono::{DateTime, Utc};
use minotari_node_grpc_client::grpc;
use tari_common_types::types::FixedHash;
use tari_core::{AuxChainHashes, proof_of_work::monero_rx::FixedByteArray};
use tokio::sync::RwLock;
use tracing::trace;

use crate::{block_template_manager::FinalBlockTemplateData, error::MmProxyError};

const LOG_TARGET: &str = "minotari_mm_proxy::xmrig";

/// The maximum number of final block templates held at once.
///
/// Every `getblocktemplate` request produces a brand new template - the coinbase is built from a freshly generated
/// random key - so entries accumulate at the request rate, and the dedup in
/// [`BlockTemplateRepository::save_final_block_template_if_key_unique`] never actually dedups. Combined with the
/// time based retention below (a 20 minute window swept every 10 minutes, so up to 30 minutes of residency) the size
/// of the repository was set purely by how fast miners asked for templates, and each entry holds a complete Tari
/// block including all of its mempool transactions.
///
/// A template is only useful until a solution for it is submitted, which a miner does within seconds of receiving
/// it, so this cap is far more than a working miner ever needs: 2048 entries is about 30 minutes of history at one
/// template per second, or three minutes at the 10 per second of a large farm. What it buys is a fixed ceiling -
/// worst case occupancy becomes 2048 blocks rather than `request_rate * 1800s` blocks.
const MAX_BLOCK_TEMPLATES: usize = 2048;

/// Structure for holding hashmap of hashes -> [BlockRepositoryItem] and [TemplateRepositoryItem].
#[derive(Debug, Clone)]
pub(crate) struct BlockTemplateRepository {
    blocks: Arc<RwLock<BlockTemplates>>,
}

/// The stored templates, together with the insertion order needed to evict the oldest once the cap is reached.
#[derive(Debug, Default)]
struct BlockTemplates {
    by_hash: HashMap<Vec<u8>, BlockRepositoryItem>,
    /// The keys of `by_hash` in insertion order. May still contain keys that have since been removed from
    /// `by_hash`; those are simply skipped when evicting, which keeps removal O(1).
    insertion_order: VecDeque<Vec<u8>>,
}

/// Structure holding [FinalBlockTemplateData] along with a timestamp.
#[derive(Debug, Clone)]
pub(crate) struct BlockRepositoryItem {
    pub data: FinalBlockTemplateData,
    datetime: DateTime<Utc>,
}

impl BlockRepositoryItem {
    /// Create new [Self] with current time in UTC.
    pub fn new(final_block: FinalBlockTemplateData) -> Self {
        Self {
            data: final_block,
            datetime: Utc::now(),
        }
    }

    /// Get the timestamp of creation.
    pub fn datetime(&self) -> DateTime<Utc> {
        self.datetime
    }
}

impl BlockTemplateRepository {
    pub fn new() -> Self {
        Self {
            blocks: Arc::new(RwLock::new(BlockTemplates::default())),
        }
    }

    /// Return [BlockTemplateData] with the associated hash. None if the hash is not stored.
    pub async fn get_final_template<T: AsRef<[u8]>>(&self, merge_mining_hash: T) -> Option<FinalBlockTemplateData> {
        let b = self.blocks.read().await;
        b.by_hash.get(merge_mining_hash.as_ref()).map(|item| item.data.clone())
    }

    /// Store [FinalBlockTemplateData] at the hash value if the key does not exist.
    ///
    /// Once [`MAX_BLOCK_TEMPLATES`] are held, the oldest entries are evicted to make room. This is a hard ceiling
    /// on top of the time based [`Self::remove_outdated`] cleanup, not a replacement for it: the time window is
    /// what normally frees templates, and the cap is what stops the request rate from deciding how much memory the
    /// proxy uses.
    pub async fn save_final_block_template_if_key_unique(&self, block_template: FinalBlockTemplateData) {
        let merge_mining_hash = block_template.aux_chain_mr.to_vec();
        let mut b = self.blocks.write().await;
        if b.by_hash.contains_key(&merge_mining_hash) {
            return;
        }
        b.by_hash
            .insert(merge_mining_hash.clone(), BlockRepositoryItem::new(block_template));
        b.insertion_order.push_back(merge_mining_hash);
        while b.insertion_order.len() > MAX_BLOCK_TEMPLATES {
            let Some(oldest) = b.insertion_order.pop_front() else {
                break;
            };
            if b.by_hash.remove(&oldest).is_some() {
                trace!(
                    target: LOG_TARGET,
                    "Final block template with merge mining hash {:?} evicted: more than {} are held",
                    hex::encode(&oldest), MAX_BLOCK_TEMPLATES
                );
            }
        }
    }

    /// Remove any data that is older than 20 minutes.
    pub async fn remove_outdated(&self) {
        trace!(target: LOG_TARGET, "Removing outdated final block templates");
        let mut b = self.blocks.write().await;
        #[cfg(test)]
        let threshold = Utc::now();
        #[cfg(not(test))]
        let threshold = Utc::now()
            .checked_sub_signed(Duration::minutes(20))
            .unwrap_or(chrono::DateTime::<Utc>::MIN_UTC);
        let BlockTemplates {
            by_hash,
            insertion_order,
        } = &mut *b;
        let retained = by_hash.drain().filter(|(_, i)| i.datetime() >= threshold).collect();
        *by_hash = retained;
        insertion_order.retain(|hash| by_hash.contains_key(hash));
    }

    /// Remove a particularfinla block template for hash and return the associated [BlockRepositoryItem] if any.
    pub async fn remove_final_block_template<T: AsRef<[u8]>>(&self, hash: T) -> Option<BlockRepositoryItem> {
        trace!(
            target: LOG_TARGET,
            "Final block template removed with merge mining hash {:?}",
            hex::encode(hash.as_ref())
        );
        let mut b = self.blocks.write().await;
        b.by_hash.remove(hash.as_ref())
    }

    /// The number of final block templates currently held.
    #[cfg(test)]
    pub async fn len(&self) -> usize {
        self.blocks.read().await.by_hash.len()
    }
}

/// Setup values for the new block.
#[derive(Clone, Debug)]
pub(crate) struct BlockTemplateData {
    pub monero_seed: FixedByteArray,
    pub tari_block: grpc::Block,
    pub tari_miner_data: grpc::MinerData,
    pub monero_difficulty: u64,
    pub tari_difficulty: u64,
    pub tari_merge_mining_hash: FixedHash,
    #[allow(dead_code)]
    pub aux_chain_hashes: AuxChainHashes,
}

impl BlockTemplateData {}

/// Builder for the [BlockTemplateData]. All fields have to be set to succeed.
#[derive(Default)]
pub(crate) struct BlockTemplateDataBuilder {
    monero_seed: Option<FixedByteArray>,
    tari_block: Option<grpc::Block>,
    tari_miner_data: Option<grpc::MinerData>,
    monero_difficulty: Option<u64>,
    tari_difficulty: Option<u64>,
    tari_merge_mining_hash: Option<FixedHash>,
    aux_chain_hashes: AuxChainHashes,
}

impl BlockTemplateDataBuilder {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn monero_seed(mut self, monero_seed: FixedByteArray) -> Self {
        self.monero_seed = Some(monero_seed);
        self
    }

    pub fn tari_block(mut self, tari_block: grpc::Block) -> Self {
        self.tari_block = Some(tari_block);
        self
    }

    pub fn tari_miner_data(mut self, miner_data: grpc::MinerData) -> Self {
        self.tari_miner_data = Some(miner_data);
        self
    }

    pub fn monero_difficulty(mut self, difficulty: u64) -> Self {
        self.monero_difficulty = Some(difficulty);
        self
    }

    pub fn tari_difficulty(mut self, difficulty: u64) -> Self {
        self.tari_difficulty = Some(difficulty);
        self
    }

    pub fn tari_merge_mining_hash(mut self, hash: FixedHash) -> Self {
        self.tari_merge_mining_hash = Some(hash);
        self
    }

    pub fn aux_hashes(mut self, aux_chain_hashes: AuxChainHashes) -> Self {
        self.aux_chain_hashes = aux_chain_hashes;
        self
    }

    /// Build a new [BlockTemplateData], all the values have to be set.
    ///
    /// # Errors
    ///
    /// Return error if any of values has not been set.
    pub fn build(self) -> Result<BlockTemplateData, MmProxyError> {
        let monero_seed = self
            .monero_seed
            .ok_or_else(|| MmProxyError::MissingDataError("monero_seed not provided".to_string()))?;
        let tari_block = self
            .tari_block
            .ok_or_else(|| MmProxyError::MissingDataError("block not provided".to_string()))?;
        let tari_miner_data = self
            .tari_miner_data
            .ok_or_else(|| MmProxyError::MissingDataError("miner_data not provided".to_string()))?;
        let monero_difficulty = self
            .monero_difficulty
            .ok_or_else(|| MmProxyError::MissingDataError("monero_difficulty not provided".to_string()))?;
        let tari_difficulty = self
            .tari_difficulty
            .ok_or_else(|| MmProxyError::MissingDataError("tari_difficulty not provided".to_string()))?;
        let tari_merge_mining_hash = self
            .tari_merge_mining_hash
            .ok_or_else(|| MmProxyError::MissingDataError("tari_hash not provided".to_string()))?;
        if self.aux_chain_hashes.is_empty() {
            return Err(MmProxyError::MissingDataError("aux chain hashes are empty".to_string()));
        };

        Ok(BlockTemplateData {
            monero_seed,
            tari_block,
            tari_miner_data,
            monero_difficulty,
            tari_difficulty,
            tari_merge_mining_hash,
            aux_chain_hashes: self.aux_chain_hashes,
        })
    }
}

#[cfg(test)]
mod test {
    use std::convert::{TryFrom, TryInto};

    use tari_node_components::blocks::{Block, BlockHeader};
    use tari_transaction_components::{aggregated_body::AggregateBody, tari_proof_of_work::Difficulty};
    use tari_utilities::ByteArray;

    use super::*;
    use crate::block_template_manager::AuxChainMr;

    fn create_block_template_data() -> FinalBlockTemplateData {
        let header = BlockHeader::new(100);
        let body = AggregateBody::empty();
        let block = Block::new(header, body);
        let hash = block.hash();
        let miner_data = grpc::MinerData {
            reward: 10000,
            target_difficulty: 600000,
            total_fees: 100,
            algo: Some(grpc::PowAlgo { pow_algo: 0 }),
        };
        let btdb = BlockTemplateDataBuilder::new()
            .monero_seed(FixedByteArray::new())
            .tari_block(block.try_into().unwrap())
            .tari_miner_data(miner_data)
            .monero_difficulty(123456)
            .tari_difficulty(12345)
            .tari_merge_mining_hash(hash)
            .aux_hashes(AuxChainHashes::try_from(vec![monero::Hash::from_slice(hash.as_slice())]).unwrap());
        let block_template_data = btdb.build().unwrap();
        FinalBlockTemplateData {
            template: block_template_data,
            target_difficulty: Difficulty::from_u64(12345).unwrap(),
            blockhashing_blob: "no blockhashing_blob data".to_string(),
            blocktemplate_blob: "no blocktemplate_blob data".to_string(),
            aux_chain_hashes: AuxChainHashes::try_from(vec![monero::Hash::from_slice(hash.as_slice())]).unwrap(),
            aux_chain_mr: AuxChainMr::try_from(hash.to_vec()).unwrap(),
        }
    }

    /// Clones `template` with a distinct merge mining hash derived from `key`, so that each one is stored under its
    /// own entry (the way real templates are, each having a freshly generated coinbase).
    fn with_distinct_key(template: &FinalBlockTemplateData, key: u32) -> FinalBlockTemplateData {
        let hash: Vec<u8> = key
            .to_le_bytes()
            .into_iter()
            .chain(std::iter::repeat(0u8))
            .take(32)
            .collect();
        let mut template = template.clone();
        template.aux_chain_mr = AuxChainMr::try_from(hash).unwrap();
        template
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_block_template_repository() {
        let btr = BlockTemplateRepository::new();
        let block_template = create_block_template_data();
        let hash1 = block_template.aux_chain_mr.to_vec();
        btr.save_final_block_template_if_key_unique(block_template.clone())
            .await;
        assert!(btr.get_final_template(hash1.clone()).await.is_some());
        assert!(btr.remove_final_block_template(hash1.clone()).await.is_some());
        assert!(btr.get_final_template(hash1.clone()).await.is_none());
        btr.save_final_block_template_if_key_unique(block_template).await;
        assert!(btr.get_final_template(hash1.clone()).await.is_some());
        btr.remove_outdated().await;
        assert!(btr.get_final_template(hash1).await.is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_block_template_repository_evicts_over_the_count_cap() {
        let btr = BlockTemplateRepository::new();
        let block_template = create_block_template_data();
        let overflow = 8u32;
        let total = u32::try_from(MAX_BLOCK_TEMPLATES).unwrap().saturating_add(overflow);

        for key in 0..total {
            btr.save_final_block_template_if_key_unique(with_distinct_key(&block_template, key))
                .await;
        }

        // Nothing is outdated yet - every template was created moments ago - so without a count cap all of them
        // would still be resident.
        assert_eq!(btr.len().await, MAX_BLOCK_TEMPLATES);

        // The oldest entries were the ones evicted, and the newest were all kept.
        for key in 0..overflow {
            let hash = with_distinct_key(&block_template, key).aux_chain_mr.to_vec();
            assert!(
                btr.get_final_template(hash).await.is_none(),
                "template {key} should have been evicted"
            );
        }
        for key in overflow..total {
            let hash = with_distinct_key(&block_template, key).aux_chain_mr.to_vec();
            assert!(
                btr.get_final_template(hash).await.is_some(),
                "template {key} should have been retained"
            );
        }
    }

    #[test]
    pub fn err_block_template_data_builder() {
        // Empty
        let btdb = BlockTemplateDataBuilder::new();
        assert!(matches!(btdb.build(), Err(MmProxyError::MissingDataError(err)) if err == *"monero_seed not provided"));
        // With monero seed
        let btdb = BlockTemplateDataBuilder::new().monero_seed(FixedByteArray::new());
        assert!(matches!(btdb.build(), Err(MmProxyError::MissingDataError(err)) if err == *"block not provided"));
        // With monero seed, block
        let header = BlockHeader::new(100);
        let body = AggregateBody::empty();
        let block = Block::new(header, body);
        let btdb = BlockTemplateDataBuilder::new()
            .monero_seed(FixedByteArray::new())
            .tari_block(block.clone().try_into().unwrap());
        assert!(matches!(btdb.build(), Err(MmProxyError::MissingDataError(err)) if err == *"miner_data not provided"));
        // With monero seed, block, miner data
        let miner_data = grpc::MinerData {
            reward: 10000,
            target_difficulty: 600000,
            total_fees: 100,
            algo: Some(grpc::PowAlgo { pow_algo: 0 }),
        };
        let btdb = BlockTemplateDataBuilder::new()
            .monero_seed(FixedByteArray::new())
            .tari_block(block.clone().try_into().unwrap())
            .tari_miner_data(miner_data);
        assert!(
            matches!(btdb.build(), Err(MmProxyError::MissingDataError(err)) if err == *"monero_difficulty not provided")
        );
        // With monero seed, block, miner data, monero difficulty
        let btdb = BlockTemplateDataBuilder::new()
            .monero_seed(FixedByteArray::new())
            .tari_block(block.try_into().unwrap())
            .tari_miner_data(miner_data)
            .monero_difficulty(123456);
        assert!(
            matches!(btdb.build(), Err(MmProxyError::MissingDataError(err)) if err == *"tari_difficulty not provided")
        );
    }

    #[test]
    pub fn ok_block_template_data_builder() {
        let build = create_block_template_data();
        assert!(build.template.monero_seed.is_empty());
        assert_eq!(build.template.tari_block.header.unwrap().version, 100);
        assert_eq!(build.template.tari_miner_data.target_difficulty, 600000);
        assert_eq!(build.template.monero_difficulty, 123456);
        assert_eq!(build.template.tari_difficulty, 12345);
        assert_eq!(build.blockhashing_blob, "no blockhashing_blob data".to_string());
        assert_eq!(build.blocktemplate_blob, "no blocktemplate_blob data".to_string());
        assert_eq!(build.target_difficulty, Difficulty::from_u64(12345).unwrap());
    }
}
