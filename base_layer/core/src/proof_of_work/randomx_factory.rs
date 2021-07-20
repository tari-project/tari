use crate::proof_of_work::monero_rx::MergeMineError;
use log::*;
use randomx_rs::{RandomXCache, RandomXDataset, RandomXError, RandomXFlag, RandomXVM};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, RwLock},
    time::Instant,
};

const LOG_TARGET: &str = "c::pow::randomx_factory";

#[derive(Clone)]
pub struct RandomXVMInstance {
    // Note: If the cache and dataset drops, the vm will be wonky, so have to store all
    // three for now
    instance: Arc<Mutex<(RandomXVM, RandomXCache, RandomXDataset)>>,
    flags: RandomXFlag,
}

impl RandomXVMInstance {
    // Note: Can maybe even get more gains by creating a new VM and sharing the dataset and cache
    fn create(key: &[u8], flags: RandomXFlag) -> Result<Self, RandomXError> {
        let (flags, cache) = match RandomXCache::new(flags, key) {
            Ok(cache) => (flags, cache),
            Err(err) => {
                warn!(
                    target: LOG_TARGET,
                    "Error initializing randomx cache with flags {:?}. {}. Fallback to default flags", flags, err
                );
                // This is informed by how randomx falls back on any cache allocation failure
                // https://github.com/xmrig/xmrig/blob/02b2b87bb685ab83b132267aa3c2de0766f16b8b/src/crypto/rx/RxCache.cpp#L88
                let flags = RandomXFlag::FLAG_DEFAULT;
                let cache = RandomXCache::new(flags, key)?;
                (flags, cache)
            },
        };
        debug!(target: LOG_TARGET, "RandomX cache initialized with flags {:?}.", flags);

        let (flags, dataset) = match RandomXDataset::new(flags, &cache, 0) {
            Ok(dataset) => (flags, dataset),
            Err(err) => {
                warn!(
                    target: LOG_TARGET,
                    "Error initializing randomx dataset with flags {:?}. {}. Fallback to default flags", flags, err
                );
                let flags = RandomXFlag::FLAG_DEFAULT;
                let dataset = RandomXDataset::new(flags, &cache, 0)?;
                (flags, dataset)
            },
        };

        debug!(
            target: LOG_TARGET,
            "RandomX dataset initialized with flags {:?}.", flags
        );

        let (flags, vm) = match RandomXVM::new(flags, Some(&cache), Some(&dataset)) {
            Ok(vm) => (flags, vm),
            Err(err) => {
                warn!(
                    target: LOG_TARGET,
                    "Error initializing randomx VM with flags {:?}. {}. Fallback to default flags", flags, err
                );
                let flags = RandomXFlag::FLAG_DEFAULT;
                let vm = RandomXVM::new(flags, Some(&cache), Some(&dataset))?;
                (flags, vm)
            },
        };

        debug!(target: LOG_TARGET, "RandomX VM initialized with flags {:?}.", flags);

        Ok(Self {
            instance: Arc::new(Mutex::new((vm, cache, dataset))),
            flags,
        })
    }

    pub fn calculate_hash(&self, input: &[u8]) -> Result<Vec<u8>, RandomXError> {
        self.instance.lock().unwrap().0.calculate_hash(input)
    }
}

unsafe impl Send for RandomXVMInstance {}
unsafe impl Sync for RandomXVMInstance {}

// Thread safe impl of the inner impl
#[derive(Clone)]
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
            "RandomX factory started with {} max VMs and flags = {:?}", max_vms, flags
        );
        Self {
            flags,
            vms: Default::default(),
            max_vms,
        }
    }

    pub fn create(&mut self, key: &[u8]) -> Result<RandomXVMInstance, MergeMineError> {
        debug!(
            target: LOG_TARGET,
            "RandomXFactoryInner::create fn called with key: {:?}", key
        );

        debug!(target: LOG_TARGET, "Checking RandomX VMs cache");
        if let Some((instant, vm)) = self.vms.get_mut(key) {
            debug!(target: LOG_TARGET, "RandomX VM cache found, updating instant.");
            *instant = Instant::now();
            return Ok(vm.clone());
        }

        if self.vms.len() >= self.max_vms {
            debug!(target: LOG_TARGET, "RandomX max VMs exceeded");
            let mut oldest_value = Instant::now();
            let mut oldest_key = None;
            for (key, (instant, _)) in self.vms.iter() {
                if instant < &oldest_value {
                    oldest_key = Some(key.clone());
                    oldest_value = *instant;
                }
            }
            if let Some(key) = oldest_key {
                debug!(
                    target: LOG_TARGET,
                    "Removing RandomX VM from cache with oldest key: {:?}", key
                );
                self.vms.remove(&key);
            }
        }

        debug!(
            target: LOG_TARGET,
            "Creating a new RandomXVMInstance with flags: {:?}", self.flags
        );
        let vm = RandomXVMInstance::create(&key, self.flags)?;

        self.vms.insert(Vec::from(key), (Instant::now(), vm.clone()));

        Ok(vm)
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

    #[test]
    fn large_page_fallback() {
        // This only tests the fallback branch on platforms that do not support large pages (e.g. MacOS)
        let factory = RandomXFactory::new(1);
        factory.inner.write().unwrap().flags = RandomXFlag::FLAG_LARGE_PAGES;
        let key = "highly-imaginative-key-name";
        let vm = factory.create(key.as_bytes()).unwrap();
        vm.calculate_hash("hashme".as_bytes()).unwrap();
    }
}
