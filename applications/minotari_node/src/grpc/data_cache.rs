// Copyright 2025. The Tari Project
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

use std::sync::Arc;

use tari_common_types::types::FixedHash;
use tari_core::blocks::NewBlockTemplate;
use tokio::sync::RwLock;

pub struct DataCache {
    inner_data_cache: Arc<RwLock<InnerDataCache>>,
}

impl DataCache {
    pub fn new() -> Self {
        Self {
            inner_data_cache: Arc::new(RwLock::new(InnerDataCache::default())),
        }
    }

    pub async fn get_randomx_estimated_hash_rate(&self, current_tip: &FixedHash) -> Option<u64> {
        let res = self.inner_data_cache.read().await.randomx_estimated_hash_rate;
        if res.1 == *current_tip {
            Some(res.0)
        } else {
            None
        }
    }

    pub async fn get_sha3x_estimated_hash_rate(&self, current_tip: &FixedHash) -> Option<u64> {
        let res = self.inner_data_cache.read().await.sha3x_estimated_hash_rate;
        if res.1 == *current_tip {
            Some(res.0)
        } else {
            None
        }
    }

    pub async fn set_randomx_estimated_hash_rate(&self, hash_rate: u64, current_tip: FixedHash) {
        self.inner_data_cache.write().await.randomx_estimated_hash_rate = (hash_rate, current_tip);
    }

    pub async fn set_sha3x_estimated_hash_rate(&self, hash_rate: u64, current_tip: FixedHash) {
        self.inner_data_cache.write().await.sha3x_estimated_hash_rate = (hash_rate, current_tip);
    }

    pub async fn get_randomx_new_block_template(&self, current_tip: &FixedHash) -> Option<NewBlockTemplate> {
        let res = &self.inner_data_cache.read().await.randomx_new_block_template;
        if res.1 == *current_tip {
            Some(res.0.clone())
        } else {
            None
        }
    }

    pub async fn get_sha3x_new_block_template(&self, current_tip: &FixedHash) -> Option<NewBlockTemplate> {
        let res = &self.inner_data_cache.read().await.sha3x_new_block_template;
        if res.1 == *current_tip {
            Some(res.0.clone())
        } else {
            None
        }
    }

    pub async fn set_randomx_new_block_template(&self, new_block_template: NewBlockTemplate, current_tip: FixedHash) {
        self.inner_data_cache.write().await.randomx_new_block_template = (new_block_template, current_tip);
    }

    pub async fn set_sha3x_new_block_template(&self, new_block_template: NewBlockTemplate, current_tip: FixedHash) {
        self.inner_data_cache.write().await.sha3x_new_block_template = (new_block_template, current_tip);
    }
}

struct InnerDataCache {
    pub randomx_estimated_hash_rate: (u64, FixedHash),
    pub sha3x_estimated_hash_rate: (u64, FixedHash),
    pub sha3x_new_block_template: (NewBlockTemplate, FixedHash),
    pub randomx_new_block_template: (NewBlockTemplate, FixedHash),
}
impl Default for InnerDataCache {
    fn default() -> Self {
        Self {
            randomx_estimated_hash_rate: (0, FixedHash::default()),
            sha3x_estimated_hash_rate: (0, FixedHash::default()),
            sha3x_new_block_template: (NewBlockTemplate::empty(), FixedHash::default()),
            randomx_new_block_template: (NewBlockTemplate::empty(), FixedHash::default()),
        }
    }
}
