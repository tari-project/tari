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

use std::{collections::HashMap, convert::TryFrom, sync::Arc};

#[cfg(not(test))]
use chrono::Duration;
use chrono::{self, DateTime, Utc};
use minotari_node_grpc_client::grpc;
use tari_common_types::types::FixedHash;
use tari_core::proof_of_work::monero_rx::FixedByteArray;
use tokio::sync::RwLock;
use tracing::trace;

use crate::{
    block_template_protocol::{FinalBlockTemplateData, NewBlockTemplateData},
    error::MmProxyError,
};

const LOG_TARGET: &str = "minotari_mm_proxy::xmrig";

/// Structure for holding hashmap of hashes -> [BlockRepositoryItem] and [TemplateRepositoryItem].
#[derive(Debug, Clone)]
pub struct BlockTemplateRepository {
    blocks: Arc<RwLock<HashMap<Vec<u8>, BlockRepositoryItem>>>,
    templates: Arc<RwLock<HashMap<Vec<u8>, TemplateRepositoryItem>>>,
}

/// Structure holding [NewBlockTemplate] along with a timestamp.
#[derive(Debug, Clone)]
pub struct TemplateRepositoryItem {
    pub new_block_template: NewBlockTemplateData,
    pub template_with_coinbase: grpc::NewBlockTemplate,
    datetime: DateTime<Utc>,
}

impl TemplateRepositoryItem {
    /// Create new [Self] with current time in UTC.
    pub fn new(new_block_template: NewBlockTemplateData, template_with_coinbase: grpc::NewBlockTemplate) -> Self {
        Self {
            new_block_template,
            template_with_coinbase,
            datetime: Utc::now(),
        }
    }

    /// Get the timestamp of creation.
    pub fn datetime(&self) -> DateTime<Utc> {
        self.datetime
    }
}

/// Structure holding [FinalBlockTemplateData] along with a timestamp.
#[derive(Debug, Clone)]
pub struct BlockRepositoryItem {
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
            blocks: Arc::new(RwLock::new(HashMap::new())),
            templates: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Return [BlockTemplateData] with the associated hash. None if the hash is not stored.
    pub async fn get_final_template<T: AsRef<[u8]>>(&self, merge_mining_hash: T) -> Option<FinalBlockTemplateData> {
        let b = self.blocks.read().await;
        b.get(merge_mining_hash.as_ref()).map(|item| {
            trace!(
                target: LOG_TARGET,
                "Retrieving block template at height #{} with merge mining hash: {:?}",
                item.data.clone().template.new_block_template.header.unwrap_or_default().height,
                hex::encode(merge_mining_hash.as_ref())
            );
            item.data.clone()
        })
    }

    /// Return [BlockTemplateData] with the associated hash. None if the hash is not stored.
    pub async fn get_new_template<T: AsRef<[u8]>>(
        &self,
        best_block_hash: T,
    ) -> Option<(NewBlockTemplateData, grpc::NewBlockTemplate)> {
        let b = self.templates.read().await;
        b.get(best_block_hash.as_ref())
            .map(|item| (item.new_block_template.clone(), item.template_with_coinbase.clone()))
    }

    /// Store [FinalBlockTemplateData] at the hash value if the key does not exist.
    pub async fn save_final_block_template_if_key_unique(
        &self,
        merge_mining_hash: Vec<u8>,
        block_template: FinalBlockTemplateData,
    ) {
        let mut b = self.blocks.write().await;
        b.entry(merge_mining_hash.clone()).or_insert_with(|| {
            trace!(
                target: LOG_TARGET,
                "Saving final block template with merge mining hash: {:?}",
                hex::encode(&merge_mining_hash)
            );
            BlockRepositoryItem::new(block_template)
        });
    }

    /// Store [NewBlockTemplate] at the hash value if the key does not exist.
    pub async fn save_new_block_template_if_key_unique(
        &self,
        best_block_hash: Vec<u8>,
        new_block_template: NewBlockTemplateData,
        template_with_coinbase: grpc::NewBlockTemplate,
    ) {
        let mut b = self.templates.write().await;
        b.entry(best_block_hash.clone()).or_insert_with(|| {
            trace!(
                target: LOG_TARGET,
                "Saving new block template for best block hash: {:?}",
                hex::encode(&best_block_hash)
            );
            TemplateRepositoryItem::new(new_block_template, template_with_coinbase)
        });
    }

    /// Check if the repository contains a block template with best_previous_block_hash
    pub async fn blocks_contains(&self, current_best_block_hash: FixedHash) -> Option<FinalBlockTemplateData> {
        let b = self.blocks.read().await;
        b.values()
            .find(|item| {
                let header = item.data.template.new_block_template.header.clone().unwrap_or_default();
                FixedHash::try_from(header.prev_hash).unwrap_or(FixedHash::default()) == current_best_block_hash
            })
            .map(|val| val.data.clone())
    }

    /// Remove any data that is older than 20 minutes.
    pub async fn remove_outdated(&self) {
        trace!(target: LOG_TARGET, "Removing outdated final block templates");
        let mut b = self.blocks.write().await;
        #[cfg(test)]
        let threshold = Utc::now();
        #[cfg(not(test))]
        let threshold = Utc::now() - Duration::minutes(20);
        *b = b.drain().filter(|(_, i)| i.datetime() >= threshold).collect();
        trace!(target: LOG_TARGET, "Removing outdated new block templates");
        let mut b = self.templates.write().await;
        #[cfg(test)]
        let threshold = Utc::now();
        #[cfg(not(test))]
        let threshold = Utc::now() - Duration::minutes(20);
        *b = b.drain().filter(|(_, i)| i.datetime() >= threshold).collect();
    }

    /// Remove a particularfinla block template for hash and return the associated [BlockRepositoryItem] if any.
    pub async fn remove_final_block_template<T: AsRef<[u8]>>(&self, hash: T) -> Option<BlockRepositoryItem> {
        trace!(
            target: LOG_TARGET,
            "Final block template removed with merge mining hash {:?}",
            hex::encode(hash.as_ref())
        );
        let mut b = self.blocks.write().await;
        b.remove(hash.as_ref())
    }

    /// Remove a particular new block template for hash and return the associated [BlockRepositoryItem] if any.
    pub async fn remove_new_block_template<T: AsRef<[u8]>>(&self, hash: T) -> Option<TemplateRepositoryItem> {
        trace!(
            target: LOG_TARGET,
            "New block template removed with best block hash {:?}",
            hex::encode(hash.as_ref())
        );
        let mut b = self.templates.write().await;
        b.remove(hash.as_ref())
    }
}

/// Setup values for the new block.
#[derive(Clone, Debug)]
pub struct BlockTemplateData {
    pub monero_seed: FixedByteArray,
    pub tari_block: grpc::Block,
    pub tari_miner_data: grpc::MinerData,
    pub monero_difficulty: u64,
    pub tari_difficulty: u64,
    pub tari_merge_mining_hash: FixedHash,
    pub aux_chain_hashes: Vec<monero::Hash>,
    pub new_block_template: grpc::NewBlockTemplate,
}

impl BlockTemplateData {}

/// Builder for the [BlockTemplateData]. All fields have to be set to succeed.
#[derive(Default)]
pub struct BlockTemplateDataBuilder {
    monero_seed: Option<FixedByteArray>,
    tari_block: Option<grpc::Block>,
    tari_miner_data: Option<grpc::MinerData>,
    monero_difficulty: Option<u64>,
    tari_difficulty: Option<u64>,
    tari_merge_mining_hash: Option<FixedHash>,
    aux_chain_hashes: Vec<monero::Hash>,
    new_block_template: Option<grpc::NewBlockTemplate>,
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

    pub fn aux_hashes(mut self, aux_chain_hashes: Vec<monero::Hash>) -> Self {
        self.aux_chain_hashes = aux_chain_hashes;
        self
    }

    pub fn new_block_template(mut self, template: grpc::NewBlockTemplate) -> Self {
        self.new_block_template = Some(template);
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
        let new_block_template = self
            .new_block_template
            .ok_or_else(|| MmProxyError::MissingDataError("new_block_template not provided".to_string()))?;

        Ok(BlockTemplateData {
            monero_seed,
            tari_block,
            tari_miner_data,
            monero_difficulty,
            tari_difficulty,
            tari_merge_mining_hash,
            aux_chain_hashes: self.aux_chain_hashes,
            new_block_template,
        })
    }
}

#[cfg(test)]
pub mod test {
    use std::convert::TryInto;

    use tari_core::{
        blocks::{Block, BlockHeader},
        proof_of_work::Difficulty,
        transactions::aggregated_body::AggregateBody,
    };
    use tari_utilities::ByteArray;

    use super::*;

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
        let new_block_template = grpc::NewBlockTemplate::default();
        let btdb = BlockTemplateDataBuilder::new()
            .monero_seed(FixedByteArray::new())
            .tari_block(block.try_into().unwrap())
            .tari_miner_data(miner_data)
            .monero_difficulty(123456)
            .tari_difficulty(12345)
            .tari_merge_mining_hash(hash)
            .aux_hashes(vec![monero::Hash::from_slice(hash.as_slice())])
            .new_block_template(new_block_template);
        let block_template_data = btdb.build().unwrap();
        FinalBlockTemplateData {
            template: block_template_data,
            target_difficulty: Difficulty::from_u64(12345).unwrap(),
            blockhashing_blob: "no blockhashing_blob data".to_string(),
            blocktemplate_blob: "no blocktemplate_blob data".to_string(),
            aux_chain_hashes: vec![monero::Hash::from_slice(hash.as_slice())],
            aux_chain_mr: hash.to_vec(),
        }
    }

    #[tokio::test]
    async fn test_block_template_repository() {
        let btr = BlockTemplateRepository::new();
        let hash1 = vec![1; 32];
        let hash2 = vec![2; 32];
        let hash3 = vec![3; 32];
        let block_template = create_block_template_data();
        btr.save_final_block_template_if_key_unique(hash1.clone(), block_template.clone())
            .await;
        btr.save_final_block_template_if_key_unique(hash2.clone(), block_template)
            .await;
        assert!(btr.get_final_template(hash1.clone()).await.is_some());
        assert!(btr.get_final_template(hash2.clone()).await.is_some());
        assert!(btr.get_final_template(hash3.clone()).await.is_none());
        assert!(btr.remove_final_block_template(hash1.clone()).await.is_some());
        assert!(btr.get_final_template(hash1.clone()).await.is_none());
        assert!(btr.get_final_template(hash2.clone()).await.is_some());
        assert!(btr.get_final_template(hash3.clone()).await.is_none());
        btr.remove_outdated().await;
        assert!(btr.get_final_template(hash1).await.is_none());
        assert!(btr.get_final_template(hash2).await.is_none());
        assert!(btr.get_final_template(hash3).await.is_none());
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
            .tari_miner_data(miner_data.clone());
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
