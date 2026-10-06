// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, Condvar, Mutex, RwLock, Weak},
    time::Instant,
};

use log::*;
use once_cell::sync::OnceCell;
use randomx_rs::{RandomXCache, RandomXDataset, RandomXError, RandomXFlag, RandomXVM};

const LOG_TARGET: &str = "c::pow::randomx_factory";

#[derive(thiserror::Error, Debug, Clone)]
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
    fn create(
        key: &[u8],
        flags: RandomXFlag,
        cache: Option<RandomXCache>,
        dataset: Option<RandomXDataset>,
    ) -> Result<Self, RandomXVMFactoryError> {
        // Note: Memory required per VM in light mode is 256MB
        // Note: RandomXFlag::FULL_MEM and RandomXFlag::LARGE_PAGES are incompatible with
        // light mode. These are not set by RandomX automatically even in fast mode.
        let (flags, cache) = match cache {
            Some(c) => (flags, c),
            None => match RandomXCache::new(flags, key) {
                Ok(cache) => (flags, cache),
                Err(err) => {
                    warn!(
                        target: LOG_TARGET,
                        "Error initializing RandomX cache with flags {flags:?}. {err:?}. Fallback to default flags"
                    );
                    // This is informed by how RandomX falls back on any cache allocation failure
                    // https://github.com/xmrig/xmrig/blob/02b2b87bb685ab83b132267aa3c2de0766f16b8b/src/crypto/rx/RxCache.cpp#L88
                    let flags = RandomXFlag::FLAG_DEFAULT;
                    let cache = RandomXCache::new(flags, key)?;
                    (flags, cache)
                },
            },
        };
        let vm = RandomXVM::new(flags, Some(cache), dataset)?;

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

/// The maximum number of RandomX caches that may be built at the same time. Each build allocates ~256MB, so this
/// bounds the transient memory used when many unknown keys arrive at once.
const MAX_CONCURRENT_BUILDS: usize = 2;

/// The RandomX factory that manages the creation of RandomX VMs.
#[derive(Clone, Debug)]
pub struct RandomXFactory {
    // Thread safe impl of the inner impl
    inner: Arc<RandomXFactoryInner>,
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
            inner: Arc::new(RandomXFactoryInner::new(max_vms)),
        }
    }

    pub fn new_with_flags(max_vms: usize, flags: RandomXFlag) -> Self {
        Self {
            inner: Arc::new(RandomXFactoryInner::new_with_flags(max_vms, flags)),
        }
    }

    /// Create a new RandomX VM instance with the specified key
    pub fn create(
        &self,
        key: &[u8],
        cache: Option<RandomXCache>,
        dataset: Option<RandomXDataset>,
    ) -> Result<RandomXVMInstance, RandomXVMFactoryError> {
        self.inner.create(key, cache, dataset)
    }

    /// Get the number of built VMs currently cached. Builds still in flight are not counted.
    pub fn get_count(&self) -> Result<usize, RandomXVMFactoryError> {
        Ok(self.inner.get_count())
    }

    /// Get the flags used to create the VMs
    pub fn get_flags(&self) -> Result<RandomXFlag, RandomXVMFactoryError> {
        Ok(self.inner.get_flags())
    }
}

// The cell holds the outcome of the one build for a key, so a failure is shared by all waiters too
type VmCell = Arc<OnceCell<Result<RandomXVMInstance, RandomXVMFactoryError>>>;

#[derive(Default)]
struct VmMaps {
    // The LRU cache. Entries whose cell is still empty are builds in flight; they count towards `max_vms`.
    lru: HashMap<Vec<u8>, (Instant, VmCell)>,
    // Cells that are being built or waited on, so a caller can still join a build whose LRU entry was evicted
    in_flight: HashMap<Vec<u8>, Weak<OnceCell<Result<RandomXVMInstance, RandomXVMFactoryError>>>>,
}

struct RandomXFactoryInner {
    flags: RandomXFlag,
    // Each key has its own cell, so the map lock is only held for lookups and inserts, never while a VM is built.
    vms: Mutex<VmMaps>,
    max_vms: usize,
    // Counting semaphore limiting concurrent builds to `MAX_CONCURRENT_BUILDS`
    builds_in_flight: Mutex<usize>,
    build_finished: Condvar,
    #[cfg(test)]
    test_hooks: test::TestHooks,
}

impl RandomXFactoryInner {
    /// Create a new RandomXFactoryInner
    pub(crate) fn new(max_vms: usize) -> Self {
        let flags = RandomXFlag::get_recommended_flags();
        Self::new_with_flags(max_vms, flags)
    }

    pub(crate) fn new_with_flags(max_vms: usize, flags: RandomXFlag) -> Self {
        debug!(
            target: LOG_TARGET,
            "RandomX factory started with {max_vms} max VMs and recommended flags = {flags:?}"
        );
        Self {
            flags,
            vms: Default::default(),
            max_vms,
            builds_in_flight: Mutex::new(0),
            build_finished: Condvar::new(),
            #[cfg(test)]
            test_hooks: Default::default(),
        }
    }

    /// Get or create the RandomXVMInstance for `key`. Cache hits never wait on a build, and builds for different keys
    /// never wait on each other (other than for the build cap).
    pub(crate) fn create(
        &self,
        key: &[u8],
        cache: Option<RandomXCache>,
        dataset: Option<RandomXDataset>,
    ) -> Result<RandomXVMInstance, RandomXVMFactoryError> {
        let cell = self.get_or_insert_cell(key);
        // Concurrent callers for the same key wait here for the one build and all get its result, success or error.
        let result = cell.get_or_init(|| {
            let _permit = self.acquire_build_permit();
            #[cfg(test)]
            self.test_hooks.before_build(key)?;
            // The cache and VM are not Send, so they are built on this thread and only shared once wrapped.
            RandomXVMInstance::create(key, self.flags, cache, dataset)
        });

        let mut vms = self.vms.lock().unwrap_or_else(|e| e.into_inner());
        // The build is done, so later callers find the cell in the LRU (or start afresh) rather than via `in_flight`
        if vms.in_flight.get(key).is_some_and(|w| w.as_ptr() == Arc::as_ptr(&cell)) {
            vms.in_flight.remove(key);
        }
        match result.clone() {
            Ok(vm) => {
                // If this key was evicted while it was building, cache the finished VM again as the newest entry. A
                // different cell for the key belongs to a newer caller and is left alone.
                if !vms.lru.contains_key(key) {
                    self.insert_newest(&mut vms.lru, key, cell);
                }
                Ok(vm)
            },
            Err(err) => {
                // Remove the failed entry (unless it was already replaced or evicted) so the next request retries.
                // Every caller holding this cell does this; only the first one still finds it in the map.
                if vms.lru.get(key).is_some_and(|(_, c)| Arc::ptr_eq(c, &cell)) {
                    vms.lru.remove(key);
                }
                Err(err)
            },
        }
    }

    fn get_or_insert_cell(&self, key: &[u8]) -> VmCell {
        let mut vms = self.vms.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = vms.lru.get_mut(key) {
            entry.0 = Instant::now();
            return entry.1.clone();
        }

        // A build for this key may still be running after its LRU entry was evicted; join it instead of building the
        // same key twice. A cell left empty by a panicked build is joined too, and its next caller retries the build.
        if let Some(cell) = vms.in_flight.get(key).and_then(Weak::upgrade) {
            self.insert_newest(&mut vms.lru, key, cell.clone());
            return cell;
        }

        // Only keys that are being built or waited on are kept; drop entries nobody holds any more (for example
        // after a panicked build) so the map cannot grow without bound.
        vms.in_flight.retain(|_, w| w.strong_count() > 0);
        let cell = VmCell::default();
        vms.in_flight.insert(Vec::from(key), Arc::downgrade(&cell));
        self.insert_newest(&mut vms.lru, key, cell.clone());
        cell
    }

    /// Insert `cell` for `key` as the newest LRU entry, evicting the oldest entry first if the map is full. Evicting
    /// only drops the map's reference: callers already holding the cell still get their VM, a later caller can join
    /// the build through `in_flight`, and a successful build puts its cell back in the map.
    fn insert_newest(&self, vms: &mut HashMap<Vec<u8>, (Instant, VmCell)>, key: &[u8], cell: VmCell) {
        if vms.len() >= self.max_vms &&
            let Some(oldest_key) = vms.iter().min_by_key(|(_, (i, _))| *i).map(|(k, _)| k.clone())
        {
            vms.remove(&oldest_key);
        }
        vms.insert(Vec::from(key), (Instant::now(), cell));
    }

    fn acquire_build_permit(&self) -> BuildPermit<'_> {
        let mut in_flight = self.builds_in_flight.lock().unwrap_or_else(|e| e.into_inner());
        while *in_flight >= MAX_CONCURRENT_BUILDS {
            in_flight = self.build_finished.wait(in_flight).unwrap_or_else(|e| e.into_inner());
        }
        *in_flight = in_flight.saturating_add(1);
        #[cfg(test)]
        self.test_hooks.record_builds_in_flight(*in_flight);
        BuildPermit { factory: self }
    }

    /// Get the number of built VMs currently cached. Builds still in flight are not counted.
    pub(crate) fn get_count(&self) -> usize {
        let vms = self.vms.lock().unwrap_or_else(|e| e.into_inner());
        vms.lru
            .values()
            .filter(|(_, cell)| matches!(cell.get(), Some(Ok(_))))
            .count()
    }

    /// Get the flags used to create the VMs
    pub(crate) fn get_flags(&self) -> RandomXFlag {
        self.flags
    }
}

/// Releases a build slot when dropped, including when the build errors or panics
struct BuildPermit<'a> {
    factory: &'a RandomXFactoryInner,
}

impl Drop for BuildPermit<'_> {
    fn drop(&mut self) {
        let mut in_flight = self.factory.builds_in_flight.lock().unwrap_or_else(|e| e.into_inner());
        *in_flight = in_flight.saturating_sub(1);
        self.factory.build_finished.notify_one();
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
    use std::{
        sync::{
            Barrier,
            atomic::{AtomicUsize, Ordering},
            mpsc,
        },
        thread,
        time::Duration,
    };

    use super::*;

    type BuildHook = Arc<dyn Fn(&[u8]) -> Result<(), RandomXVMFactoryError> + Send + Sync>;

    /// Test-only hooks into the build path of the factory
    #[derive(Default)]
    pub(super) struct TestHooks {
        // Called inside the build, after a build permit was acquired. Returning an error fails the build.
        hook: Mutex<Option<BuildHook>>,
        max_builds_in_flight: AtomicUsize,
    }

    impl TestHooks {
        pub(super) fn before_build(&self, key: &[u8]) -> Result<(), RandomXVMFactoryError> {
            let hook = self.hook.lock().unwrap().clone();
            match hook {
                Some(hook) => hook(key),
                None => Ok(()),
            }
        }

        pub(super) fn record_builds_in_flight(&self, in_flight: usize) {
            self.max_builds_in_flight.fetch_max(in_flight, Ordering::SeqCst);
        }
    }

    fn set_hook<F>(factory: &RandomXFactory, hook: F)
    where F: Fn(&[u8]) -> Result<(), RandomXVMFactoryError> + Send + Sync + 'static {
        *factory.inner.test_hooks.hook.lock().unwrap() = Some(Arc::new(hook));
    }

    fn clear_hook(factory: &RandomXFactory) {
        *factory.inner.test_hooks.hook.lock().unwrap() = None;
    }

    fn injected_error() -> RandomXVMFactoryError {
        RandomXVMFactoryError::RandomXError(RandomXError::Other("injected build failure".to_string()))
    }

    #[test]
    fn basic_initialization_and_hash() {
        let factory = RandomXFactory::new(2);

        let key = b"some-key";
        let vm = factory.create(&key[..], None, None).unwrap();
        let preimage = b"hashme";
        let hash1 = vm.calculate_hash(&preimage[..]).unwrap();
        let vm = factory.create(&key[..], None, None).unwrap();
        assert_eq!(vm.calculate_hash(&preimage[..]).unwrap(), hash1);

        let key = b"another-key";
        let vm = factory.create(&key[..], None, None).unwrap();
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
                let vm = factory.create(&key[..], None, None).unwrap();
                let preimage = b"hashme";
                let _hash = vm.calculate_hash(&preimage[..]).unwrap();
            }));
        }
        for t in threads {
            t.await.unwrap();
        }
    }

    #[test]
    fn cache_hit_is_not_blocked_by_builds() {
        let factory = RandomXFactory::new(5);
        factory.create(b"key-a", None, None).unwrap();

        // Builds for B and C block in the hook, holding both build permits, until released
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let release_rx = Mutex::new(release_rx);
        set_hook(&factory, move |_key| {
            entered_tx.send(()).unwrap();
            release_rx.lock().unwrap().recv().unwrap();
            Err(injected_error())
        });
        let builders = [b"key-b", b"key-c"]
            .into_iter()
            .map(|key| {
                let factory = factory.clone();
                thread::spawn(move || factory.create(key, None, None))
            })
            .collect::<Vec<_>>();
        entered_rx.recv().unwrap();
        entered_rx.recv().unwrap();

        // Both builds are in flight and the build cap is reached, yet the hit still returns
        let vm = factory.create(b"key-a", None, None).unwrap();
        vm.calculate_hash(b"hashme").unwrap();
        assert!(builders.iter().all(|b| !b.is_finished()));

        release_tx.send(()).unwrap();
        release_tx.send(()).unwrap();
        for builder in builders {
            assert!(builder.join().unwrap().is_err());
        }
    }

    #[test]
    fn one_build_per_key() {
        const CALLERS: usize = 8;
        let factory = RandomXFactory::new(5);
        let builds = Arc::new(AtomicUsize::new(0));
        let builds_hook = builds.clone();
        set_hook(&factory, move |_key| {
            builds_hook.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });

        let barrier = Arc::new(Barrier::new(CALLERS));
        let callers = (0..CALLERS)
            .map(|_| {
                let factory = factory.clone();
                let barrier = barrier.clone();
                thread::spawn(move || {
                    barrier.wait();
                    factory.create(b"shared-key", None, None).unwrap()
                })
            })
            .collect::<Vec<_>>();
        let vms = callers.into_iter().map(|c| c.join().unwrap()).collect::<Vec<_>>();

        assert_eq!(builds.load(Ordering::SeqCst), 1);
        let first = vms.first().unwrap();
        assert!(vms.iter().all(|vm| Arc::ptr_eq(&vm.instance, &first.instance)));
        assert_eq!(factory.get_count().unwrap(), 1);
    }

    #[test]
    fn failed_build_does_not_poison_key() {
        const CALLERS: usize = 8;
        let factory = RandomXFactory::new(5);
        let builds = Arc::new(AtomicUsize::new(0));
        let builds_hook = builds.clone();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let release_rx = Mutex::new(release_rx);
        set_hook(&factory, move |_key| {
            builds_hook.fetch_add(1, Ordering::SeqCst);
            entered_tx.send(()).unwrap();
            release_rx.lock().unwrap().recv().unwrap();
            Err(injected_error())
        });

        let callers = (0..CALLERS)
            .map(|_| {
                let factory = factory.clone();
                thread::spawn(move || factory.create(b"failing-key", None, None))
            })
            .collect::<Vec<_>>();
        entered_rx.recv().unwrap();

        // Fail the build only once every caller holds the cell: the map, the CALLERS and our own reference
        let cell = factory
            .inner
            .vms
            .lock()
            .unwrap()
            .lru
            .get(b"failing-key".as_slice())
            .unwrap()
            .1
            .clone();
        while Arc::strong_count(&cell) < CALLERS + 2 {
            thread::sleep(Duration::from_millis(10));
        }
        release_tx.send(()).unwrap();

        for caller in callers {
            assert!(caller.join().unwrap().is_err());
        }
        assert_eq!(builds.load(Ordering::SeqCst), 1);
        assert_eq!(factory.get_count().unwrap(), 0);

        clear_hook(&factory);
        let vm = factory.create(b"failing-key", None, None).unwrap();
        vm.calculate_hash(b"hashme").unwrap();
        assert_eq!(factory.get_count().unwrap(), 1);
    }

    #[test]
    fn concurrent_builds_are_capped() {
        const KEYS: usize = 4;
        let factory = RandomXFactory::new(KEYS);
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let release_rx = Mutex::new(release_rx);
        set_hook(&factory, move |_key| {
            entered_tx.send(()).unwrap();
            release_rx.lock().unwrap().recv().unwrap();
            Err(injected_error())
        });

        let builders = (0..KEYS)
            .map(|i| {
                let factory = factory.clone();
                thread::spawn(move || factory.create(format!("key-{i}").as_bytes(), None, None))
            })
            .collect::<Vec<_>>();

        // Exactly MAX_CONCURRENT_BUILDS builds get in; the rest wait for a permit
        for _ in 0..MAX_CONCURRENT_BUILDS {
            entered_rx.recv().unwrap();
        }
        assert!(entered_rx.recv_timeout(Duration::from_millis(500)).is_err());
        // All four builds are in the map, but none has a VM yet
        assert_eq!(factory.inner.vms.lock().unwrap().lru.len(), KEYS);
        assert_eq!(factory.get_count().unwrap(), 0);

        for _ in 0..KEYS {
            release_tx.send(()).unwrap();
        }
        for builder in builders {
            assert!(builder.join().unwrap().is_err());
        }
        assert_eq!(
            factory.inner.test_hooks.max_builds_in_flight.load(Ordering::SeqCst),
            MAX_CONCURRENT_BUILDS
        );
        assert_eq!(factory.get_count().unwrap(), 0);
    }

    #[test]
    fn lru_bounds_vm_count() {
        let factory = RandomXFactory::new(2);
        factory.create(b"key-1", None, None).unwrap();
        factory.create(b"key-2", None, None).unwrap();
        factory.create(b"key-3", None, None).unwrap();

        assert_eq!(factory.get_count().unwrap(), 2);
        let vms = factory.inner.vms.lock().unwrap();
        assert!(!vms.lru.contains_key(b"key-1".as_slice()));
        assert!(vms.lru.contains_key(b"key-2".as_slice()));
        assert!(vms.lru.contains_key(b"key-3".as_slice()));
    }

    #[test]
    fn vm_evicted_while_building_is_cached_again() {
        let factory = RandomXFactory::new(2);
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_k_tx, release_k_rx) = mpsc::channel::<()>();
        let (release_others_tx, release_others_rx) = mpsc::channel::<()>();
        let release_k_rx = Mutex::new(release_k_rx);
        let release_others_rx = Mutex::new(release_others_rx);
        set_hook(&factory, move |key| {
            entered_tx.send(key.to_vec()).unwrap();
            if key == b"key-k" {
                release_k_rx.lock().unwrap().recv().unwrap();
                Ok(())
            } else {
                release_others_rx.lock().unwrap().recv().unwrap();
                Err(injected_error())
            }
        });

        let spawn_create = |key: &'static [u8]| {
            let factory = factory.clone();
            thread::spawn(move || factory.create(key, None, None))
        };
        // K holds one build permit and blocks in the hook
        let builder_k = spawn_create(b"key-k");
        assert_eq!(entered_rx.recv().unwrap(), b"key-k");
        // Two more keys fill the map and evict K: one holds the other permit, the other waits for a permit
        let builder_1 = spawn_create(b"key-1");
        assert_eq!(entered_rx.recv().unwrap(), b"key-1");
        let builder_2 = spawn_create(b"key-2");
        while !factory.inner.vms.lock().unwrap().lru.contains_key(b"key-2".as_slice()) {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(!factory.inner.vms.lock().unwrap().lru.contains_key(b"key-k".as_slice()));

        // K finishes and is cached again as the newest entry, evicting the oldest (key-1)
        release_k_tx.send(()).unwrap();
        let vm = builder_k.join().unwrap().unwrap();
        vm.calculate_hash(b"hashme").unwrap();
        {
            let vms = factory.inner.vms.lock().unwrap();
            assert!(vms.lru.contains_key(b"key-k".as_slice()));
            assert!(!vms.lru.contains_key(b"key-1".as_slice()));
            assert!(vms.lru.len() <= 2);
        }

        // A later caller for K gets the cached VM without another build
        let vm_again = factory.create(b"key-k", None, None).unwrap();
        assert!(Arc::ptr_eq(&vm.instance, &vm_again.instance));

        release_others_tx.send(()).unwrap();
        release_others_tx.send(()).unwrap();
        assert!(builder_1.join().unwrap().is_err());
        assert!(builder_2.join().unwrap().is_err());
        assert!(factory.get_count().unwrap() <= 2);
        assert!(factory.inner.vms.lock().unwrap().lru.contains_key(b"key-k".as_slice()));
    }

    #[test]
    fn evicted_build_is_joined_not_restarted() {
        let factory = RandomXFactory::new(2);
        let k_builds = Arc::new(AtomicUsize::new(0));
        let k_builds_hook = k_builds.clone();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_k_tx, release_k_rx) = mpsc::channel::<()>();
        let (release_others_tx, release_others_rx) = mpsc::channel::<()>();
        let release_k_rx = Mutex::new(release_k_rx);
        let release_others_rx = Mutex::new(release_others_rx);
        set_hook(&factory, move |key| {
            entered_tx.send(key.to_vec()).unwrap();
            if key == b"key-k" {
                // Only the first build of K blocks, so a second build would show up in the counter, not hang
                if k_builds_hook.fetch_add(1, Ordering::SeqCst) == 0 {
                    release_k_rx.lock().unwrap().recv().unwrap();
                }
                Ok(())
            } else {
                release_others_rx.lock().unwrap().recv().unwrap();
                Err(injected_error())
            }
        });

        let spawn_create = |key: &'static [u8]| {
            let factory = factory.clone();
            thread::spawn(move || factory.create(key, None, None))
        };
        // K blocks in the hook holding one build permit; key-1 holds the other and key-2 waits, evicting K
        let first_k = spawn_create(b"key-k");
        assert_eq!(entered_rx.recv().unwrap(), b"key-k");
        let k_cell = factory
            .inner
            .vms
            .lock()
            .unwrap()
            .lru
            .get(b"key-k".as_slice())
            .unwrap()
            .1
            .clone();
        let builder_1 = spawn_create(b"key-1");
        assert_eq!(entered_rx.recv().unwrap(), b"key-1");
        let builder_2 = spawn_create(b"key-2");
        while !factory.inner.vms.lock().unwrap().lru.contains_key(b"key-2".as_slice()) {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(!factory.inner.vms.lock().unwrap().lru.contains_key(b"key-k".as_slice()));

        // A second caller for K joins the running build: K's own cell goes back into the LRU
        let second_k = spawn_create(b"key-k");
        loop {
            let cell = factory
                .inner
                .vms
                .lock()
                .unwrap()
                .lru
                .get(b"key-k".as_slice())
                .map(|(_, c)| c.clone());
            if let Some(cell) = cell {
                assert!(
                    Arc::ptr_eq(&cell, &k_cell),
                    "the second caller started a new build of K"
                );
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }

        release_k_tx.send(()).unwrap();
        let vm_1 = first_k.join().unwrap().unwrap();
        let vm_2 = second_k.join().unwrap().unwrap();
        assert!(Arc::ptr_eq(&vm_1.instance, &vm_2.instance));
        assert_eq!(k_builds.load(Ordering::SeqCst), 1);
        assert!(factory.inner.vms.lock().unwrap().lru.contains_key(b"key-k".as_slice()));
        assert!(
            factory
                .inner
                .vms
                .lock()
                .unwrap()
                .in_flight
                .get(b"key-k".as_slice())
                .is_none()
        );

        release_others_tx.send(()).unwrap();
        release_others_tx.send(()).unwrap();
        assert!(builder_1.join().unwrap().is_err());
        assert!(builder_2.join().unwrap().is_err());
        assert_eq!(factory.get_count().unwrap(), 1);
        assert!(factory.inner.vms.lock().unwrap().in_flight.is_empty());
    }
}
