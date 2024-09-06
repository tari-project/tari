//  Copyright 2024. The Tari Project
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

pub const LOG_TARGET: &str = "minotari::randomx_miner::shared_dataset";

use std::sync::RwLock;

use log::debug;
use randomx_rs::{RandomXCache, RandomXDataset, RandomXFlag};

use crate::error::DatasetError;

pub struct Dataset {
    pub identifier: String,
    pub dataset: RandomXDataset,
    pub cache: RandomXCache,
}

impl Dataset {
    pub fn new(identifier: String, dataset: RandomXDataset, cache: RandomXCache) -> Self {
        Self {
            identifier,
            dataset,
            cache,
        }
    }
}

// This allows us to share the randomx dataset across multiple threads.
#[derive(Default)]
pub struct SharedDataset {
    pub inner: RwLock<Option<Dataset>>,
}
unsafe impl Send for SharedDataset {}
unsafe impl Sync for SharedDataset {}

impl SharedDataset {
    pub fn fetch_or_create_dataset(
        &self,
        key: String,
        flags: RandomXFlag,
        thread_number: usize,
    ) -> Result<(RandomXDataset, RandomXCache), DatasetError> {
        {
            let read_guard = self.inner.read().map_err(|e| DatasetError::ReadLock(e.to_string()))?;
            if let Some(existing_dataset) = read_guard.as_ref() {
                if existing_dataset.identifier == key {
                    debug!(target: LOG_TARGET, "Thread {} found existing dataset with seed {}", thread_number, &key);
                    return Ok((existing_dataset.dataset.clone(), existing_dataset.cache.clone()));
                }
            }
        } // Read lock is released here

        {
            let mut write_guard = self.inner.write().map_err(|e| DatasetError::WriteLock(e.to_string()))?;

            // Double-check the condition after acquiring the write lock to avoid race conditions.
            if let Some(existing_dataset) = &*write_guard {
                if existing_dataset.identifier == key {
                    debug!(target: LOG_TARGET, "Thread {} found existing dataset with seed {} found after waiting for write lock", thread_number, &key);
                    return Ok((existing_dataset.dataset.clone(), existing_dataset.cache.clone()));
                }
            }

            let cache = RandomXCache::new(flags, &hex::decode(key.clone())?)?;
            let new_dataset = RandomXDataset::new(flags, cache.clone(), 0)?;

            *write_guard = Some(Dataset::new(key.clone(), new_dataset, cache));
            debug!(target: LOG_TARGET, "Thread {} created new dataset with seed {}", thread_number, key);
        }

        // Return the updated or created dataset
        {
            let read_guard = self.inner.read().map_err(|e| DatasetError::ReadLock(e.to_string()))?;
            if let Some(existing_dataset) = read_guard.as_ref() {
                return Ok((existing_dataset.dataset.clone(), existing_dataset.cache.clone()));
            };
        }

        Err(DatasetError::DatasetNotFound)
    }
}
