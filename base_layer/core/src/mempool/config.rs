// Copyright 2020. The Tari Project
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

use crate::mempool::{
    consts,
    orphan_pool::OrphanPoolConfig,
    pending_pool::PendingPoolConfig,
    reorg_pool::ReorgPoolConfig,
    unconfirmed_pool::UnconfirmedPoolConfig,
};
use bitflags::_core::time::Duration;
use serde::{Deserialize, Serialize};
use tari_common::{configuration::seconds, NetworkConfigPath};

/// Configuration for the Mempool.
#[derive(Clone, Copy, Deserialize, Serialize)]
pub struct MempoolConfig {
    pub unconfirmed_pool: UnconfirmedPoolConfig,
    pub orphan_pool: OrphanPoolConfig,
    pub pending_pool: PendingPoolConfig,
    pub reorg_pool: ReorgPoolConfig,
}

impl Default for MempoolConfig {
    fn default() -> Self {
        Self {
            unconfirmed_pool: UnconfirmedPoolConfig::default(),
            orphan_pool: OrphanPoolConfig::default(),
            pending_pool: PendingPoolConfig::default(),
            reorg_pool: ReorgPoolConfig::default(),
        }
    }
}

impl NetworkConfigPath for MempoolConfig {
    fn main_key_prefix() -> &'static str {
        "mempool"
    }
}

/// Configuration for the MempoolService.
#[derive(Clone, Copy, Deserialize, Serialize)]
pub struct MempoolServiceConfig {
    /// The allocated waiting time for a request waiting for service responses from the Mempools of remote Base nodes.
    #[serde(with = "seconds")]
    pub request_timeout: Duration,
    /// Number of peers from which to initiate a sync. Once this many peers have successfully synced, this node will
    /// not initiate any more mempool syncs. Default: 2
    pub initial_sync_num_peers: usize,
    /// The maximum number of transactions to sync in a single sync session Default: 10_000
    pub initial_sync_max_transactions: usize,
}

impl Default for MempoolServiceConfig {
    fn default() -> Self {
        Self {
            request_timeout: consts::MEMPOOL_SERVICE_REQUEST_TIMEOUT,
            initial_sync_num_peers: 2,
            initial_sync_max_transactions: 10_000,
        }
    }
}

impl NetworkConfigPath for MempoolServiceConfig {
    fn main_key_prefix() -> &'static str {
        "mempool_service"
    }
}

#[cfg(test)]
mod test {
    use super::{
        consts::{
            MEMPOOL_PENDING_POOL_STORAGE_CAPACITY,
            MEMPOOL_REORG_POOL_CACHE_TTL,
            MEMPOOL_REORG_POOL_STORAGE_CAPACITY,
        },
        MempoolConfig,
    };
    use config::Config;
    use std::time::Duration;
    use tari_common::DefaultConfigLoader;

    #[test]
    pub fn test_mempool() {
        let mut config = Config::new();
        config
            .set("mempool.orphan_pool.tx_ttl", 70)
            .expect("Could not set 'mempool.orphan.tx_ttl'");
        config
            .set("mempool.unconfirmed_pool.storage_capacity", 3)
            .expect("Could not set ''");
        config
            .set("mempool.mainnet.pending_pool.storage_capacity", 100)
            .expect("Could not set 'pending_pool.storage_capacity'");
        config
            .set("mempool.mainnet.orphan_pool.tx_ttl", 99)
            .expect("Could not set 'orphan_pool.tx_ttl'");
        let my_config = MempoolConfig::load_from(&config).expect("Could not load configuration");
        // missing use_network value
        // [X] mempool.mainnet, [ ]  mempool, [X] Default = 4096
        assert_eq!(
            my_config.pending_pool.storage_capacity,
            MEMPOOL_PENDING_POOL_STORAGE_CAPACITY
        );
        // [X] mempool.mainnet, [X] mempool = 70s, [X] Default
        assert_eq!(my_config.orphan_pool.tx_ttl, Duration::from_secs(70));
        // [ ] mempool.mainnet, [X]  mempool = 3, [X] Default
        assert_eq!(my_config.unconfirmed_pool.storage_capacity, 3);
        // [ ] mempool.mainnet, [ ]  mempool, [X] Default = 512
        assert_eq!(
            my_config.reorg_pool.storage_capacity,
            MEMPOOL_REORG_POOL_STORAGE_CAPACITY
        );
        // [ ] mempool.mainnet, [ ]  mempool, [X] Default = 10s
        assert_eq!(my_config.reorg_pool.tx_ttl, MEMPOOL_REORG_POOL_CACHE_TTL);

        config
            .set("mempool.use_network", "mainnet")
            .expect("Could not set 'use_network'");
        // use_network = mainnet
        let my_config = MempoolConfig::load_from(&config).expect("Could not load configuration");
        // [X] mempool.mainnet = 100, [ ]  mempool, [X] Default
        assert_eq!(my_config.pending_pool.storage_capacity, 100);
        // [X] mempool.mainnet = 99s, [X] mempool, [X] Default
        assert_eq!(my_config.orphan_pool.tx_ttl, Duration::from_secs(99));
        // [ ] mempool.mainnet, [X]  mempool = 3, [X] Default
        assert_eq!(my_config.unconfirmed_pool.storage_capacity, 3);
        // [ ] mempool.mainnet, [ ]  mempool, [X] Default = 512
        assert_eq!(
            my_config.reorg_pool.storage_capacity,
            MEMPOOL_REORG_POOL_STORAGE_CAPACITY
        );
        // [ ] mempool.mainnet, [ ]  mempool, [X] Default = 10s
        assert_eq!(my_config.reorg_pool.tx_ttl, MEMPOOL_REORG_POOL_CACHE_TTL);

        config
            .set("mempool.use_network", "wrong_network")
            .expect("Could not set 'use_network'");
        assert!(MempoolConfig::load_from(&config).is_err());
    }
}
