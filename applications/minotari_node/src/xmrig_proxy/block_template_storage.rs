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

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use log::debug;
use tari_node_components::blocks::Block;
use tokio::sync::RwLock;

const LOG_TARGET: &str = "minotari::base_node::xmrig_proxy::storage";
const MAX_TEMPLATE_AGE: Duration = Duration::from_secs(20 * 60); // 20 minutes
const MAX_STORED_TEMPLATES: usize = 64;

struct TemplateEntry {
    block: Block,
    inserted_at: Instant,
}

/// Thread-safe in-memory store for block templates, keyed by the 32-byte mining hash.
///
/// Templates are automatically expired after [`MAX_TEMPLATE_AGE`].
#[derive(Clone)]
pub struct BlockTemplateStorage {
    inner: Arc<RwLock<HashMap<[u8; 32], TemplateEntry>>>,
}

impl BlockTemplateStorage {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Store a block template. If a template with the same key already exists it is replaced.
    pub async fn store(&self, key: [u8; 32], block: Block) {
        let mut map = self.inner.write().await;
        if !map.contains_key(&key) && map.len() >= MAX_STORED_TEMPLATES {
            if let Some(oldest_key) = map.iter().min_by_key(|(_, entry)| entry.inserted_at).map(|(k, _)| *k) {
                map.remove(&oldest_key);
            }
        }
        map.insert(key, TemplateEntry {
            block,
            inserted_at: Instant::now(),
        });
        debug!(target: LOG_TARGET, "Stored template, total templates={}", map.len());
    }

    /// Retrieve a clone of a block template by its mining hash key without removing it.
    pub async fn get(&self, key: &[u8; 32]) -> Option<Block> {
        let map = self.inner.read().await;
        map.get(key).map(|e| e.block.clone())
    }

    /// Retrieve and remove a block template by its mining hash key.
    pub async fn take(&self, key: &[u8; 32]) -> Option<Block> {
        let mut map = self.inner.write().await;
        map.remove(key).map(|e| e.block)
    }

    /// Look up a stored block template by `key`, set `nonce` on its header, submit it via `submit`,
    /// and remove the template from storage only when `submit` succeeds.
    pub async fn submit_with<F, Fut, T, E>(&self, key: &[u8; 32], nonce: u64, submit: F) -> Option<(u64, Result<T, E>)>
    where
        F: FnOnce(Block) -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
    {
        let mut block = self.get(key).await?;
        block.header.nonce = nonce;
        let block_height = block.header.height;
        let result = submit(block).await;
        if result.is_ok() {
            let _ = self.take(key).await;
        }
        Some((block_height, result))
    }

    /// Remove all templates older than [`MAX_TEMPLATE_AGE`].
    pub async fn remove_outdated(&self) {
        let now = Instant::now();
        let mut map = self.inner.write().await;
        let before = map.len();
        map.retain(|_, e| now.duration_since(e.inserted_at) < MAX_TEMPLATE_AGE);
        let removed = before.saturating_sub(map.len());
        if removed > 0 {
            debug!(target: LOG_TARGET, "Removed {removed} outdated templates");
        }
    }
}

impl Default for BlockTemplateStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use tari_common::configuration::Network;
    use tari_core::consensus::BaseNodeConsensusManager;

    use super::*;

    #[tokio::test]
    async fn failed_submission_does_not_evict_template_when_queried_via_get() {
        let rules = BaseNodeConsensusManager::builder(Network::LocalNet).build().unwrap();
        let genesis = rules.get_genesis_block().block().clone();
        let storage = BlockTemplateStorage::new();
        let key = [0x42u8; 32];

        storage.store(key, genesis.clone()).await;

        let (_, first_attempt) = storage
            .submit_with(&key, 1001, |_block| async { Err::<[u8; 32], &str>("low difficulty") })
            .await
            .expect("template must exist on first attempt");
        assert!(first_attempt.is_err());
        assert!(
            storage.get(&key).await.is_some(),
            "template must remain available after rejected share"
        );

        let (_, retry_attempt) = storage
            .submit_with(&key, 1002, |block| async move {
                assert_eq!(block.header.nonce, 1002);
                Ok::<[u8; 32], &str>([0x99u8; 32])
            })
            .await
            .expect("template must remain retryable after rejected share");
        assert!(retry_attempt.is_ok());
        assert!(
            storage.get(&key).await.is_none(),
            "template must be removed once submission succeeds"
        );
    }
}
