use crate::proof_of_work::monero_rx::MergeMineError;
use randomx_rs::{RandomXCache, RandomXDataset, RandomXError, RandomXFlag, RandomXVM};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, RwLock},
    time::Instant,
};

const MAX_VMS: usize = 5;

#[derive(Default)]
pub struct RandomXConfig {
    pub use_large_pages: bool,
}

impl From<&RandomXConfig> for RandomXFlag {
    fn from(source: &RandomXConfig) -> Self {
        let mut result = RandomXFlag::get_recommended_flags();
        if source.use_large_pages {
            result |= RandomXFlag::FLAG_LARGE_PAGES
        }
        result
    }
}

#[derive(Clone)]
pub struct RandomXVMInstance {
    // Note: If the cache and dataset drops, the vm will be wonky, so have to store all
    // three for now
    instance: Arc<Mutex<(RandomXVM, RandomXCache, RandomXDataset)>>,
}

impl RandomXVMInstance {
    // Note: Can maybe even get more gains by creating a new VM and sharing the dataset and cache
    pub fn new(key: &[u8]) -> Result<Self, RandomXError> {
        let flags = RandomXFlag::get_recommended_flags();
        let cache = RandomXCache::new(flags, key)?;
        let dataset = RandomXDataset::new(flags, &cache, 0)?;
        let vm = RandomXVM::new(flags, Some(&cache), Some(&dataset))?;

        Ok(Self {
            instance: Arc::new(Mutex::new((vm, cache, dataset))),
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
        Self::new(RandomXConfig::default())
    }
}

impl RandomXFactory {
    pub fn new(config: RandomXConfig) -> Self {
        Self {
            inner: Arc::new(RwLock::new(RandomXFactoryInner::new(config))),
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
    // config: RandomXConfig,
    vms: HashMap<Vec<u8>, (Instant, RandomXVMInstance)>,
}

impl RandomXFactoryInner {
    pub fn new(_config: RandomXConfig) -> Self {
        Self {
            // config,
            vms: Default::default(),
        }
    }

    pub fn create(&mut self, key: &[u8]) -> Result<RandomXVMInstance, MergeMineError> {
        if let Some(entry) = self.vms.get_mut(key) {
            let vm = entry.1.clone();
            entry.0 = Instant::now();
            return Ok(vm);
        }

        if self.vms.len() + 1 > MAX_VMS {
            let oldest_value = Instant::now();
            let mut oldest_key = None;
            for (k, v) in self.vms.iter() {
                if v.0 < oldest_value {
                    oldest_key = Some(k.clone());
                }
            }
            if let Some(k) = oldest_key {
                self.vms.remove(&k);
            }
        }

        // TODO: put config in.

        let vm = RandomXVMInstance::new(&key)?;

        self.vms.insert(Vec::from(key), (Instant::now(), vm.clone()));

        Ok(vm)
    }
}
