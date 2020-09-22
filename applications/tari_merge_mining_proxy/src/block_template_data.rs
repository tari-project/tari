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
use monero::cryptonote::hash::Hash;
use std::collections::HashMap;
use crate::error::MmProxyError;
use tari_app_grpc::tari_rpc::{Block, MinerData};
use std::sync::{Arc};
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct BlockTemplateRepository {
    blocks: Arc<RwLock<HashMap<Vec<u8>, BlockTemplateData>>>
}


impl BlockTemplateRepository {
    pub fn new() -> Self {
        Self {
            blocks: Arc::new(RwLock::new(HashMap::new()))
        }
    }
    pub async fn get<T:AsRef<[u8]>>(&self, hash: T) -> Option<BlockTemplateData> {
        let b = self.blocks.read().await;
       b.get(hash.as_ref()).map(|bt| bt.clone())
    }

    pub async fn save<T:AsRef<[u8]>>(&mut self, hash: T, block_template: BlockTemplateData) {
        let mut b = self.blocks.write().await;
        b.insert(Vec::from(hash.as_ref()), block_template);
    }
    pub async fn remove<T:AsRef<[u8]>>(&mut self, hash: T) -> Option<BlockTemplateData> {
        let mut b = self.blocks.write().await;
        b.remove(hash.as_ref())
    }
}

#[derive(Clone,Debug)]
pub struct BlockTemplateData {
    pub monero_seed : String,
    pub tari_block: Block,
    pub tari_miner_data: MinerData,
    pub monero_difficulty: u64,
    pub tari_difficulty: u64
}

impl BlockTemplateData {

}

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
        let monero_seed = self.monero_seed.ok_or_else(|| MmProxyError::MissingDataError("monero_seed not provided".to_string()))?;
        let tari_block = self.tari_block.ok_or_else(|| MmProxyError::MissingDataError("block not provided".to_string()))?;
        let tari_miner_data = self.tari_miner_data.ok_or_else(|| MmProxyError::MissingDataError("miner_data not provided".to_string()))?;
        let monero_difficulty = self.monero_difficulty .ok_or_else(|| MmProxyError::MissingDataError("monero_difficulty not provided".to_string()))?;
        let tari_difficulty = self.tari_difficulty .ok_or_else(|| MmProxyError::MissingDataError("tari_difficulty not provided".to_string()))?;

        Ok(BlockTemplateData{
            monero_seed
        , tari_block, tari_miner_data: tari_miner_data, monero_difficulty,
    tari_difficulty})
    }
}
