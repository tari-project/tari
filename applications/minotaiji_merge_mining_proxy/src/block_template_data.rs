//  Copyright 2020, The Taiji Project
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

//! Provides methods for for building template data and storing them with timestamps.

use std::{collections::HashMap, sync::Arc};

#[cfg(not(test))]
use chrono::Duration;
use chrono::{self, DateTime, Utc};
use minotaiji_node_grpc_client::grpc;
use taiji_core::proof_of_work::monero_rx::FixedByteArray;
use tokio::sync::RwLock;
use tracing::trace;

use crate::error::MmProxyError;

const LOG_TARGET: &str = "minotaiji_mm_proxy::xmrig";

/// Structure for holding hashmap of hashes -> [BlockTemplateRepositoryItem]
#[derive(Debug, Clone)]
pub struct BlockTemplateRepository {
    blocks: Arc<RwLock<HashMap<Vec<u8>, BlockTemplateRepositoryItem>>>,
}

/// Structure holding [BlockTemplateData] along with a timestamp.
#[derive(Debug, Clone)]
pub struct BlockTemplateRepositoryItem {
    pub data: BlockTemplateData,
    datetime: DateTime<Utc>,
}

impl BlockTemplateRepositoryItem {
    /// Create new [Self] with current time in UTC.
    pub fn new(block_template: BlockTemplateData) -> Self {
        Self {
            data: block_template,
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
        }
    }

    /// Return [BlockTemplateData] with the associated hash. None if the hash is not stored.
    pub async fn get<T: AsRef<[u8]>>(&self, hash: T) -> Option<BlockTemplateData> {
        trace!(
            target: LOG_TARGET,
            "Retrieving blocktemplate with merge mining hash: {:?}",
            hex::encode(hash.as_ref())
        );
        let b = self.blocks.read().await;
        b.get(hash.as_ref()).map(|item| item.data.clone())
    }

    /// Store [BlockTemplateData] at the hash value.
    pub async fn save(&self, hash: Vec<u8>, block_template: BlockTemplateData) {
        trace!(
            target: LOG_TARGET,
            "Saving blocktemplate with merge mining hash: {:?}",
            hex::encode(&hash)
        );
        let mut b = self.blocks.write().await;
        let repository_item = BlockTemplateRepositoryItem::new(block_template);
        b.insert(hash, repository_item);
    }

    /// Remove any data that is older than 20 minutes.
    pub async fn remove_outdated(&self) {
        trace!(target: LOG_TARGET, "Removing outdated blocktemplates");
        let mut b = self.blocks.write().await;
        #[cfg(test)]
        let threshold = Utc::now();
        #[cfg(not(test))]
        let threshold = Utc::now() - Duration::minutes(20);
        *b = b.drain().filter(|(_, i)| i.datetime() >= threshold).collect();
    }

    /// Remove a particular hash and return the associated [BlockTemplateRepositoryItem] if any.
    pub async fn remove<T: AsRef<[u8]>>(&self, hash: T) -> Option<BlockTemplateRepositoryItem> {
        trace!(
            target: LOG_TARGET,
            "Blocktemplate removed with merge mining hash {:?}",
            hex::encode(hash.as_ref())
        );
        let mut b = self.blocks.write().await;
        b.remove(hash.as_ref())
    }
}

/// Setup values for the new block.
#[derive(Clone, Debug)]
pub struct BlockTemplateData {
    pub monero_seed: FixedByteArray,
    pub taiji_block: grpc::Block,
    pub taiji_miner_data: grpc::MinerData,
    pub monero_difficulty: u64,
    pub taiji_difficulty: u64,
}

impl BlockTemplateData {}

/// Builder for the [BlockTemplateData]. All fields have to be set to succeed.
#[derive(Default)]
pub struct BlockTemplateDataBuilder {
    monero_seed: Option<FixedByteArray>,
    taiji_block: Option<grpc::Block>,
    taiji_miner_data: Option<grpc::MinerData>,
    monero_difficulty: Option<u64>,
    taiji_difficulty: Option<u64>,
}

impl BlockTemplateDataBuilder {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn monero_seed(mut self, monero_seed: FixedByteArray) -> Self {
        self.monero_seed = Some(monero_seed);
        self
    }

    pub fn taiji_block(mut self, taiji_block: grpc::Block) -> Self {
        self.taiji_block = Some(taiji_block);
        self
    }

    pub fn taiji_miner_data(mut self, miner_data: grpc::MinerData) -> Self {
        self.taiji_miner_data = Some(miner_data);
        self
    }

    pub fn monero_difficulty(mut self, difficulty: u64) -> Self {
        self.monero_difficulty = Some(difficulty);
        self
    }

    pub fn taiji_difficulty(mut self, difficulty: u64) -> Self {
        self.taiji_difficulty = Some(difficulty);
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
        let taiji_block = self
            .taiji_block
            .ok_or_else(|| MmProxyError::MissingDataError("block not provided".to_string()))?;
        let taiji_miner_data = self
            .taiji_miner_data
            .ok_or_else(|| MmProxyError::MissingDataError("miner_data not provided".to_string()))?;
        let monero_difficulty = self
            .monero_difficulty
            .ok_or_else(|| MmProxyError::MissingDataError("monero_difficulty not provided".to_string()))?;
        let taiji_difficulty = self
            .taiji_difficulty
            .ok_or_else(|| MmProxyError::MissingDataError("taiji_difficulty not provided".to_string()))?;

        Ok(BlockTemplateData {
            monero_seed,
            taiji_block,
            taiji_miner_data,
            monero_difficulty,
            taiji_difficulty,
        })
    }
}

#[cfg(test)]
pub mod test {
    use std::convert::TryInto;

    use taiji_core::{
        blocks::{Block, BlockHeader},
        transactions::aggregated_body::AggregateBody,
    };

    use super::*;

    fn create_block_template_data() -> BlockTemplateData {
        let header = BlockHeader::new(100);
        let body = AggregateBody::empty();
        let block = Block::new(header, body);
        let miner_data = grpc::MinerData {
            reward: 10000,
            target_difficulty: 600000,
            total_fees: 100,
            algo: Some(grpc::PowAlgo { pow_algo: 0 }),
        };
        let btdb = BlockTemplateDataBuilder::new()
            .monero_seed(FixedByteArray::new())
            .taiji_block(block.try_into().unwrap())
            .taiji_miner_data(miner_data)
            .monero_difficulty(123456)
            .taiji_difficulty(12345);
        btdb.build().unwrap()
    }

    #[tokio::test]
    async fn test_block_template_repository() {
        let btr = BlockTemplateRepository::new();
        let hash1 = vec![1; 32];
        let hash2 = vec![2; 32];
        let hash3 = vec![3; 32];
        let block_template = create_block_template_data();
        btr.save(hash1.clone(), block_template.clone()).await;
        btr.save(hash2.clone(), block_template).await;
        assert!(btr.get(hash1.clone()).await.is_some());
        assert!(btr.get(hash2.clone()).await.is_some());
        assert!(btr.get(hash3.clone()).await.is_none());
        assert!(btr.remove(hash1.clone()).await.is_some());
        assert!(btr.get(hash1.clone()).await.is_none());
        assert!(btr.get(hash2.clone()).await.is_some());
        assert!(btr.get(hash3.clone()).await.is_none());
        btr.remove_outdated().await;
        assert!(btr.get(hash1).await.is_none());
        assert!(btr.get(hash2).await.is_none());
        assert!(btr.get(hash3).await.is_none());
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
            .taiji_block(block.clone().try_into().unwrap());
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
            .taiji_block(block.clone().try_into().unwrap())
            .taiji_miner_data(miner_data.clone());
        assert!(
            matches!(btdb.build(), Err(MmProxyError::MissingDataError(err)) if err == *"monero_difficulty not provided")
        );
        // With monero seed, block, miner data, monero difficulty
        let btdb = BlockTemplateDataBuilder::new()
            .monero_seed(FixedByteArray::new())
            .taiji_block(block.try_into().unwrap())
            .taiji_miner_data(miner_data)
            .monero_difficulty(123456);
        assert!(
            matches!(btdb.build(), Err(MmProxyError::MissingDataError(err)) if err == *"taiji_difficulty not provided")
        );
    }

    #[test]
    pub fn ok_block_template_data_builder() {
        let build = create_block_template_data();
        assert!(build.monero_seed.is_empty());
        assert_eq!(build.taiji_block.header.unwrap().version, 100);
        assert_eq!(build.taiji_miner_data.target_difficulty, 600000);
        assert_eq!(build.monero_difficulty, 123456);
        assert_eq!(build.taiji_difficulty, 12345);
    }
}
