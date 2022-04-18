// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, RwLock},
    time::Instant,
};

use log::*;
use randomx_rs::{RandomXCache, RandomXDataset, RandomXError, RandomXFlag, RandomXVM};

use crate::proof_of_work::monero_rx::MergeMineError;

const LOG_TARGET: &str = "c::pow::randomx_factory";

struct RandomXVMInstanceInner {
    vm: RandomXVM,
    _cache: RandomXCache,
    _dataset: Option<RandomXDataset>,
}

#[derive(Clone)]
pub struct RandomXVMInstance {
    // Note: If a cache and dataset (if assigned) allocated to the VM drops, the VM will crash.
    // The cache and dataset for the VM need to be stored together with it since they are not
    // mix and match.
    instance: Arc<RwLock<RandomXVMInstanceInner>>,
}

impl RandomXVMInstance {
    fn create(key: &[u8], flags: RandomXFlag) -> Result<Self, RandomXError> {
        let (flags, cache) = match RandomXCache::new(flags, key) {
            Ok(cache) => (flags, cache),
            Err(err) => {
                warn!(
                    target: LOG_TARGET,
                    "Error initializing randomx cache with flags {:?}. {:?}. Fallback to default flags", flags, err
                );
                // This is informed by how RandomX falls back on any cache allocation failure
                // https://github.com/xmrig/xmrig/blob/02b2b87bb685ab83b132267aa3c2de0766f16b8b/src/crypto/rx/RxCache.cpp#L88
                let flags = RandomXFlag::FLAG_DEFAULT;
                let cache = RandomXCache::new(flags, key)?;
                (flags, cache)
            },
        };

        // Note: Memory required per VM in light mode is 256MB
        let vm = RandomXVM::new(flags, Some(&cache), None)?;

        // Note: No dataset is initialized here because we want to run in light mode. Only a cache
        // is required by the VM for verification, giving it a dataset will only make the VM
        // consume more memory than necessary. Dataset is currently an optional value as it may be
        // useful at some point in future.

        // Note: RandomXFlag::FULL_MEM and RandomXFlag::LARGE_PAGES are incompatible with
        // light mode. These are not set by RandomX automatically even in fast mode.

        Ok(Self {
            instance: Arc::new(RwLock::new(RandomXVMInstanceInner {
                vm,
                _cache: cache,
                _dataset: None,
            })),
        })
    }

    pub fn calculate_hash(&self, input: &[u8]) -> Result<Vec<u8>, RandomXError> {
        self.instance.read().unwrap().vm.calculate_hash(input)
    }
}

// TODO: Find a better way of marking this Send and Sync
// This type should be Send and Sync since it is wrapped in an Arc RwLock, but
// for some reason Rust and clippy don't see it automatically.
#[allow(clippy::non_send_fields_in_send_ty)]
unsafe impl Send for RandomXVMInstance {}
unsafe impl Sync for RandomXVMInstance {}

// Thread safe impl of the inner impl
#[derive(Clone, Debug)]
pub struct RandomXFactory {
    inner: Arc<RwLock<RandomXFactoryInner>>,
}

impl Default for RandomXFactory {
    fn default() -> Self {
        Self::new(2)
    }
}

impl RandomXFactory {
    pub fn new(max_vms: usize) -> Self {
        Self {
            inner: Arc::new(RwLock::new(RandomXFactoryInner::new(max_vms))),
        }
    }

    pub fn create(&self, key: &[u8]) -> Result<RandomXVMInstance, MergeMineError> {
        let res;
        {
            let mut inner = self.inner.write().unwrap();
            res = inner.create(key)?;
        }
        Ok(res)
    }

    pub fn get_count(&self) -> usize {
        let inner = self.inner.read().unwrap();
        inner.get_count()
    }

    pub fn get_flags(&self) -> RandomXFlag {
        let inner = self.inner.read().unwrap();
        inner.get_flags()
    }
}
struct RandomXFactoryInner {
    flags: RandomXFlag,
    vms: HashMap<Vec<u8>, (Instant, RandomXVMInstance)>,
    max_vms: usize,
}

impl RandomXFactoryInner {
    pub fn new(max_vms: usize) -> Self {
        let flags = RandomXFlag::get_recommended_flags();
        debug!(
            target: LOG_TARGET,
            "RandomX factory started with {} max VMs and recommended flags = {:?}", max_vms, flags
        );
        Self {
            flags,
            vms: Default::default(),
            max_vms,
        }
    }

    pub fn create(&mut self, key: &[u8]) -> Result<RandomXVMInstance, MergeMineError> {
        if let Some(entry) = self.vms.get_mut(key) {
            let vm = entry.1.clone();
            entry.0 = Instant::now();
            return Ok(vm);
        }

        if self.vms.len() >= self.max_vms {
            let mut oldest_value = Instant::now();
            let mut oldest_key = None;
            for (k, v) in &self.vms {
                if v.0 < oldest_value {
                    oldest_key = Some(k.clone());
                    oldest_value = v.0;
                }
            }
            if let Some(k) = oldest_key {
                self.vms.remove(&k);
            }
        }

        let vm = RandomXVMInstance::create(key, self.flags)?;

        self.vms.insert(Vec::from(key), (Instant::now(), vm.clone()));

        Ok(vm)
    }

    pub fn get_count(&self) -> usize {
        self.vms.len()
    }

    pub fn get_flags(&self) -> RandomXFlag {
        self.flags
    }
}

impl fmt::Debug for RandomXFactoryInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RandomXFactory")
            .field("flags", &self.flags)
            .field("max_vms", &self.max_vms)
            .finish()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn basic_initialization_and_hash() {
        let factory = RandomXFactory::new(2);

        let key = b"some-key";
        let vm = factory.create(&key[..]).unwrap();
        let preimage = b"hashme";
        let hash1 = vm.calculate_hash(&preimage[..]).unwrap();
        let vm = factory.create(&key[..]).unwrap();
        assert_eq!(vm.calculate_hash(&preimage[..]).unwrap(), hash1);

        let key = b"another-key";
        let vm = factory.create(&key[..]).unwrap();
        assert_ne!(vm.calculate_hash(&preimage[..]).unwrap(), hash1);
    }
}
