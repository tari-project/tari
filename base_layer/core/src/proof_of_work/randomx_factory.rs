// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, RwLock},
    time::Instant,
};

use log::*;
use randomx_rs::{RandomXCache, RandomXError, RandomXFlag, RandomXVM};

const LOG_TARGET: &str = "c::pow::randomx_factory";

#[derive(thiserror::Error, Debug)]
pub enum RandomXVMFactoryError {
    // The maximum number of VMs has been reached
    // MaxVMsReached,
    /// The RandomX VM failed to initialize
    // VMInitializationFailed,
    #[error(transparent)]
    RandomXError(#[from] RandomXError),
    #[error("Poisoned lock error")]
    PoisonedLockError,
}

/// The RandomX virtual machine instance used for to verify mining.
#[derive(Clone)]
pub struct RandomXVMInstance {
    // Note: If a cache and dataset (if assigned) allocated to the VM drops, the VM will crash.
    // The cache and dataset for the VM need to be stored together with it since they are not
    // mix and match.
    instance: Arc<RwLock<RandomXVM>>,
}

impl RandomXVMInstance {
    fn create(key: &[u8], flags: RandomXFlag) -> Result<Self, RandomXVMFactoryError> {
        let (flags, cache) = match RandomXCache::new(flags, key) {
            Ok(cache) => (flags, cache),
            Err(err) => {
                warn!(
                    target: LOG_TARGET,
                    "Error initializing RandomX cache with flags {:?}. {:?}. Fallback to default flags", flags, err
                );
                // This is informed by how RandomX falls back on any cache allocation failure
                // https://github.com/xmrig/xmrig/blob/02b2b87bb685ab83b132267aa3c2de0766f16b8b/src/crypto/rx/RxCache.cpp#L88
                let flags = RandomXFlag::FLAG_DEFAULT;
                let cache = RandomXCache::new(flags, key)?;
                (flags, cache)
            },
        };

        // Note: Memory required per VM in light mode is 256MB
        let vm = RandomXVM::new(flags, Some(cache), None)?;

        // Note: No dataset is initialized here because we want to run in light mode. Only a cache
        // is required by the VM for verification, giving it a dataset will only make the VM
        // consume more memory than necessary. Dataset is currently an optional value as it may be
        // useful at some point in future.

        // Note: RandomXFlag::FULL_MEM and RandomXFlag::LARGE_PAGES are incompatible with
        // light mode. These are not set by RandomX automatically even in fast mode.

        Ok(Self {
            #[allow(clippy::arc_with_non_send_sync)]
            instance: Arc::new(RwLock::new(vm)),
        })
    }

    /// Calculate the RandomX mining hash
    pub fn calculate_hash(&self, input: &[u8]) -> Result<Vec<u8>, RandomXVMFactoryError> {
        let lock = self
            .instance
            .write()
            .map_err(|_| RandomXVMFactoryError::PoisonedLockError)?;

        Ok(lock.calculate_hash(input)?)
    }
}

// This type should be Send and Sync since it is wrapped in an Arc RwLock, but
// for some reason Rust and clippy don't see it automatically.
#[allow(clippy::non_send_fields_in_send_ty)]
unsafe impl Send for RandomXVMInstance {}
unsafe impl Sync for RandomXVMInstance {}

/// The RandomX factory that manages the creation of RandomX VMs.
#[derive(Clone, Debug)]
pub struct RandomXFactory {
    // Thread safe impl of the inner impl
    inner: Arc<RwLock<RandomXFactoryInner>>,
}

impl Default for RandomXFactory {
    fn default() -> Self {
        Self::new(2)
    }
}

impl RandomXFactory {
    /// Create a new RandomX factory with the specified maximum number of VMs
    pub fn new(max_vms: usize) -> Self {
        Self {
            inner: Arc::new(RwLock::new(RandomXFactoryInner::new(max_vms))),
        }
    }

    /// Create a new RandomX VM instance with the specified key
    pub fn create(&self, key: &[u8]) -> Result<RandomXVMInstance, RandomXVMFactoryError> {
        let res;
        {
            let mut inner = self
                .inner
                .write()
                .map_err(|_| RandomXVMFactoryError::PoisonedLockError)?;
            res = inner.create(key)?;
        }
        Ok(res)
    }

    /// Get the number of VMs currently allocated
    pub fn get_count(&self) -> Result<usize, RandomXVMFactoryError> {
        let inner = self
            .inner
            .read()
            .map_err(|_| RandomXVMFactoryError::PoisonedLockError)?;
        Ok(inner.get_count())
    }

    /// Get the flags used to create the VMs
    pub fn get_flags(&self) -> Result<RandomXFlag, RandomXVMFactoryError> {
        let inner = self
            .inner
            .read()
            .map_err(|_| RandomXVMFactoryError::PoisonedLockError)?;
        Ok(inner.get_flags())
    }
}
struct RandomXFactoryInner {
    flags: RandomXFlag,
    vms: HashMap<Vec<u8>, (Instant, RandomXVMInstance)>,
    max_vms: usize,
}

impl RandomXFactoryInner {
    /// Create a new RandomXFactoryInner
    pub(crate) fn new(max_vms: usize) -> Self {
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

    /// Create a new RandomXVMInstance
    pub(crate) fn create(&mut self, key: &[u8]) -> Result<RandomXVMInstance, RandomXVMFactoryError> {
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

    /// Get the number of VMs currently allocated
    pub(crate) fn get_count(&self) -> usize {
        self.vms.len()
    }

    /// Get the flags used to create the VMs
    pub(crate) fn get_flags(&self) -> RandomXFlag {
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

    #[tokio::test(flavor = "multi_thread", worker_threads = 100)]
    async fn test_spawning_multiples() {
        let factory = RandomXFactory::new(1);

        let mut threads = vec![];
        for _ in 0..100 {
            let factory = factory.clone();
            threads.push(tokio::spawn(async move {
                let key = b"some-key";
                let vm = factory.create(&key[..]).unwrap();
                let preimage = b"hashme";
                let _hash = vm.calculate_hash(&preimage[..]).unwrap();
            }));
        }
        for t in threads {
            t.await.unwrap();
        }
    }
}
