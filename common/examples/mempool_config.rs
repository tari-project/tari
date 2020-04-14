use serde::{Deserialize, Serialize};
use std::time::Duration;
use tari_common::{ConfigurationError, DefaultConfigLoader, NetworkConfigPath};

const UNCONFIRMED_STORAGE_CAPACITY: usize = 1024;
const ORPHAN_STORAGE_CAPACITY: usize = 2048;
const PENDING_STORAGE_CAPACITY: usize = 4096;
const REORG_STORAGE_CAPACITY: usize = 512;
const UNCONFIRMED_TX_SKIP: usize = 2;
const ORPHAN_TX_TTL: Duration = Duration::from_secs(2);
const REORG_TX_TTL: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct UnconfirmedPoolConfig {
    /// The maximum number of transactions that can be stored in the Unconfirmed Transaction pool
    #[serde(rename = "unconfirmed_pool_storage_capacity")]
    pub storage_capacity: usize,
    /// The maximum number of transactions that can be skipped when compiling a set of highest priority transactions,
    /// skipping over large transactions are performed in an attempt to fit more transactions into the remaining space.
    #[serde(rename = "weight_tx_skip_count")]
    pub weight_tx_skip_count: usize,
}
impl Default for UnconfirmedPoolConfig {
    fn default() -> Self {
        Self {
            storage_capacity: UNCONFIRMED_STORAGE_CAPACITY,
            weight_tx_skip_count: UNCONFIRMED_TX_SKIP,
        }
    }
}
/// Configuration for the OrphanPool
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct OrphanPoolConfig {
    /// The maximum number of transactions that can be stored in the Orphan pool
    #[serde(rename = "orphan_pool_storage_capacity")]
    pub storage_capacity: usize,
    /// The Time-to-live for each stored transaction
    #[serde(rename = "orphan_tx_ttl", with = "seconds")]
    pub tx_ttl: Duration,
}
impl Default for OrphanPoolConfig {
    fn default() -> Self {
        Self {
            storage_capacity: ORPHAN_STORAGE_CAPACITY,
            tx_ttl: ORPHAN_TX_TTL,
        }
    }
}
/// Configuration for the PendingPool.
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct PendingPoolConfig {
    /// The maximum number of transactions that can be stored in the Pending pool.
    #[serde(rename = "pending_pool_storage_capacity")]
    pub storage_capacity: usize,
}
impl Default for PendingPoolConfig {
    fn default() -> Self {
        Self { storage_capacity: PENDING_STORAGE_CAPACITY }
    }
}

/// Configuration for the ReorgPool
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct ReorgPoolConfig {
    /// The maximum number of transactions that can be stored in the ReorgPool
    #[serde(rename = "reorg_pool_storage_capacity")]
    pub storage_capacity: usize,
    /// The Time-to-live for each stored transaction
    #[serde(rename = "reorg_tx_ttl", with = "seconds")]
    pub tx_ttl: Duration,
}
impl Default for ReorgPoolConfig {
    fn default() -> Self {
        Self {
            storage_capacity: REORG_STORAGE_CAPACITY,
            tx_ttl: REORG_TX_TTL,
        }
    }
}

/// Configuration for the Mempool.
#[derive(Clone, Copy, Default, Serialize, Deserialize)]
pub struct MempoolConfig {
    #[serde(flatten)]
    pub unconfirmed_pool_config: UnconfirmedPoolConfig,
    #[serde(flatten)]
    pub orphan_pool_config: OrphanPoolConfig,
    #[serde(flatten)]
    pub pending_pool_config: PendingPoolConfig,
    #[serde(flatten)]
    pub reorg_pool_config: ReorgPoolConfig,
}
impl NetworkConfigPath for MempoolConfig {
    fn main_key_prefix() -> &'static str {
        "mempool"
    }
}

fn main() -> Result<(), ConfigurationError> {
    let mut config = config::Config::new();

    config.set("mempool.orphan_tx_ttl", 70)?;
    config.set("mempool.unconfirmed_pool_storage_capacity", 3)?;
    config.set("mempool.mainnet.pending_pool_storage_capacity", 100)?;
    config.set("mempool.mainnet.orphan_tx_ttl", 99)?;
    let my_config = MempoolConfig::load_from(&config)?;
    // no use_network value
    // [X] mempool.mainnet, [ ]  mempool, [X] Default = 4096
    assert_eq!(my_config.pending_pool_config.storage_capacity, PENDING_STORAGE_CAPACITY);
    // [X] mempool.mainnet, [X] mempool = 70s, [X] Default
    assert_eq!(my_config.orphan_pool_config.tx_ttl, Duration::from_secs(70));
    // [ ] mempool.mainnet, [X]  mempool = 3, [X] Default
    assert_eq!(my_config.unconfirmed_pool_config.storage_capacity, 3);
    // [ ] mempool.mainnet, [ ]  mempool, [X] Default = 512
    assert_eq!(my_config.reorg_pool_config.storage_capacity, REORG_STORAGE_CAPACITY);
    // [ ] mempool.mainnet, [ ]  mempool, [X] Default = 10s
    assert_eq!(my_config.reorg_pool_config.tx_ttl, REORG_TX_TTL);

    config.set("mempool.use_network", "mainnet")?;
    // use_network = mainnet
    let my_config = MempoolConfig::load_from(&config)?;
    // [X] mempool.mainnet = 100, [ ]  mempool, [X] Default
    assert_eq!(my_config.pending_pool_config.storage_capacity, 100);
    // [X] mempool.mainnet = 99s, [X] mempool, [X] Default
    assert_eq!(my_config.orphan_pool_config.tx_ttl, Duration::from_secs(99));
    // [ ] mempool.mainnet, [X]  mempool = 3, [X] Default
    assert_eq!(my_config.unconfirmed_pool_config.storage_capacity, 3);
    // [ ] mempool.mainnet, [ ]  mempool, [X] Default = 512
    assert_eq!(my_config.reorg_pool_config.storage_capacity, REORG_STORAGE_CAPACITY);
    // [ ] mempool.mainnet, [ ]  mempool, [X] Default = 10s
    assert_eq!(my_config.reorg_pool_config.tx_ttl, REORG_TX_TTL);

    config.set("mempool.use_network", "wrong_network")?;
    assert!(MempoolConfig::load_from(&config).is_err());

    Ok(())
}

mod seconds {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where D: Deserializer<'de> {
        Ok(Duration::from_secs(u64::deserialize(deserializer)?))
    }
    pub fn serialize<S>(duration: &Duration, s: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        s.serialize_u64(duration.as_secs())
    }
}
