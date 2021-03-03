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
use crate::error::MmProxyError;
use chrono::{self, DateTime, Duration, Utc};
use std::{collections::HashMap, sync::Arc};
use tari_app_grpc::tari_rpc::{Block, MinerData};
use tokio::sync::RwLock;
use tracing::trace;

pub const LOG_TARGET: &str = "tari_mm_proxy::xmrig";

#[derive(Debug, Clone)]
pub struct BlockTemplateRepository {
    blocks: Arc<RwLock<HashMap<Vec<u8>, BlockTemplateRepositoryItem>>>,
}

#[derive(Debug, Clone)]
pub struct BlockTemplateRepositoryItem {
    pub data: BlockTemplateData,
    datetime: DateTime<Utc>,
}

impl BlockTemplateRepositoryItem {
    pub fn new(block_template: BlockTemplateData) -> Self {
        Self {
            data: block_template,
            datetime: Utc::now(),
        }
    }

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

    pub async fn get<T: AsRef<[u8]>>(&self, hash: T) -> Option<BlockTemplateData> {
        trace!(
            target: LOG_TARGET,
            "Retrieving blocktemplate with merge mining hash: {:?}",
            hex::encode(hash.as_ref())
        );
        let b = self.blocks.read().await;
        b.get(hash.as_ref()).map(|item| item.data.clone())
    }

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

    pub async fn remove_outdated(&self) {
        trace!(target: LOG_TARGET, "Removing outdated blocktemplates");
        let mut b = self.blocks.write().await;
        let threshold = Utc::now() - Duration::minutes(20);
        *b = b.drain().filter(|(_, i)| i.datetime() >= threshold).collect();
    }

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

#[derive(Clone, Debug)]
pub struct BlockTemplateData {
    pub monero_seed: String,
    pub tari_block: Block,
    pub tari_miner_data: MinerData,
    pub monero_difficulty: u64,
    pub tari_difficulty: u64,
}

impl BlockTemplateData {}

#[derive(Default)]
pub struct BlockTemplateDataBuilder {
    monero_seed: Option<String>,
    tari_block: Option<Block>,
    tari_miner_data: Option<MinerData>,
    monero_difficulty: Option<u64>,
    tari_difficulty: Option<u64>,
}

impl BlockTemplateDataBuilder {
    pub fn monero_seed(mut self, monero_seed: String) -> Self {
        self.monero_seed = Some(monero_seed);
        self
    }

    pub fn tari_block(mut self, tari_block: Block) -> Self {
        self.tari_block = Some(tari_block);
        self
    }

    pub fn tari_miner_data(mut self, miner_data: MinerData) -> Self {
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

        Ok(BlockTemplateData {
            monero_seed,
            tari_block,
            tari_miner_data,
            monero_difficulty,
            tari_difficulty,
        })
    }
}
